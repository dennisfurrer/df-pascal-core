//! Order entry websocket protocol without I/O: request frames for place, cancel and mass cancel
//! (the same signed payloads as REST) and response decoding.
//!
//! Requests are processed in the order sent; responses can arrive in any order and are matched by
//! request `id`. Limits per connection: 128 requests in flight, 1,000 per second, 50 items per
//! frame (10 for mass cancels), 64 KiB per frame, and a ping at least every 60 seconds.

use serde::{Deserialize, Serialize};

use crate::client::http::calls::decode_batch;
use crate::client::{ApiError, ErrorCode, Refusal, Reply, Signed};
use crate::live::frame::BadFrame;
use crate::trade::cancel::{CancelBody, MassCancelBody, MassCancelled};
use crate::trade::order::{Order, Placed};
use crate::trade::orders::PlaceBody;
use crate::utils::lenient;

pub const MAX_IN_FLIGHT: usize = 128;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const PING_EVERY_MS: u64 = 30_000;

/// Close codes the server uses.
pub mod close {
    pub const IDLE: u16 = 1000;
    pub const GOING_AWAY: u16 = 1001;
    pub const TOO_BIG: u16 = 1009;
    pub const SLOW_CONSUMER: u16 = 4001;
    /// Sent when the client keeps sending after rate-limit errors; new connections are then
    /// refused for several minutes.
    pub const RATE_LIMITED: u16 = 4002;
    pub const MALFORMED_FRAME: u16 = 4003;
}

#[derive(Serialize)]
struct Out<'a, T> {
    id: &'a str,
    #[serde(rename = "type")]
    kind: &'static str,
    payload: T,
}

#[must_use]
pub fn place_frame(id: &str, orders: &[Signed<PlaceBody>]) -> String {
    serde_json::to_string(&Out { id, kind: "place", payload: orders }).unwrap_or_default()
}

#[must_use]
pub fn cancel_frame(id: &str, cancels: &[Signed<CancelBody>]) -> String {
    serde_json::to_string(&Out { id, kind: "cancel", payload: cancels }).unwrap_or_default()
}

#[must_use]
pub fn mass_cancel_frame(id: &str, mass_cancels: &[Signed<MassCancelBody>]) -> String {
    serde_json::to_string(&Out { id, kind: "mass_cancel", payload: mass_cancels }).unwrap_or_default()
}

/// `{"type":"ping"}`; the server replies `{"type":"pong"}`.
pub const PING: &str = r#"{"type":"ping"}"#;

/// One response from the order websocket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrderAnswer {
    Place {
        id: Option<String>,
        results: Vec<Result<Reply<Placed>, Refusal>>,
    },
    Cancel {
        id: Option<String>,
        results: Vec<Result<Reply<Order>, Refusal>>,
    },
    MassCancel {
        id: Option<String>,
        results: Vec<Result<Reply<MassCancelled>, Refusal>>,
    },
    /// A frame rejected before it entered the write pipeline (rate limit, backpressure, malformed
    /// frame, too many items): none of it was executed.
    Rejected {
        id: Option<String>,
        code: ErrorCode,
        message: String,
        client_order_ids: Vec<u64>,
    },
    Pong,
}

impl OrderAnswer {
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            OrderAnswer::Place { id, .. }
            | OrderAnswer::Cancel { id, .. }
            | OrderAnswer::MassCancel { id, .. }
            | OrderAnswer::Rejected { id, .. } => id.as_deref(),
            OrderAnswer::Pong => None,
        }
    }
}

#[derive(Deserialize)]
struct In {
    #[serde(default)]
    id: Option<String>,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Deserialize)]
struct Rejection {
    #[serde(default)]
    data: Option<RejectionData>,
    #[serde(default)]
    client_order_ids: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct RejectionData {
    #[serde(default = "unknown")]
    code: ErrorCode,
    #[serde(default, deserialize_with = "lenient::text")]
    message: String,
}

fn unknown() -> ErrorCode {
    ErrorCode::Unknown(String::new())
}

/// Decodes one response frame.
pub fn decode_answer(text: &str) -> Result<OrderAnswer, BadFrame> {
    let bad = |reason: String| BadFrame { reason, text: text.chars().take(500).collect() };
    let frame: In = serde_json::from_str(text).map_err(|error| bad(error.to_string()))?;
    let unreadable = |error: ApiError| bad(error.to_string());
    Ok(match frame.kind.as_str() {
        "pong" => OrderAnswer::Pong,
        "place" => {
            let (path, payload) = ("ws place", frame.payload.to_string());
            OrderAnswer::Place { id: frame.id, results: decode_batch(path, 200, &payload).map_err(unreadable)? }
        }
        "cancel" => {
            let (path, payload) = ("ws cancel", frame.payload.to_string());
            OrderAnswer::Cancel { id: frame.id, results: decode_batch(path, 200, &payload).map_err(unreadable)? }
        }
        "mass_cancel" => {
            let (path, payload) = ("ws mass_cancel", frame.payload.to_string());
            OrderAnswer::MassCancel { id: frame.id, results: decode_batch(path, 200, &payload).map_err(unreadable)? }
        }
        "error" => {
            let rejection: Rejection = serde_json::from_value(frame.payload).map_err(|error| bad(error.to_string()))?;
            let (code, message) = rejection.data.map_or((unknown(), String::new()), |data| (data.code, data.message));
            let client_order_ids = rejection
                .client_order_ids
                .iter()
                .filter_map(|id| match id {
                    serde_json::Value::String(text) => text.parse().ok(),
                    serde_json::Value::Number(number) => number.as_u64(),
                    _ => None,
                })
                .collect();
            OrderAnswer::Rejected { id: frame.id, code, message, client_order_ids }
        }
        other => return Err(bad(format!("unknown answer type `{other}`"))),
    })
}

/// Error from an order socket request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrderSocketError {
    /// The connection dropped with the request in flight, so it may have been executed. Read the
    /// account to check.
    #[error("the order socket was lost with the request in flight: {0}")]
    Lost(String),
    /// The socket could not be opened; nothing was sent.
    #[error("the order socket could not open: {0}")]
    Unreachable(String),
    /// Rejected before it entered the write pipeline; nothing was executed.
    #[error("refused ({code}): {message}")]
    Rejected { code: crate::client::ErrorCode, message: String },
    #[error("an offline build sends no writes")]
    Offline,
    #[error("{0}")]
    Invalid(String),
    #[error("the order socket has stopped")]
    Stopped,
}

#[cfg(test)]
#[path = "tests/orders.rs"]
mod tests;
