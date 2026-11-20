//! The [`Transport`] trait, the [`Call`] type, and decoding of Pascal's response envelope:
//! `{"status": "success" | "error", "data": .., "seq": .., ..}`.

use std::future::Future;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::client::http::codes::ErrorCode;
use crate::client::http::wire::{Method, TransportError};
use crate::utils::lenient;
use crate::utils::permit::BadPermit;
use crate::utils::sign::SignError;

/// Which of Pascal's APIs a call goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Host {
    /// `data.pascal.trade`: all reads, unauthenticated.
    Read,
    /// `trade.pascal.trade`: all signed writes.
    Write,
}

/// One request to Pascal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub method: Method,
    pub host: Host,
    pub path: String,
    pub query: Vec<(&'static str, String)>,
    /// A JSON body, for a POST.
    pub body: Option<String>,
    /// True if it counts against the limit of 100 order requests per second.
    pub orders: bool,
}

impl Call {
    #[must_use]
    pub fn get(path: impl Into<String>) -> Call {
        Call { method: Method::Get, host: Host::Read, path: path.into(), query: Vec::new(), body: None, orders: false }
    }

    /// A POST with a JSON body to the read API (market search).
    #[must_use]
    pub fn search(path: impl Into<String>, body: String) -> Call {
        Call {
            method: Method::Post,
            host: Host::Read,
            path: path.into(),
            query: Vec::new(),
            body: Some(body),
            orders: false,
        }
    }

    /// A POST with a JSON body to the write API.
    #[must_use]
    pub fn write(path: impl Into<String>, body: String) -> Call {
        Call {
            method: Method::Post,
            host: Host::Write,
            path: path.into(),
            query: Vec::new(),
            body: Some(body),
            orders: false,
        }
    }

    /// Adds a query field.
    #[must_use]
    pub fn field(mut self, key: &'static str, value: impl ToString) -> Call {
        self.query.push((key, value.to_string()));
        self
    }

    /// Adds a query field if `value` is `Some`.
    #[must_use]
    pub fn maybe(self, key: &'static str, value: Option<impl ToString>) -> Call {
        match value {
            Some(value) => self.field(key, value),
            None => self,
        }
    }

    /// Marks the call as counting against the order rate limit.
    #[must_use]
    pub const fn orders(mut self) -> Call {
        self.orders = true;
        self
    }

    /// The query string, percent-encoded.
    #[must_use]
    pub fn query_string(&self) -> String {
        self.query.iter().map(|(key, value)| format!("{key}={}", encode_component(value))).collect::<Vec<_>>().join("&")
    }
}

/// Percent-encodes everything except unreserved characters and commas, which join lists.
#[must_use]
pub fn encode_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' => out.push(char::from(byte)),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Percent-encodes a path segment, such as an owner or a referral code.
#[must_use]
pub fn segment(text: &str) -> String {
    encode_component(text).replace(',', "%2C")
}

/// Sends a [`Call`] and returns the HTTP status and response body. [`crate::client::Client`]
/// implements it with rate limiting. There is no `Send` bound because the browser is
/// single-threaded.
pub trait Transport {
    fn call(&self, call: Call) -> impl Future<Output = Result<(u16, String), TransportError>>;
}

impl<T: Transport> Transport for &T {
    fn call(&self, call: Call) -> impl Future<Output = Result<(u16, String), TransportError>> {
        (**self).call(call)
    }
}

/// The engine state a response was produced at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Context {
    /// Global sequence number. Compare it across REST, websocket and history to tell which state
    /// is newer. Absent when a request was rejected before reaching the engine.
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub seq: Option<u64>,
    /// Engine time at its last matching round, in ms.
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub state_time_ms: Option<u64>,
    /// When the API produced this response, in ms.
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub server_time_ms: Option<u64>,
    /// When the write API received the request, in ms; writes only.
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub server_receive_time_ms: Option<u64>,
}

/// A value and the engine state it was read or written at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply<T> {
    pub data: T,
    pub context: Context,
}

impl<T> Reply<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Reply<U> {
        Reply { data: f(self.data), context: self.context }
    }
}

/// An error response from Pascal: its code, message, HTTP status if any, and engine state.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("refused ({code}): {message}")]
pub struct Refusal {
    pub code: ErrorCode,
    pub message: String,
    pub status: Option<u16>,
    pub context: Context,
}

