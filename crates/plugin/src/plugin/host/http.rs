use bytes::Bytes;
use domain::download::DownloadReporter;
use http_body::{Body, Frame, SizeHint};
use http_body_util::BodyExt;
use reqwest::{Client, redirect::Policy};
use runtime::AbortOnDrop;
use std::{
    fmt,
    future::Future,
    ops::Deref,
    pin::Pin,
    sync::{Arc, Mutex, PoisonError},
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, mpsc},
    time::{Instant, Sleep},
};
use wasmtime_wasi_http::{Error, RequestOptions, WasiBody, WasiHttpHooks};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);
const READ_AHEAD: usize = 4 * 1024 * 1024;

pub fn http_client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .connect_timeout(DEFAULT_TIMEOUT)
        .tcp_keepalive(TCP_KEEPALIVE)
        .build()
        .expect("the plugin HTTP client should be built")
}

type IoFuture = Box<dyn Future<Output = Result<(), Error>> + Send>;

type SendRequestFuture =
    Box<dyn Future<Output = Result<(http::Response<WasiBody>, IoFuture), Error>> + Send>;

pub struct OutboundHttp {
    client: Client,
    allowed_hosts: AllowedHosts,
    reporter: Option<Arc<dyn DownloadReporter>>,
}

impl OutboundHttp {
    pub fn new(
        client: Client,
        allowed_hosts: Arc<[String]>,
        reporter: Option<Arc<dyn DownloadReporter>>,
    ) -> Self {
        Self {
            client,
            allowed_hosts: AllowedHosts::new(allowed_hosts),
            reporter,
        }
    }
}

impl WasiHttpHooks for OutboundHttp {
    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        options: Option<RequestOptions>,
        _fut: IoFuture,
    ) -> SendRequestFuture {
        if !self.allowed_hosts.is_allowed(request.uri().host()) {
            return Box::new(async { Err(Error::HttpRequestDenied) });
        }

        Box::new(dispatch(
            self.client.clone(),
            request,
            options,
            self.reporter.clone(),
        ))
    }
}

async fn dispatch(
    client: Client,
    request: http::Request<WasiBody>,
    options: Option<RequestOptions>,
    reporter: Option<Arc<dyn DownloadReporter>>,
) -> Result<(http::Response<WasiBody>, IoFuture), Error> {
    let Timeouts {
        head,
        between_bytes,
    } = Timeouts::new(options);
    let (parts, body) = request.into_parts();

    let captured = CapturedError::default();
    let body = reqwest::Body::wrap(SyncBody::new(body, captured.clone()));

    let request = reqwest::Request::try_from(http::Request::from_parts(parts, body))
        .map_err(|_| Error::HttpRequestUriInvalid)?;

    let response = tokio::time::timeout(head, client.execute(request))
        .await
        .map_err(|_| Error::ConnectionTimeout)?
        .map_err(|err| captured.take().unwrap_or_else(|| request_error(err)))?;

    let response = http::Response::<reqwest::Body>::from(response).map(move |body| {
        let body = body
            .map_err(move |err| captured.take().unwrap_or_else(|| response_error(err)))
            .boxed_unsync();
        response_body(body, between_bytes, reporter)
    });

    Ok((response, Box::new(async { Ok(()) })))
}

fn response_body(
    body: WasiBody,
    idle_timeout: Duration,
    reporter: Option<Arc<dyn DownloadReporter>>,
) -> WasiBody {
    let body = IdleTimeoutBody::new(body, idle_timeout);
    match reporter {
        Some(reporter) => ReadAheadBody::new(body, reporter).boxed_unsync(),
        None => body.boxed_unsync(),
    }
}

struct Timeouts {
    head: Duration,
    between_bytes: Duration,
}

impl Timeouts {
    fn new(options: Option<RequestOptions>) -> Self {
        let field = |pick: fn(&RequestOptions) -> Option<Duration>| {
            options.as_ref().and_then(pick).unwrap_or(DEFAULT_TIMEOUT)
        };

        Self {
            head: field(|o| o.connect_timeout).saturating_add(field(|o| o.first_byte_timeout)),
            between_bytes: field(|o| o.between_bytes_timeout),
        }
    }
}

/// Fails if no response data arrives within the timeout.
///
/// Only time spent waiting on the network counts, so a slow consumer never
/// causes a timeout.
struct IdleTimeoutBody {
    inner: WasiBody,
    idle_timeout: Duration,
    deadline: Pin<Box<Sleep>>,
    waiting: bool,
}

