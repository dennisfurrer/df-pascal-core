//! Trade candles (OHLCV, TradingView format) and sparse mark price candles, for one market or up
//! to 20. Ranges end before `end_time_ms`; without `start_time_ms`, `count_back` candles are
//! returned (1 to 500, default 100).

use serde::{Deserialize, Serialize};

use crate::client::{ApiError, Call, Reply, Transport, read};
use crate::utils::dec::Dec;
use crate::utils::lenient;
use crate::utils::order::Interval;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candle {
    /// Open time.
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub time: u64,
    pub open: Dec,
    pub high: Dec,
    pub low: Dec,
    pub close: Dec,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub volume: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub taker_buy_volume: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub taker_sell_volume: u64,
    #[serde(default)]
    pub volume_notional: Dec,
    #[serde(default)]
    pub taker_buy_volume_notional: Dec,
    #[serde(default)]
    pub taker_sell_volume_notional: Dec,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub trade_count: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub taker_buy_count: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub taker_sell_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkPriceCandle {
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub time: u64,
    pub open: Dec,
    pub high: Dec,
    pub low: Dec,
    pub close: Dec,
}

/// A candle query's range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub interval: Interval,
    /// Exclusive.
    pub end_time_ms: u64,
    /// Inclusive. If `None`, `count_back` is used.
    pub start_time_ms: Option<u64>,
    pub count_back: Option<u32>,
}

impl Range {
    /// The last `count` candles before `end_time_ms`.
    #[must_use]
    pub const fn last(interval: Interval, end_time_ms: u64, count: u32) -> Range {
        Range { interval, end_time_ms, start_time_ms: None, count_back: Some(count) }
    }

    /// Every candle from `start_time_ms` up to `end_time_ms`.
    #[must_use]
    pub const fn between(interval: Interval, start_time_ms: u64, end_time_ms: u64) -> Range {
        Range { interval, end_time_ms, start_time_ms: Some(start_time_ms), count_back: None }
    }

    fn apply(&self, call: Call) -> Call {
        call.field("interval", &self.interval)
            .field("end_time_ms", self.end_time_ms)
            .maybe("start_time_ms", self.start_time_ms)
            .maybe("count_back", self.count_back)
    }
}

pub async fn trade_candles(
    client: &impl Transport,
    symbol: &str,
    range: &Range,
) -> Result<Reply<Vec<Candle>>, ApiError> {
    read(client, range.apply(Call::get("/api/v1/trade-candles").field("symbol", symbol))).await
}

pub async fn mark_price_candles(
    client: &impl Transport,
    symbol: &str,
    range: &Range,
) -> Result<Reply<Vec<MarkPriceCandle>>, ApiError> {
    read(client, range.apply(Call::get("/api/v1/mark-price-candles").field("symbol", symbol))).await
}

/// One symbol's mark price candles in a batch, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkPriceCandles {
    pub symbol: String,
    #[serde(default, deserialize_with = "lenient::list")]
    pub candles: Vec<MarkPriceCandle>,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Items<T> {
    #[serde(default, deserialize_with = "lenient::list")]
    items: Vec<T>,
}

/// Mark price candles for up to 20 symbols over one range, at most 200 per symbol. Unknown
/// symbols are omitted; the rest keep the request order.
pub async fn mark_price_candles_batch(
    client: &impl Transport,
    symbols: &[&str],
    range: &Range,
) -> Result<Reply<Vec<MarkPriceCandles>>, ApiError> {
    let call = range.apply(Call::get("/api/v1/mark-price-candles/batch").field("symbols", symbols.join(",")));
    Ok(read::<Items<MarkPriceCandles>>(client, call).await?.map(|items| items.items))
}
