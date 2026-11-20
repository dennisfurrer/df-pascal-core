//! Decoding market data websocket frames into [`Frame`]s: a channel snapshot or update, an
//! error, or a pong.

use serde::Deserialize;

use crate::client::{Context, ErrorCode};
use crate::live::account::AccountUpdate;
use crate::live::channel::Channel;
use crate::markets::book::OrderBook;
use crate::markets::candles::{Candle, MarkPriceCandle};
use crate::markets::info::Market;
use crate::markets::trades::PublicTrade;
use crate::utils::lenient;
use crate::utils::order::MarketUpdateType;

/// Whether a message is a channel's initial state or a change to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Snapshot,
    Update,
}

/// A market that was listed, changed or resolved.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MarketUpdate {
    pub update_type: MarketUpdateType,
    pub market: Market,
}

/// The payload of a channel message.
#[derive(Debug, Clone, PartialEq)]
pub enum Data {
    Book(OrderBook),
    Trades(Vec<PublicTrade>),
    Candles(Vec<Candle>),
    MarkPriceCandles(Vec<MarkPriceCandle>),
    Account(Box<AccountUpdate>),
    Markets(Vec<MarketUpdate>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Message {
        channel: Channel,
        kind: Kind,
        data: Data,
        context: Context,
    },
    /// A rejected request, or a channel that can no longer be served.
    Error {
        channel: Option<Channel>,
        code: ErrorCode,
        message: String,
        context: Context,
    },
    Pong,
}

/// A frame that could not be decoded, and the reason. The socket is still open.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unreadable frame ({reason}): {text}")]
pub struct BadFrame {
    pub reason: String,
    pub text: String,
}

#[derive(Deserialize)]
struct Head {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    data: serde_json::Value,
    #[serde(flatten)]
    context: Context,
}

#[derive(Deserialize)]
struct ErrorData {
    #[serde(default = "unknown")]
    code: ErrorCode,
    #[serde(default, deserialize_with = "lenient::text")]
    message: String,
}

fn unknown() -> ErrorCode {
    ErrorCode::Unknown(String::new())
}

/// Decodes one text frame.
pub fn decode(text: &str) -> Result<Frame, BadFrame> {
    let bad = |reason: String| BadFrame { reason, text: text.chars().take(500).collect() };
    let value: serde_json::Value = serde_json::from_str(text).map_err(|error| bad(error.to_string()))?;
    let head: Head = serde_json::from_value(value.clone()).map_err(|error| bad(error.to_string()))?;
    if head.kind == "pong" {
        return Ok(Frame::Pong);
    }
    let channel: Option<Channel> = serde_json::from_value(value).ok();
    if head.kind == "error" {
        let error: ErrorData =
            serde_json::from_value(head.data).unwrap_or(ErrorData { code: unknown(), message: String::new() });
        return Ok(Frame::Error { channel, code: error.code, message: error.message, context: head.context });
    }
    let kind = match head.kind.as_str() {
        "snapshot" => Kind::Snapshot,
        "update" => Kind::Update,
        other => return Err(bad(format!("unknown message type `{other}`"))),
    };
    let channel = channel.ok_or_else(|| bad("no channel".to_owned()))?;
    let read = |error: serde_json::Error| bad(error.to_string());
    let data = match &channel {
        Channel::Book { .. } => Data::Book(serde_json::from_value(head.data).map_err(read)?),
        Channel::Trades { .. } | Channel::AllTrades | Channel::FilteredTrades => {
            Data::Trades(lenient_list(head.data).map_err(read)?)
        }
        Channel::Candles { .. } => Data::Candles(lenient_list(head.data).map_err(read)?),
        Channel::MarkPriceCandles { .. } => Data::MarkPriceCandles(lenient_list(head.data).map_err(read)?),
        Channel::Account { .. } => Data::Account(Box::new(serde_json::from_value(head.data).map_err(read)?)),
        Channel::MarketUpdate => Data::Markets(lenient_list(head.data).map_err(read)?),
    };
    Ok(Frame::Message { channel, kind, data, context: head.context })
}

fn lenient_list<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<Vec<T>, serde_json::Error> {
    match value {
        serde_json::Value::Null => Ok(Vec::new()),
        other => serde_json::from_value(other),
    }
}
