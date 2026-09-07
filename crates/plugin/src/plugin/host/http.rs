use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use http_body_util::BodyExt;
use reqwest::{Client, redirect::Policy};
use std::{
    fmt,
    future::Future,
    ops::Deref,
    pin::Pin,
    sync::{Arc, Mutex, PoisonError},
    task::{Context, Poll},
    time::Duration,
};
use tokio::time::{Instant, Sleep};
use wasmtime_wasi_http::{Error, RequestOptions, WasiBody, WasiHttpHooks};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);
const TCP_KEEPALIVE: Duration = Duration::from_secs(60);

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
}

impl OutboundHttp {
    pub fn new(client: Client, allowed_hosts: Arc<[String]>) -> Self {
        Self {
            client,
            allowed_hosts: AllowedHosts::new(allowed_hosts),
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

        Box::new(dispatch(self.client.clone(), request, options))
    }
}

async fn dispatch(
    client: Client,
    request: http::Request<WasiBody>,
    options: Option<RequestOptions>,
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
        IdleTimeoutBody::new(body, between_bytes).boxed_unsync()
    });

    Ok((response, Box::new(async { Ok(()) })))
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
struct IdleTimeoutBody {
    inner: WasiBody,
    idle_timeout: Duration,
    deadline: Pin<Box<Sleep>>,
}

impl IdleTimeoutBody {
    fn new(inner: WasiBody, idle_timeout: Duration) -> Self {
        Self {
            inner,
            idle_timeout,
            deadline: Box::pin(tokio::time::sleep(idle_timeout)),
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
            let deadline = Instant::now() + self.idle_timeout;
            self.deadline.as_mut().reset(deadline);
            return Poll::Ready(frame);
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

/// Wraps the guest body so `reqwest` can use it.
///
/// The mutex makes the body `Sync`. Its real error is stored separately
/// because `reqwest` needs its own error type.
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
        AllowedHosts, Error, IdleTimeoutBody, OutboundHttp, WasiBody, http_client, io_error,
    };
    use bytes::Bytes;
    use http::{Method, Request, StatusCode, header::CONTENT_LENGTH};
    use http_body::{Body, Frame};
    use http_body_util::{BodyExt, Empty, Full};
    use hyper::{body::Incoming, server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;
    use std::{
        future::poll_fn,
        net::SocketAddr,
        pin::Pin,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
        time::Duration,
    };
    use tokio::{net::TcpListener, task::JoinHandle};
    use wasmtime_wasi_http::WasiHttpHooks;

    fn allowlist(patterns: &[&str]) -> AllowedHosts {
        let hosts: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
        AllowedHosts::new(hosts.as_slice().into())
    }

    // A keep-alive server that records every request it serves, so a test can
    // assert both on what arrived and on how many connections carried it.
    struct TestServer {
        addr: SocketAddr,
        connections: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<Request<Bytes>>>>,
        listener: JoinHandle<()>,
    }

    impl TestServer {
        async fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();

            let connections = Arc::new(AtomicUsize::new(0));
            let requests = Arc::new(Mutex::new(Vec::new()));

            let accept = tokio::spawn({
                let connections = connections.clone();
                let requests = requests.clone();

                async move {
                    loop {
                        let (stream, _) = listener.accept().await.unwrap();
                        connections.fetch_add(1, Ordering::SeqCst);

                        let requests = requests.clone();

                        tokio::spawn(async move {
                            let service =
                                service_fn(|request| Self::serve(requests.clone(), request));

                            http1::Builder::new()
                                .serve_connection(TokioIo::new(stream), service)
                                .await
                                .ok();
                        });
                    }
                }
            });

            Self {
                addr,
                connections,
                requests,
                listener: accept,
            }
        }

        async fn serve(
            requests: Arc<Mutex<Vec<Request<Bytes>>>>,
            request: Request<Incoming>,
        ) -> Result<http::Response<Full<Bytes>>, hyper::Error> {
            let (parts, body) = request.into_parts();
            let body = body.collect().await?.to_bytes();

            requests
                .lock()
                .unwrap()
                .push(Request::from_parts(parts, body));

            Ok(http::Response::new(Full::new(Bytes::from_static(b"ok"))))
        }

        fn url(&self) -> String {
            format!("http://{}/", self.addr)
        }

        fn connections(&self) -> usize {
            self.connections.load(Ordering::SeqCst)
        }

        fn requests(&self) -> Vec<Request<Bytes>> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            self.listener.abort();
        }
    }

