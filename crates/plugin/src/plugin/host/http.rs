use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};
use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
use reqwest::{Client, redirect::Policy};
use std::{
    error::Error,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};
use wasmtime_wasi::TrappableError;
use wasmtime_wasi_http::p3::{RequestOptions, WasiHttpHooks, bindings::http::types::ErrorCode};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(600);

type SendRequestFuture = Box<
    dyn Future<
            Output = Result<
                (
                    http::Response<UnsyncBoxBody<Bytes, ErrorCode>>,
                    Box<dyn Future<Output = Result<(), ErrorCode>> + Send>,
                ),
                TrappableError<ErrorCode>,
            >,
        > + Send,
>;

pub fn http_client() -> Client {
    Client::builder()
        .redirect(Policy::none())
        .read_timeout(DEFAULT_TIMEOUT)
        .build()
        .expect("Failed to build plugin HTTP client")
}

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
        request: http::Request<UnsyncBoxBody<Bytes, ErrorCode>>,
        options: Option<RequestOptions>,
        _fut: Box<dyn Future<Output = Result<(), ErrorCode>> + Send>,
    ) -> SendRequestFuture {
        if !self.allowed_hosts.is_allowed(request.uri().host()) {
            return Box::new(async { Err(ErrorCode::HttpRequestDenied.into()) });
        }

        let client = self.client.clone();

        Box::new(send_request(client, request, options))
    }
}

async fn send_request(
    client: Client,
    request: http::Request<UnsyncBoxBody<Bytes, ErrorCode>>,
    options: Option<RequestOptions>,
) -> Result<
    (
        http::Response<UnsyncBoxBody<Bytes, ErrorCode>>,
        Box<dyn Future<Output = Result<(), ErrorCode>> + Send>,
    ),
    TrappableError<ErrorCode>,
> {
    let (parts, body) = request.into_parts();
    let body = reqwest::Body::wrap(SyncBody(Mutex::new(body)));

    let request = reqwest::Request::try_from(http::Request::from_parts(parts, body))
        .map_err(|_| ErrorCode::HttpRequestUriInvalid)?;

    let connect_timeout = options
        .and_then(|options| options.connect_timeout)
        .unwrap_or(DEFAULT_TIMEOUT);
    let first_byte_timeout = options
        .and_then(|options| options.first_byte_timeout)
        .unwrap_or(DEFAULT_TIMEOUT);
    let head_timeout = connect_timeout.saturating_add(first_byte_timeout);

    let response = tokio::time::timeout(head_timeout, client.execute(request))
        .await
        .map_err(|_| ErrorCode::ConnectionTimeout)?
        .map_err(request_error)?;

    let response = http::Response::<reqwest::Body>::from(response)
        .map(|body| body.map_err(response_error).boxed_unsync());

    Ok((response, Box::new(async { Ok(()) })))
}

struct SyncBody(Mutex<UnsyncBoxBody<Bytes, ErrorCode>>);

impl SyncBody {
    fn get(&self) -> impl std::ops::Deref<Target = UnsyncBoxBody<Bytes, ErrorCode>> + '_ {
        self.0.lock().expect("Plugin request body poisoned")
    }
}

impl Body for SyncBody {
    type Data = Bytes;
    type Error = ErrorCode;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, ErrorCode>>> {
        let body = self
            .get_mut()
            .0
            .get_mut()
            .expect("Plugin request body poisoned");

        Pin::new(body).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.get().is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.get().size_hint()
    }
}

fn request_error(err: reqwest::Error) -> ErrorCode {
    if let Some(code) = guest_error(&err) {
        code
    } else if err.is_timeout() {
        ErrorCode::ConnectionTimeout
    } else if err.is_connect() {
        ErrorCode::ConnectionRefused
    } else {
        ErrorCode::InternalError(Some(err.to_string()))
    }
}

fn response_error(err: reqwest::Error) -> ErrorCode {
    if let Some(code) = guest_error(&err) {
        code
    } else if err.is_timeout() {
        ErrorCode::HttpResponseTimeout
    } else {
        ErrorCode::InternalError(Some(err.to_string()))
    }
}

fn guest_error(err: &reqwest::Error) -> Option<ErrorCode> {
    let mut source: Option<&(dyn Error + 'static)> = err.source();

    while let Some(err) = source {
        if let Some(code) = err.downcast_ref::<ErrorCode>() {
            return Some(code.clone());
        }

        source = err.source();
    }

    None
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
    let mut segments = pattern.split('*');

    let Some(prefix) = segments.next() else {
        return false;
    };
    let Some(mut rest) = host.strip_prefix(prefix) else {
        return false;
    };

    let segments: Vec<&str> = segments.collect();
    let Some((suffix, middles)) = segments.split_last() else {
        return host == pattern;
    };

    for middle in middles {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }

    rest.len() >= suffix.len() && rest.ends_with(suffix)
}

#[cfg(test)]
mod tests {
    use super::{AllowedHosts, ErrorCode, OutboundHttp, UnsyncBoxBody, http_client};
    use bytes::Bytes;
    use http::{Method, Request, StatusCode, header::CONTENT_LENGTH};
    use http_body_util::{BodyExt, Empty, Full};
    use hyper::{body::Incoming, server::conn::http1, service::service_fn};
    use hyper_util::rt::TokioIo;
    use std::{
        net::SocketAddr,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tokio::{net::TcpListener, task::JoinHandle};
    use wasmtime_wasi_http::p3::WasiHttpHooks;

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

    async fn get(hooks: &mut OutboundHttp, url: &str) -> Result<StatusCode, ErrorCode> {
        let body = Empty::<Bytes>::new()
            .map_err(ErrorCode::from)
            .boxed_unsync();
        let request = Request::builder().uri(url).body(body).unwrap();

        send(hooks, request).await
    }

    async fn post(
        hooks: &mut OutboundHttp,
        url: &str,
        body: &'static str,
    ) -> Result<StatusCode, ErrorCode> {
        let body = Full::new(Bytes::from_static(body.as_bytes()))
            .map_err(ErrorCode::from)
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
        request: Request<UnsyncBoxBody<Bytes, ErrorCode>>,
    ) -> Result<StatusCode, ErrorCode> {
        let sending = hooks.send_request(request, None, Box::new(async { Ok(()) }));

        let (response, _) = Box::into_pin(sending)
            .await
            .map_err(|err| err.downcast().unwrap())?;

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

        assert!(matches!(err, ErrorCode::HttpRequestDenied));
        assert_eq!(server.connections(), 0);
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