impl IdleTimeoutBody {
    fn new(inner: WasiBody, idle_timeout: Duration) -> Self {
        Self {
            inner,
            idle_timeout,
            deadline: Box::pin(tokio::time::sleep(idle_timeout)),
            waiting: false,
        }
    }
}

impl Body for IdleTimeoutBody {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Error>>> {
        if let Poll::Ready(frame) = Pin::new(&mut self.inner).poll_frame(cx) {
            self.waiting = false;
            return Poll::Ready(frame);
        }

        if !self.waiting {
            self.waiting = true;
            let deadline = Instant::now() + self.idle_timeout;
            self.deadline.as_mut().reset(deadline);
        }

        match self.deadline.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(Some(Err(Error::ConnectionReadTimeout))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

struct ReadAheadBody {
    frames: mpsc::UnboundedReceiver<Received>,
    _receiving: AbortOnDrop<()>,
}

type Received = (Result<Frame<Bytes>, Error>, Option<OwnedSemaphorePermit>);

impl ReadAheadBody {
    fn new(mut body: IdleTimeoutBody, reporter: Arc<dyn DownloadReporter>) -> Self {
        let (sender, frames) = mpsc::unbounded_channel();
        let room = Arc::new(Semaphore::new(READ_AHEAD));

        let receiving = tokio::spawn(async move {
            while let Some(frame) = body.frame().await {
                let len = frame
                    .as_ref()
                    .ok()
                    .and_then(Frame::data_ref)
                    .map_or(0, Bytes::len);
                reporter.received(len as u64);

                let wanted = len.min(READ_AHEAD) as u32;
                let permit = room.clone().acquire_many_owned(wanted).await.ok();
                let failed = frame.is_err();
                if sender.send((frame, permit)).is_err() || failed {
                    break;
                }
            }
        });

        Self {
            frames,
            _receiving: AbortOnDrop(receiving),
        }
    }
}

impl Body for ReadAheadBody {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Error>>> {
        self.frames
            .poll_recv(cx)
            .map(|received| received.map(|(frame, _room)| frame))
    }
}

/// Wraps the guest body so `reqwest` can use it.
///
/// This exists only because [`WasiBody`] is not [`Sync`] and `reqwest`
/// requires it to be.
struct SyncBody {
    body: Mutex<WasiBody>,
    captured: CapturedError,
}

impl SyncBody {
    fn new(body: WasiBody, captured: CapturedError) -> Self {
        Self {
            body: Mutex::new(body),
            captured,
        }
    }

    fn lock(&self) -> impl Deref<Target = WasiBody> + '_ {
        self.body.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Body for SyncBody {
    type Data = Bytes;
    type Error = GuestBodyError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, GuestBodyError>>> {
        let this = self.get_mut();
        let body = this.body.get_mut().unwrap_or_else(PoisonError::into_inner);

        Pin::new(body).poll_frame(cx).map(|frame| {
            frame.map(|result| {
                result.map_err(|error| {
                    this.captured.set(error);
                    GuestBodyError
                })
            })
        })
    }

    fn is_end_stream(&self) -> bool {
        self.lock().is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.lock().size_hint()
    }
}

/// Stores an error from the guest body so [`dispatch`] can return it later.
#[derive(Clone, Default)]
struct CapturedError(Arc<Mutex<Option<Error>>>);

impl CapturedError {
    fn set(&self, error: Error) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(error);
    }

    fn take(&self) -> Option<Error> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).take()
    }
}

/// Placeholder error passed to `reqwest`.
#[derive(Debug)]
struct GuestBodyError;

impl fmt::Display for GuestBodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("guest request body error")
    }
}

impl std::error::Error for GuestBodyError {}

fn request_error(err: reqwest::Error) -> Error {
    if err.is_timeout() {
        return Error::ConnectionTimeout;
    }

    transport_cause(&err).unwrap_or_else(|| {
        if err.is_connect() {
            Error::ConnectionRefused
        } else {
            Error::InternalError(Some(err.to_string()))
        }
    })
}

fn response_error(err: reqwest::Error) -> Error {
    if err.is_timeout() {
        return Error::HttpResponseTimeout;
    }

    transport_cause(&err).unwrap_or_else(|| Error::InternalError(Some(err.to_string())))
}

fn transport_cause(err: &reqwest::Error) -> Option<Error> {
    let mut source = std::error::Error::source(err);

    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>() {
            return Some(io_error(io));
        }
        if let Some(mapped) = cause.downcast_ref::<hyper::Error>().and_then(hyper_error) {
            return Some(mapped);
        }
        source = cause.source();
    }

    None
}