    fn outbound(allowed: &str) -> OutboundHttp {
        OutboundHttp::new(http_client(), [allowed.to_string()][..].into())
    }

    async fn get(hooks: &mut OutboundHttp, url: &str) -> Result<StatusCode, Error> {
        let body = Empty::<Bytes>::new().map_err(Error::from).boxed_unsync();
        let request = Request::builder().uri(url).body(body).unwrap();

        send(hooks, request).await
    }

    async fn post(
        hooks: &mut OutboundHttp,
        url: &str,
        body: &'static str,
    ) -> Result<StatusCode, Error> {
        let body = Full::new(Bytes::from_static(body.as_bytes()))
            .map_err(Error::from)
            .boxed_unsync();

        let request = Request::builder()
            .method(Method::POST)
            .uri(url)
            .body(body)
            .unwrap();

        send(hooks, request).await
    }

    async fn send(
        hooks: &mut OutboundHttp,
        request: Request<WasiBody>,
    ) -> Result<StatusCode, Error> {
        let sending = hooks.send_request(request, None, Box::new(async { Ok(()) }));

        let (response, _) = Box::into_pin(sending).await?;

        let status = response.status();
        response.into_body().collect().await?;

        Ok(status)
    }

    #[tokio::test]
    async fn requests_to_the_same_host_share_one_connection() {
        let server = TestServer::start().await;
        let mut hooks = outbound("127.0.0.1");

        for _ in 0..3 {
            let status = get(&mut hooks, &server.url()).await.unwrap();
            assert_eq!(status, StatusCode::OK);
        }

        assert_eq!(server.requests().len(), 3);
        assert_eq!(server.connections(), 1);
    }

    #[tokio::test]
    async fn separate_plugins_share_one_connection() {
        let server = TestServer::start().await;
        let client = http_client();

        for _ in 0..3 {
            let mut hooks = OutboundHttp::new(client.clone(), ["127.0.0.1".to_string()][..].into());
            get(&mut hooks, &server.url()).await.unwrap();
        }

        assert_eq!(server.requests().len(), 3);
        assert_eq!(server.connections(), 1);
    }

    #[tokio::test]
    async fn request_bodies_reach_the_server_with_their_length() {
        let server = TestServer::start().await;
        let mut hooks = outbound("127.0.0.1");

        let status = post(&mut hooks, &server.url(), "hello").await.unwrap();
        assert_eq!(status, StatusCode::OK);

        let requests = server.requests();
        let [request] = requests.as_slice() else {
            panic!("expected exactly one request, got {}", requests.len());
        };

        assert_eq!(request.method(), Method::POST);
        assert_eq!(request.body(), "hello");
        assert_eq!(request.headers()[CONTENT_LENGTH], "5");
    }

    #[tokio::test]
    async fn requests_to_a_denied_host_never_connect() {
        let server = TestServer::start().await;
        let mut hooks = outbound("example.com");

        let err = get(&mut hooks, &server.url()).await.unwrap_err();

        assert!(matches!(err, Error::HttpRequestDenied));
        assert_eq!(server.connections(), 0);
    }

    // Yields one data frame, then stalls forever — a server that goes silent
    // partway through a download.
    struct StallingBody(bool);

