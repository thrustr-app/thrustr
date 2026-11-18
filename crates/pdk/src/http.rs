use serde::{Serialize, de::DeserializeOwned};
use std::{fmt, future::poll_fn, mem, pin::pin};
use wasip3::{
    http::{
        client,
        types::{self, Fields, Scheme},
    },
    wit_bindgen::{FutureReader, FutureWriter, StreamReader, StreamResult},
    wit_future, wit_stream,
};

pub use wasip3::http::types::{ErrorCode, Method};

const CHUNK_LEN: usize = 256 * 1024;

pub fn get(url: impl Into<String>) -> Request {
    Request::new(Method::Get, url)
}

pub fn post(url: impl Into<String>) -> Request {
    Request::new(Method::Post, url)
}

#[must_use]
pub struct Request {
    method: Method,
    url: String,
    query: String,
    headers: Vec<(String, Vec<u8>)>,
    body: Vec<u8>,
    /// From a builder method, returned by `send`.
    error: Option<Error>,
}

impl Request {
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            query: String::new(),
            headers: Vec::new(),
            body: Vec::new(),
            error: None,
        }
    }

    /// Percent-encodes the pairs and appends them to the query.
    pub fn query<K, V>(mut self, pairs: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: AsRef<str>,
        V: fmt::Display,
    {
        let mut query = form_urlencoded::Serializer::new(mem::take(&mut self.query));
        for (name, value) in pairs {
            query.append_pair(name.as_ref(), &value.to_string());
        }
        self.query = query.finish();
        self
    }

    /// Adding a name again adds another value.
    pub fn header(mut self, name: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    pub fn json<T: Serialize + ?Sized>(mut self, body: &T) -> Self {
        match serde_json::to_vec(body) {
            Ok(body) => {
                if !self.has_header("content-type") {
                    self = self.header("content-type", "application/json");
                }
                self.body = body;
            }
            Err(error) => self.error = Some(Error::Json(error)),
        }
        self
    }

    pub async fn send(mut self) -> Result<Response, Error> {
        if let Some(error) = self.error.take() {
            return Err(error);
        }
        let target = self.target();
        let has_len = self.has_header("content-length");
        let Self {
            method,
            url,
            mut headers,
            body,
            ..
        } = self;

        let invalid = |what: &str| Error::InvalidRequest(format!("{what} in `{url}`"));
        let (scheme, authority, path) = target.ok_or_else(|| invalid("bad URL"))?;

        if !body.is_empty() && !has_len {
            headers.push(("content-length".into(), body.len().to_string().into()));
        }

        let headers = Fields::from_list(&headers)
            .map_err(|error| Error::InvalidRequest(format!("bad header: {error:?}")))?;

        let (writer, contents) = (!body.is_empty()).then(wit_stream::new).unzip();
        let (_, trailers) = wit_future::new(|| Ok(None));
        let (request, _) = types::Request::new(headers, contents, trailers, None);

        request
            .set_method(&method)
            .map_err(|()| invalid("bad method"))?;
        request
            .set_scheme(Some(&scheme))
            .and_then(|()| request.set_authority(Some(&authority)))
            .and_then(|()| request.set_path_with_query(Some(&path)))
            .map_err(|()| invalid("bad URL"))?;

        let write = async move {
            if let Some(mut writer) = writer {
                writer.write_all(body).await;
            }
        };
        let response = alongside(write, client::send(request)).await;

        Ok(Response::new(response.map_err(Error::Http)?))
    }

    fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(other, _)| other.eq_ignore_ascii_case(name))
    }

    fn target(&self) -> Option<(Scheme, String, String)> {
        let (scheme, rest) = self.url.split_once("://")?;
        let scheme = match scheme.to_ascii_lowercase().as_str() {
            "https" => Scheme::Https,
            "http" => Scheme::Http,
            _ => return None,
        };

        let rest = rest.split_once('#').map_or(rest, |(rest, _fragment)| rest);
        let (authority, path) = rest.split_at(rest.find(['/', '?']).unwrap_or(rest.len()));

        if authority.is_empty() {
            return None;
        }
        let mut path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };

        if !self.query.is_empty() {
            if !path.ends_with('?') {
                path.push(if path.contains('?') { '&' } else { '?' });
            }
            path.push_str(&self.query);
        }

        Some((scheme, authority.to_string(), path))
    }
}

pub struct Response {
    status: u16,
    headers: Fields,
    body: Body,
}

impl Response {
    fn new(response: types::Response) -> Self {
        let status = response.get_status_code();
        let headers = response.get_headers();
        let (done, result) = wit_future::new(|| Ok(()));
        let (stream, trailers) = types::Response::consume_body(response, result);
        Self {
            status,
            headers,
            body: Body {
                stream,
                trailers: Some(trailers),
                _done: done,
            },
        }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    /// The first value of a header, if it's text.
    pub fn header(&self, name: &str) -> Option<String> {
        let value = self.headers.get(name).into_iter().next()?;
        String::from_utf8(value).ok()
    }

    pub fn into_body(self) -> Body {
        self.body
    }

    pub async fn bytes(self) -> Result<Vec<u8>, Error> {
        self.body.bytes().await
    }

    pub async fn json<T: DeserializeOwned>(self) -> Result<T, Error> {
        Ok(serde_json::from_slice(&self.bytes().await?)?)
    }
}

pub struct Body {
    stream: StreamReader<u8>,
    trailers: Option<FutureReader<Result<Option<Fields>, ErrorCode>>>,
    _done: FutureWriter<Result<(), ErrorCode>>,
}

impl Body {
    pub async fn chunk(&mut self) -> Result<Option<Vec<u8>>, Error> {
        let mut chunk = Vec::with_capacity(CHUNK_LEN);
        let more = self.read_into(&mut chunk).await?;
        Ok(more.then_some(chunk))
    }

