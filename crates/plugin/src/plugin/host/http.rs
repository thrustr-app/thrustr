use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use std::{future::Future, sync::Arc};
use wasmtime_wasi::TrappableError;
use wasmtime_wasi_http::p3::{
    RequestOptions, WasiHttpHooks, bindings::http::types::ErrorCode, default_hooks,
};

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

#[derive(Default)]
pub struct AllowedHosts(Arc<[String]>);

impl AllowedHosts {
    pub fn new(hosts: Arc<[String]>) -> Self {
        Self(hosts)
    }

    fn is_allowed(&self, host: Option<&str>) -> bool {
        let Some(host) = host else {
            return false;
        };
        let host = host.to_ascii_lowercase();

        self.0
            .iter()
            .any(|pattern| matches_pattern(&pattern.to_ascii_lowercase(), &host))
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

impl WasiHttpHooks for AllowedHosts {
    fn send_request(
        &mut self,
        request: http::Request<UnsyncBoxBody<Bytes, ErrorCode>>,
        options: Option<RequestOptions>,
        fut: Box<dyn Future<Output = Result<(), ErrorCode>> + Send>,
    ) -> SendRequestFuture {
        if !self.is_allowed(request.uri().host()) {
            return Box::new(async { Err(ErrorCode::HttpRequestDenied.into()) });
        }

        default_hooks().send_request(request, options, fut)
    }
}

#[cfg(test)]
mod tests {
    use super::AllowedHosts;

    fn allowlist(patterns: &[&str]) -> AllowedHosts {
        let hosts: Vec<String> = patterns.iter().map(|p| p.to_string()).collect();
        AllowedHosts::new(hosts.as_slice().into())
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