/// Maps a [`std::io::Error`] to a wasi-http [`Error`].
fn io_error(err: &std::io::Error) -> Error {
    use std::io::ErrorKind;

    match err.kind() {
        ErrorKind::ConnectionRefused => Error::ConnectionRefused,
        ErrorKind::HostUnreachable | ErrorKind::NetworkUnreachable | ErrorKind::NetworkDown => {
            Error::DestinationUnavailable
        }
        ErrorKind::ConnectionReset
        | ErrorKind::ConnectionAborted
        | ErrorKind::BrokenPipe
        | ErrorKind::UnexpectedEof => Error::ConnectionTerminated,
        ErrorKind::TimedOut => Error::ConnectionTimeout,
        ErrorKind::AddrNotAvailable => Error::DnsError {
            rcode: None,
            info_code: None,
        },
        _ => Error::InternalError(Some(err.to_string())),
    }
}

fn hyper_error(err: &hyper::Error) -> Option<Error> {
    if err.is_incomplete_message() {
        Some(Error::HttpResponseIncomplete)
    } else if err.is_parse_too_large() {
        Some(Error::HttpResponseHeaderSectionSize(None))
    } else if err.is_parse() || err.is_parse_status() {
        Some(Error::HttpProtocolError)
    } else if err.is_body_write_aborted() {
        Some(Error::ConnectionTerminated)
    } else {
        None
    }
}

#[derive(Default)]
struct AllowedHosts(Box<[String]>);

impl AllowedHosts {
    fn new(hosts: Arc<[String]>) -> Self {
        Self(hosts.iter().map(|host| host.to_ascii_lowercase()).collect())
    }

    fn is_allowed(&self, host: Option<&str>) -> bool {
        let Some(host) = host else {
            return false;
        };
        let host = host.to_ascii_lowercase();

        self.0.iter().any(|pattern| matches_pattern(pattern, &host))
    }
}

fn matches_pattern(pattern: &str, host: &str) -> bool {
    let mut chunks = pattern.split('*');
    let prefix = chunks.next().unwrap_or_default();

    let Some(mut rest) = host.strip_prefix(prefix) else {
        return false;
    };

    let chunks: Vec<&str> = chunks.collect();
    let Some((suffix, inner)) = chunks.split_last() else {
        return rest.is_empty();
    };

    for chunk in inner {
        let Some(at) = rest.find(chunk) else {
            return false;
        };
        rest = &rest[at + chunk.len()..];
    }

    rest.len() >= suffix.len() && rest.ends_with(suffix)
}

#[cfg(test)]
mod tests {
    use super::{
        AllowedHosts, Error, OutboundHttp, READ_AHEAD, WasiBody, http_client, response_body,
    };
    use bytes::Bytes;
    use domain::download::{DownloadPhase, DownloadReporter};
    use futures::{future::join, stream};
    use http::{Method, Request, Response, StatusCode, header::CONTENT_LENGTH};
    use http_body::Frame;
    use http_body_util::{BodyExt, Empty, Full, StreamBody};
    use std::{net::SocketAddr, sync::Arc, time::Duration};
    use tokio::{
        sync::{mpsc, watch},
        time::{Instant, sleep_until, timeout},
    };
    use wasmtime_wasi_http::WasiHttpHooks;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

    #[track_caller]
    fn check_allowed(patterns: &[&str], host: &str, expected: bool) {
        let hosts = patterns.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        let allowed = AllowedHosts::new(hosts.as_slice().into()).is_allowed(Some(host));

        assert_eq!(allowed, expected, "patterns {patterns:?} against {host}");
    }

    #[test]
    fn the_allowlist_accepts_matching_hosts() {
        check_allowed(&["*"], "api.legacygames.com", true);
        check_allowed(&["*"], "localhost", true);
        check_allowed(&["*"], "a.b.c.d.example.com", true);
        check_allowed(&["api.legacygames.com"], "api.legacygames.com", true);
        check_allowed(&["API.LegacyGames.com"], "api.legacygames.com", true);
        check_allowed(&["*.example.com"], "api.example.com", true);
        check_allowed(&["*.example.com"], "a.b.example.com", true);
        check_allowed(&["api.*"], "api.legacygames.com", true);
        check_allowed(&["api.*"], "api.example.org", true);
        check_allowed(&["blabla.*.com"], "blabla.foo.com", true);
        check_allowed(&["blabla.*.com"], "blabla.foo.bar.com", true);
        check_allowed(&["example.com", "*.example.com"], "example.com", true);
        check_allowed(&["example.com", "*.example.com"], "api.example.com", true);
    }

