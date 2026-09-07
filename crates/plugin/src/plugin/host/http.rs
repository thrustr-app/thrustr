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
    use super::{AllowedHosts, Error, OutboundHttp, WasiBody, http_client};
    use bytes::Bytes;
    use http::{Method, Request, StatusCode, header::CONTENT_LENGTH};
    use http_body_util::{BodyExt, Empty, Full};
    use std::net::SocketAddr;
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
        let mut hooks = OutboundHttp::new(http_client(), [allowed.to_string()][..].into());

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
}