    pub async fn bytes(mut self) -> Result<Vec<u8>, Error> {
        let mut bytes = Vec::new();
        loop {
            bytes.reserve(CHUNK_LEN);
            if !self.read_into(&mut bytes).await? {
                return Ok(bytes);
            }
        }
    }

    async fn read_into(&mut self, buffer: &mut Vec<u8>) -> Result<bool, Error> {
        loop {
            let (result, read) = self.stream.read(mem::take(buffer)).await;
            *buffer = read;
            match result {
                StreamResult::Complete(0) => {}
                StreamResult::Complete(_) => return Ok(true),
                StreamResult::Dropped => {
                    if let Some(trailers) = self.trailers.take() {
                        trailers.await.map_err(Error::Http)?;
                    }
                    return Ok(false);
                }
                StreamResult::Cancelled => unreachable!("reads are never cancelled"),
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid HTTP request: {0}")]
    InvalidRequest(String),

    #[error("HTTP request failed: {0:?}")]
    Http(ErrorCode),

    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<Error> for crate::Error {
    fn from(error: Error) -> Self {
        crate::Error::other(error.to_string())
    }
}

async fn alongside<T>(side: impl Future, main: impl Future<Output = T>) -> T {
    let (mut side, mut main) = (pin!(side), pin!(main));
    let mut side_done = false;
    poll_fn(|cx| {
        if !side_done {
            side_done = side.as_mut().poll(cx).is_ready();
        }
        main.as_mut().poll(cx)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check(request: Request, expected: Option<(&str, &str, &str)>) {
        let target = request.target();
        let actual = target.as_ref().map(|(scheme, authority, path)| {
            let scheme = match scheme {
                Scheme::Http => "http",
                Scheme::Https => "https",
                Scheme::Other(other) => other.as_str(),
            };
            (scheme, authority.as_str(), path.as_str())
        });

        assert_eq!(actual, expected);
    }

    #[test]
    fn query_pairs_are_appended_percent_encoded() {
        check(
            get("https://host/a").query([("page", 2), ("per_page", 50)]),
            Some(("https", "host", "/a?page=2&per_page=50")),
        );
        check(
            get("https://host/a").query([("q", "a b&c=d/é")]),
            Some(("https", "host", "/a?q=a+b%26c%3Dd%2F%C3%A9")),
        );
        check(
            get("https://host").query([("a", 1)]).query([("b", 2)]),
            Some(("https", "host", "/?a=1&b=2")),
        );
    }

    #[test]
    fn query_pairs_are_added_to_the_urls_own_query() {
        check(
            get("https://host/a?b=c").query([("d", "e")]),
            Some(("https", "host", "/a?b=c&d=e")),
        );
        check(
            get("https://host/a?").query([("d", "e")]),
            Some(("https", "host", "/a?d=e")),
        );
        check(
            get("https://host/a#top").query([("d", "e")]),
            Some(("https", "host", "/a?d=e")),
        );
    }

    #[test]
    fn missing_query_pairs_add_nothing() {
        check(
            get("https://host/a").query(None::<(&str, &str)>),
            Some(("https", "host", "/a")),
        );
    }

    #[test]
    fn absolute_urls_are_sent_as_scheme_authority_and_path() {
        check(
            get("https://api.itch.io/games/1"),
            Some(("https", "api.itch.io", "/games/1")),
        );
        check(
            get("http://127.0.0.1:8080/a/b?c=d"),
            Some(("http", "127.0.0.1:8080", "/a/b?c=d")),
        );
        check(
            get("https://user@host/a"),
            Some(("https", "user@host", "/a")),
        );
    }

    #[test]
    fn the_scheme_is_case_insensitive() {
        check(get("HTTPS://host/a"), Some(("https", "host", "/a")));
        check(get("Http://host/a"), Some(("http", "host", "/a")));
    }

    #[test]
    fn a_missing_path_is_sent_as_the_root() {
        check(get("https://host"), Some(("https", "host", "/")));
        check(
            get("https://host?page=1"),
            Some(("https", "host", "/?page=1")),
        );
        check(get("https://host#top"), Some(("https", "host", "/")));
    }

    #[test]
    fn fragments_are_not_sent() {
        check(
            get("https://host/a#fragment"),
            Some(("https", "host", "/a")),
        );
        check(
            get("https://host/a?b=c#d"),
            Some(("https", "host", "/a?b=c")),
        );
    }

    #[test]
    fn urls_without_an_http_scheme_or_host_are_rejected() {
        check(get("ftp://host/a"), None);
        check(get("host/a"), None);
        check(get("/relative"), None);
        check(get("https://"), None);
        check(get("https:///a"), None);
        check(get("https://?a=b"), None);
    }
}