/// Error from a read or write.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error("Pascal's answer on {path} could not be read: {detail}")]
    Unreadable { path: String, detail: String },
    #[error(transparent)]
    Refused(#[from] Refusal),
    /// Not sent: the request breaks a rule Pascal would reject it for.
    #[error("not sent: {0}")]
    Invalid(#[from] BadPermit),
    /// Not sent: the signer failed to sign, or the signature does not verify.
    #[error("not sent: {0}")]
    Sign(#[from] SignError),
}

impl ApiError {
    /// Pascal's error code, if it returned one.
    #[must_use]
    pub fn code(&self) -> Option<&ErrorCode> {
        match self {
            ApiError::Refused(refusal) => Some(&refusal.code),
            _ => None,
        }
    }

    /// Rate limited by the local limiter or by Pascal. Wait and send again.
    #[must_use]
    pub fn is_rate_limited(&self) -> bool {
        matches!(self, ApiError::Transport(TransportError::RateLimited { .. }))
            || matches!(self.code(), Some(ErrorCode::RateLimited | ErrorCode::Backpressure))
    }

    /// The signer is not an active trading key for the owner, or the signature is invalid.
    #[must_use]
    pub fn is_unauthorized(&self) -> bool {
        matches!(self.code(), Some(ErrorCode::Unauthorized | ErrorCode::TradingKeyNotFound))
    }

    /// Whether a write may have been executed: no response, an unreadable response, a 5xx status,
    /// or an error code saying the result was lost. Any other error means it was not executed.
    #[must_use]
    pub fn may_have_landed(&self) -> bool {
        match self {
            ApiError::Transport(TransportError::Unreachable(_)) | ApiError::Unreadable { .. } => true,
            ApiError::Transport(TransportError::Status { status, .. }) => *status >= 500,
            ApiError::Transport(_) | ApiError::Invalid(_) | ApiError::Sign(_) => false,
            ApiError::Refused(refusal) => refusal.code.may_have_landed(),
        }
    }
}

#[derive(Deserialize)]
struct Envelope {
    status: String,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(flatten)]
    context: Context,
}

#[derive(Deserialize)]
struct ErrorData {
    #[serde(default = "unknown_code")]
    code: ErrorCode,
    #[serde(default, deserialize_with = "lenient::text")]
    message: String,
}

fn unknown_code() -> ErrorCode {
    ErrorCode::Unknown(String::new())
}

/// Sends one call and decodes the response. Nothing is retried.
pub async fn read<R: DeserializeOwned>(transport: &impl Transport, call: Call) -> Result<Reply<R>, ApiError> {
    let path = call.path.clone();
    let (status, text) = transport.call(call).await?;
    decode(&path, status, &text)
}

/// Decodes a response body as `R` inside its envelope, or returns Pascal's error.
pub fn decode<R: DeserializeOwned>(path: &str, status: u16, text: &str) -> Result<Reply<R>, ApiError> {
    let envelope: Envelope = serde_json::from_str(text).map_err(|error| match status {
        200..=299 => ApiError::Unreadable { path: path.to_owned(), detail: unreadable(&error, text) },
        _ => ApiError::Transport(TransportError::Status { status, body: shown(text) }),
    })?;
    open(path, Some(status), envelope)
}

fn open<R: DeserializeOwned>(path: &str, status: Option<u16>, envelope: Envelope) -> Result<Reply<R>, ApiError> {
    if envelope.status != "success" {
        let error: ErrorData = serde_json::from_value(envelope.data).unwrap_or(ErrorData {
            code: unknown_code(),
            message: format!("status `{}` with no error object", envelope.status),
        });
        return Err(ApiError::Refused(Refusal {
            code: error.code,
            message: error.message,
            status: status.filter(|status| *status != 200),
            context: envelope.context,
        }));
    }
    let data = R::deserialize(envelope.data)
        .map_err(|error| ApiError::Unreadable { path: path.to_owned(), detail: error.to_string() })?;
    Ok(Reply { data, context: envelope.context })
}

/// Sends a batch and returns one result per request, in the order sent. A refusal of the whole
/// request (rate limit, malformed batch) is returned as one error.
pub async fn read_batch<R: DeserializeOwned>(
    transport: &impl Transport,
    call: Call,
) -> Result<Vec<Result<Reply<R>, Refusal>>, ApiError> {
    let path = call.path.clone();
    let (status, text) = transport.call(call).await?;
    decode_batch(&path, status, &text)
}

pub fn decode_batch<R: DeserializeOwned>(
    path: &str,
    status: u16,
    text: &str,
) -> Result<Vec<Result<Reply<R>, Refusal>>, ApiError> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| match status {
        200..=299 => ApiError::Unreadable { path: path.to_owned(), detail: unreadable(&error, text) },
        _ => ApiError::Transport(TransportError::Status { status, body: shown(text) }),
    })?;
    match value {
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|item| {
                let envelope: Envelope = serde_json::from_value(item)
                    .map_err(|error| ApiError::Unreadable { path: path.to_owned(), detail: error.to_string() })?;
                match open::<R>(path, None, envelope) {
                    Ok(reply) => Ok(Ok(reply)),
                    Err(ApiError::Refused(refusal)) => Ok(Err(refusal)),
                    Err(other) => Err(other),
                }
            })
            .collect(),
        single => {
            let envelope: Envelope = serde_json::from_value(single)
                .map_err(|error| ApiError::Unreadable { path: path.to_owned(), detail: error.to_string() })?;
            // A single envelope for a batch is a request-level response. A success is read as a
            // batch of one.
            open::<R>(path, Some(status), envelope).map(|reply| vec![Ok(reply)])
        }
    }
}

fn shown(text: &str) -> String {
    text.chars().take(300).collect()
}

pub(crate) fn unreadable(error: &serde_json::Error, text: &str) -> String {
    format!("{error} in {}", shown(text))
}

#[cfg(test)]
#[path = "tests/calls.rs"]
mod tests;
