//! Order books, best price first, with the market spec on first fetch. A read can be up to a
//! second old; the `book` websocket channel is real time.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::client::{ApiError, Call, Reply, Transport, read};
use crate::markets::info::MarketSpec;
use crate::utils::dec::Dec;
use crate::utils::lenient;

/// Maximum symbols per request.
pub const MAX_SYMBOLS: usize = 50;

/// A price level: price and resting contracts. Serialized as `["0.550000", "12"]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub price: Dec,
    pub size: u64,
}

impl Serialize for Level {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.price.to_fixed(6), self.size.to_string()).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (price, size): (Dec, String) = Deserialize::deserialize(deserializer)?;
        let size = size.trim().parse().map_err(serde::de::Error::custom)?;
        Ok(Level { price, size })
    }
}

/// Asks lowest first, bids highest first. In a websocket update, a level with size 0 was
/// removed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OrderBook {
    #[serde(default, deserialize_with = "lenient::list")]
    pub asks: Vec<Level>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub bids: Vec<Level>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<MarketSpec>,
}

impl OrderBook {
    #[must_use]
    pub fn best_bid(&self) -> Option<Level> {
        self.bids.first().copied()
    }

    #[must_use]
    pub fn best_ask(&self) -> Option<Level> {
        self.asks.first().copied()
    }

    /// Midpoint of the best bid and ask, if both exist.
    #[must_use]
    pub fn mid(&self) -> Option<Dec> {
        let sum = self.best_bid()?.price.checked_add(self.best_ask()?.price)?;
        sum.checked_div(Dec::from_int(2), 7, crate::utils::dec::Round::Nearest)
    }

    #[must_use]
    pub fn spread(&self) -> Option<Dec> {
        self.best_ask()?.price.checked_sub(self.best_bid()?.price)
    }
}

#[derive(Deserialize)]
struct Books {
    books: BTreeMap<String, OrderBook>,
}

/// Books for up to 50 symbols, keyed by symbol.
pub async fn books(client: &impl Transport, symbols: &[&str]) -> Result<Reply<BTreeMap<String, OrderBook>>, ApiError> {
    if symbols.is_empty() || symbols.len() > MAX_SYMBOLS {
        return Err(ApiError::Invalid(crate::utils::permit::BadPermit::Field(format!(
            "books take 1 to {MAX_SYMBOLS} symbols"
        ))));
    }
    let call = Call::get("/api/v1/books").field("symbols", symbols.join(","));
    Ok(read::<Books>(client, call).await?.map(|books| books.books))
}

/// One symbol's book; empty if Pascal returns none.
pub async fn book(client: &impl Transport, symbol: &str) -> Result<Reply<OrderBook>, ApiError> {
    Ok(books(client, &[symbol]).await?.map(|mut books| books.remove(symbol).unwrap_or_default()))
}