    impl Body for StallingBody {
        type Data = Bytes;
        type Error = Error;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, Error>>> {
            if self.0 {
                return Poll::Pending;
            }
            self.0 = true;
            Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(b"partial")))))
        }
    }

    #[tokio::test]
    async fn idle_timeout_body_fails_a_stalled_download() {
        let inner = StallingBody(false).boxed_unsync();
        let mut body = IdleTimeoutBody::new(inner, Duration::from_millis(50));

        let frame = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame.into_data().unwrap(), "partial");

        // The stream now stalls; the next frame resolves to a timeout error once
        // the idle deadline elapses.
        let err = poll_fn(|cx| Pin::new(&mut body).poll_frame(cx))
            .await
            .unwrap()
            .unwrap_err();
        assert!(matches!(err, Error::ConnectionReadTimeout));
    }

    #[test]
    fn io_errors_map_to_specific_codes() {
        use std::io::{Error as IoError, ErrorKind};

        assert!(matches!(
            io_error(&IoError::from(ErrorKind::ConnectionRefused)),
            Error::ConnectionRefused
        ));
        assert!(matches!(
            io_error(&IoError::from(ErrorKind::ConnectionReset)),
            Error::ConnectionTerminated
        ));
        assert!(matches!(
            io_error(&IoError::from(ErrorKind::TimedOut)),
            Error::ConnectionTimeout
        ));
        assert!(matches!(
            io_error(&IoError::from(ErrorKind::PermissionDenied)),
            Error::InternalError(_)
        ));
    }

    #[test]
    fn empty_allowlist_denies_everything() {
        let hosts = allowlist(&[]);
        assert!(!hosts.is_allowed(Some("api.legacygames.com")));
    }

    #[test]
    fn missing_host_is_denied() {
        assert!(!allowlist(&["*"]).is_allowed(None));
    }

    #[test]
    fn star_allows_any_host() {
        let hosts = allowlist(&["*"]);
        assert!(hosts.is_allowed(Some("api.legacygames.com")));
        assert!(hosts.is_allowed(Some("localhost")));
        assert!(hosts.is_allowed(Some("a.b.c.d.example.com")));
    }

    #[test]
    fn exact_host_matches_only_itself() {
        let hosts = allowlist(&["api.legacygames.com"]);
        assert!(hosts.is_allowed(Some("api.legacygames.com")));
        assert!(!hosts.is_allowed(Some("legacygames.com")));
        assert!(!hosts.is_allowed(Some("evil.com")));
        assert!(!hosts.is_allowed(Some("api.legacygames.com.evil.com")));
    }

    #[test]
    fn leading_wildcard_matches_subdomains_at_any_depth() {
        let hosts = allowlist(&["*.example.com"]);
        assert!(hosts.is_allowed(Some("api.example.com")));
        assert!(hosts.is_allowed(Some("a.b.example.com")));
        assert!(!hosts.is_allowed(Some("example.com")));
        assert!(!hosts.is_allowed(Some("example.com.evil.com")));
        assert!(!hosts.is_allowed(Some("notexample.com")));
    }

    #[test]
    fn trailing_wildcard_matches_any_suffix() {
        let hosts = allowlist(&["api.*"]);
        assert!(hosts.is_allowed(Some("api.legacygames.com")));
        assert!(hosts.is_allowed(Some("api.example.org")));
        assert!(!hosts.is_allowed(Some("cdn.legacygames.com")));
    }

    #[test]
    fn wildcard_in_the_middle() {
        let hosts = allowlist(&["blabla.*.com"]);
        assert!(hosts.is_allowed(Some("blabla.foo.com")));
        assert!(hosts.is_allowed(Some("blabla.foo.bar.com")));
        assert!(!hosts.is_allowed(Some("blabla.foo.org")));
        assert!(!hosts.is_allowed(Some("other.foo.com")));
    }

    #[test]
    fn matching_is_case_insensitive() {
        let hosts = allowlist(&["API.LegacyGames.com"]);
        assert!(hosts.is_allowed(Some("api.legacygames.com")));
    }

    #[test]
    fn any_pattern_in_the_list_may_match() {
        let hosts = allowlist(&["example.com", "*.example.com"]);
        assert!(hosts.is_allowed(Some("example.com")));
        assert!(hosts.is_allowed(Some("api.example.com")));
        assert!(!hosts.is_allowed(Some("evil.com")));
    }
}