    #[test]
    fn the_allowlist_rejects_every_other_host() {
        check_allowed(&[], "api.legacygames.com", false);
        check_allowed(&["api.legacygames.com"], "legacygames.com", false);
        check_allowed(&["api.legacygames.com"], "evil.com", false);
        check_allowed(
            &["api.legacygames.com"],
            "api.legacygames.com.evil.com",
            false,
        );
        check_allowed(&["*.example.com"], "example.com", false);
        check_allowed(&["*.example.com"], "example.com.evil.com", false);
        check_allowed(&["*.example.com"], "notexample.com", false);
        check_allowed(&["api.*"], "cdn.legacygames.com", false);
        check_allowed(&["blabla.*.com"], "blabla.foo.org", false);
        check_allowed(&["blabla.*.com"], "other.foo.com", false);
        check_allowed(&["example.com", "*.example.com"], "evil.com", false);
    }

    fn empty_body() -> WasiBody {
        Empty::<Bytes>::new().map_err(Error::from).boxed_unsync()
    }

    fn text_body(text: &'static str) -> WasiBody {
        Full::new(Bytes::from_static(text.as_bytes()))
            .map_err(Error::from)
            .boxed_unsync()
    }

    async fn send(allowed: &str, request: Request<WasiBody>) -> Result<(StatusCode, Bytes), Error> {
        let mut hooks = OutboundHttp::new(http_client(), [allowed.to_string()][..].into(), None);

        let sending = hooks.send_request(request, None, Box::new(async { Ok(()) }));
        let (response, _io) = Box::into_pin(sending).await?;

        let status = response.status();
        let body = response.into_body().collect().await?.to_bytes();

        Ok((status, body))
    }

    async fn ok_server() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .mount(&server)
            .await;

