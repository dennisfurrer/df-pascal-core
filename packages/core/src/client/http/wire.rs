//! The [`Http`] trait and its request and response types. Implement it to use a custom HTTP
//! client.

use std::future::Future;

/// Sends one HTTP request and returns the response for any status. It does not handle hosts or
/// rate limits; an error means no response was received ([`TransportError::Unreachable`]).
pub trait Http {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>>;
}

impl<H: Http> Http for &H {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        (**self).send(request)
    }
}

impl<H: Http> Http for std::sync::Arc<H> {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        (**self).send(request)
    }
}

impl<H: Http> Http for std::rc::Rc<H> {
    fn send(&self, request: Request) -> impl Future<Output = Result<Response, TransportError>> {
        (**self).send(request)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// True if the request goes to Pascal's write API. An `offline` build refuses it.
    pub writes: bool,
}

impl Request {
    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Request {
            method: Method::Get,
            url: url.into(),
            headers: vec![("Accept".to_owned(), "application/json".to_owned())],
            body: Vec::new(),
            writes: false,
        }
    }

    /// A POST with a JSON body, sent as `application/json`.
    #[must_use]
    pub fn post_json(url: impl Into<String>, body: String) -> Self {
        Request {
            method: Method::Post,
            url: url.into(),
            headers: vec![
                ("Accept".to_owned(), "application/json".to_owned()),
                ("Content-Type".to_owned(), "application/json".to_owned()),
            ],
            body: body.into_bytes(),
            writes: false,
        }
    }

    /// Marks the request as a write.
    #[must_use]
    pub const fn write(mut self) -> Self {
        self.writes = true;
        self
    }

    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// Returns [`TransportError::Offline`] for a write in an `offline` build, otherwise `Ok`.
    pub fn refuse_if_offline(&self) -> Result<(), TransportError> {
        match OFFLINE && self.writes {
            true => Err(TransportError::Offline(self.url.split('?').next().unwrap_or_default().to_owned())),
            false => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        find_header(&self.headers, name)
    }

    /// `Retry-After` in milliseconds, if the header is given in whole seconds.
    #[must_use]
    pub fn retry_after_ms(&self) -> Option<u64> {
        let seconds = self.header("retry-after").and_then(|seconds| seconds.trim().parse::<u64>().ok())?;
        Some(seconds.saturating_mul(1_000))
    }

    /// The body as text; invalid UTF-8 is replaced with U+FFFD.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// No response was received. For a write the outcome is unknown and it may have been
    /// executed, so it is not resent; read the account to check.
    #[error("no answer from Pascal: {0}")]
    Unreachable(String),
    /// A response that is not Pascal's envelope, such as a proxy error page or an empty body.
    #[error("Pascal answered HTTP {status}: {body}")]
    Status { status: u16, body: String },
    /// Rate limited by Pascal (HTTP 429) or by the client's limiter before sending. The request
    /// was not executed; send it again after the wait.
    #[error("rate limited; try again in {} ms", retry_after_ms.unwrap_or(0))]
    RateLimited { retry_after_ms: Option<u64> },
    /// Not sent: this build has the `offline` feature and the request is a write.
    #[error("an offline build sends no writes: {0}")]
    Offline(String),
    /// Not sent: the request needs a trading key and the client has none.
    #[error("{0} is signed by a trading key: give the client one")]
    NoKey(String),
}

/// Whether this build's transports refuse writes ([`TransportError::Offline`]).
pub const OFFLINE: bool = cfg!(feature = "offline");
