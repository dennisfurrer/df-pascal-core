//! Market data websocket channels and their subscribe and unsubscribe frames.

use serde::{Deserialize, Serialize};

use crate::utils::keys::Pubkey;
use crate::utils::order::Interval;

/// A market data channel. Subscribing to the same channel twice creates one subscription.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "channel", rename_all = "snake_case")]
pub enum Channel {
    /// One market's order book: a snapshot, then level changes (size 0 removes a level).
    Book { symbol: String },
    /// One market's trades.
    Trades { symbol: String },
    /// Trades in all markets: the last 100, then each new trade, oldest first.
    AllTrades,
    /// Larger trades across all markets.
    FilteredTrades,
    /// One market's current candle.
    Candles {
        symbol: String,
        #[serde(with = "interval")]
        interval: Interval,
    },
    /// One market's current mark price candle.
    MarkPriceCandles {
        symbol: String,
        #[serde(with = "interval")]
        interval: Interval,
    },
    /// One account: a snapshot of its state, then only the changes.
    Account {
        owner: Pubkey,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        exclude_orders: bool,
    },
    /// New listings, display changes, MECE designations and resolutions. The snapshot is empty, so
    /// fetch the markets over REST and apply updates with a later `seq`.
    MarketUpdate,
}

mod interval {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use crate::utils::order::Interval;

    pub(super) fn serialize<S: Serializer>(interval: &Interval, serializer: S) -> Result<S::Ok, S::Error> {
        interval.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Interval, D::Error> {
        Interval::deserialize(deserializer)
    }
}

impl Channel {
    #[must_use]
    pub fn book(symbol: impl Into<String>) -> Channel {
        Channel::Book { symbol: symbol.into() }
    }

    #[must_use]
    pub fn trades(symbol: impl Into<String>) -> Channel {
        Channel::Trades { symbol: symbol.into() }
    }

    #[must_use]
    pub fn candles(symbol: impl Into<String>, interval: Interval) -> Channel {
        Channel::Candles { symbol: symbol.into(), interval }
    }

    #[must_use]
    pub fn mark_price_candles(symbol: impl Into<String>, interval: Interval) -> Channel {
        Channel::MarkPriceCandles { symbol: symbol.into(), interval }
    }

    #[must_use]
    pub const fn account(owner: Pubkey) -> Channel {
        Channel::Account { owner, exclude_orders: false }
    }

    /// The channel's market symbol, if it has one.
    #[must_use]
    pub fn symbol(&self) -> Option<&str> {
        match self {
            Channel::Book { symbol }
            | Channel::Trades { symbol }
            | Channel::Candles { symbol, .. }
            | Channel::MarkPriceCandles { symbol, .. } => Some(symbol),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct Request<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    channels: &'a [Channel],
}

/// `{"type":"subscribe","channels":[..]}`.
#[must_use]
pub fn subscribe(channels: &[Channel]) -> String {
    serde_json::to_string(&Request { kind: "subscribe", channels }).unwrap_or_default()
}

/// `{"type":"unsubscribe","channels":[..]}`.
#[must_use]
pub fn unsubscribe(channels: &[Channel]) -> String {
    serde_json::to_string(&Request { kind: "unsubscribe", channels }).unwrap_or_default()
}

/// `{"type":"ping"}`; the server replies `{"type":"pong"}`.
pub const PING: &str = r#"{"type":"ping"}"#;