        server
    }

    async fn unused_addr() -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

        listener.local_addr().unwrap()
    }

    #[tokio::test]
    async fn an_allowed_request_reaches_the_server_and_returns_its_response() {
        let server = ok_server().await;
        let request = Request::builder()
            .uri(format!("{}/", server.uri()))
            .body(empty_body())
            .unwrap();

        let (status, body) = send("127.0.0.1", request).await.unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body, "ok");
    }

    #[tokio::test]
    async fn request_bodies_reach_the_server_with_their_length() {
        let server = ok_server().await;
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("{}/", server.uri()))
            .body(text_body("hello"))
            .unwrap();

        send("127.0.0.1", request).await.unwrap();

        let requests = server.received_requests().await.unwrap();
        let [request] = requests.as_slice() else {
            panic!("expected exactly one request, got {}", requests.len());
        };

        assert_eq!(request.method, Method::POST);
        assert_eq!(request.body, b"hello");
        assert_eq!(request.headers[CONTENT_LENGTH], "5");
    }

    #[tokio::test]
    async fn a_request_to_a_denied_host_is_refused_before_it_connects() {
        let server = ok_server().await;
        let request = Request::builder()
            .uri(format!("{}/", server.uri()))
            .body(empty_body())
            .unwrap();

        let error = send("example.com", request).await.unwrap_err();

        assert!(matches!(error, Error::HttpRequestDenied), "{error:?}");
        assert!(
            server.received_requests().await.unwrap().is_empty(),
            "the request must never reach the server",
        );
    }

    #[tokio::test]
    async fn a_refused_connection_is_reported() {
        let request = Request::builder()
            .uri(format!("http://{}/", unused_addr().await))
            .body(empty_body())
            .unwrap();

        let error = send("127.0.0.1", request).await.unwrap_err();

        assert!(matches!(error, Error::ConnectionRefused), "{error:?}");
    }

    struct Recorder {
        received: watch::Sender<u64>,
    }

    impl Recorder {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                received: watch::channel(0).0,
            })
        }

        fn received(&self) -> usize {
            *self.received.borrow() as usize
        }

        async fn wait_until_received(&self, bytes: usize) {
            let mut received = self.received.subscribe();
            let waiting = received.wait_for(|&received| received as usize >= bytes);
            timeout(Duration::from_secs(10), waiting)
                .await
                .unwrap_or_else(|_| panic!("only {} of {bytes} bytes arrived", self.received()))
                .unwrap();
        }
    }

    impl DownloadReporter for Recorder {
        fn report(&self, _phase: DownloadPhase) {}

        fn received(&self, bytes: u64) {
            self.received.send_modify(|received| *received += bytes);
        }

        fn written(&self, _bytes: u64) {}
    }

    async fn serve_game(len: usize) -> (MockServer, Request<WasiBody>) {
        let server = MockServer::start().await;
        Mock::given(path("/game.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![7; len]))
            .mount(&server)
            .await;
        let request = Request::builder()
            .uri(format!("{}/game.zip", server.uri()))
            .body(empty_body())
            .unwrap();

        (server, request)
    }

    async fn download(request: Request<WasiBody>, recorder: &Arc<Recorder>) -> Response<WasiBody> {
        let allowed = ["127.0.0.1".to_string()][..].into();
        let mut hooks = OutboundHttp::new(http_client(), allowed, Some(recorder.clone()));

        let sending = hooks.send_request(request, None, Box::new(async { Ok(()) }));
        let (response, _io) = Box::into_pin(sending).await.unwrap();

        response
    }

    #[tokio::test]
    async fn a_download_is_received_ahead_of_the_plugin_up_to_a_limit() {
        const SLACK: usize = READ_AHEAD / 4;
        let (_server, request) = serve_game(4 * READ_AHEAD).await;
        let recorder = Recorder::new();

        let response = download(request, &recorder).await;
        recorder.wait_until_received(READ_AHEAD).await;

        let ahead = recorder.received();
        assert!(
            ahead <= READ_AHEAD + SLACK,
            "{ahead} bytes were received ahead"
        );
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.len(), 4 * READ_AHEAD);
        assert_eq!(recorder.received(), 4 * READ_AHEAD);
    }

    #[tokio::test]
    async fn the_response_bytes_of_a_download_are_counted() {
        let (_server, request) = serve_game(300_000).await;
        let recorder = Recorder::new();

        let response = download(request, &recorder).await;

        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.len(), 300_000);
        assert_eq!(recorder.received(), 300_000);
    }

    const IDLE_TIMEOUT: Duration = Duration::from_secs(1);

    #[derive(Clone, Copy, Debug)]
    enum Mode {
        Request,
        Download,
    }

    /// The server sends a frame at each time in `arrivals` and the plugin reads
    /// the next one at each time in `reads`, or right away once those run out.
    /// Times are in milliseconds. Returns how many frames the plugin read.
    async fn stream_response(mode: Mode, arrivals: &[u64], reads: &[u64]) -> Result<usize, Error> {
        let start = Instant::now();
        let at = |ms: u64| sleep_until(start + Duration::from_millis(ms));

        let (sender, receiver) = mpsc::unbounded_channel();
        let network = stream::unfold(receiver, |mut receiver| async move {
            let data = receiver.recv().await?;
            Some((Ok(Frame::data(data)), receiver))
        });
        let reporter = match mode {
            Mode::Request => None,
            Mode::Download => Some(Recorder::new() as Arc<dyn DownloadReporter>),
        };
        let mut body = response_body(
            StreamBody::new(network).boxed_unsync(),
            IDLE_TIMEOUT,
            reporter,
        );

        let server = async move {
            let data = Bytes::from(vec![7; READ_AHEAD]);
            for &arrival in arrivals {
                at(arrival).await;
                let _ = sender.send(data.clone());
            }
        };
        let plugin = async move {
            let mut read = 0;
            loop {
                if let Some(&ms) = reads.get(read) {
                    at(ms).await;
                }
                match body.frame().await {
                    Some(frame) => frame?,
                    None => return Ok(read),
                };
                read += 1;
            }
        };

        join(server, plugin).await.1
    }

    #[tokio::test(start_paused = true)]
    async fn a_response_that_stops_arriving_times_out() {
        for mode in [Mode::Request, Mode::Download] {
            let outcome = stream_response(mode, &[0, 2500], &[]).await;

            assert!(
                matches!(outcome, Err(Error::ConnectionReadTimeout)),
                "{mode:?}: {outcome:?}"
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_plugin_does_not_time_out_a_request() {
        let outcome = stream_response(Mode::Request, &[0, 2500], &[0, 2000]).await;

        assert!(matches!(outcome, Ok(2)), "{outcome:?}");
    }

    #[tokio::test(start_paused = true)]
    async fn a_slow_plugin_does_not_time_out_a_download() {
        // The first frame fills the read-ahead buffer, so the second one has to
        // wait for the plugin to make room before the third arrives.
        let outcome = stream_response(Mode::Download, &[0, 0, 2500], &[2000]).await;

        assert!(matches!(outcome, Ok(3)), "{outcome:?}");
    }
}
