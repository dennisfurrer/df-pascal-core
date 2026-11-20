//! Public trades with both owners: paged history, or the latest trades for up to 20 markets.

use serde::{Deserialize, Serialize};

use crate::client::{ApiError, Call, Reply, Transport, read};
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::lenient;
use crate::utils::order::Side;
use crate::utils::page::{Page, PageRequest};

/// A public trade. One account's side of a trade is a [`crate::trade::Fill`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTrade {
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub trade_id: u64,
    pub symbol: String,
    /// The side that took liquidity.
    pub taker_side: Side,
    pub price: Dec,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub size: u64,
    pub maker: Pubkey,
    pub taker: Pubkey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maker_reduce_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taker_reduce_only: Option<bool>,
    #[serde(default, deserialize_with = "lenient::opt_i64s", serialize_with = "lenient::opt_i64_text")]
    pub maker_position_size_prev: Option<i64>,
    #[serde(default, deserialize_with = "lenient::opt_i64s", serialize_with = "lenient::opt_i64_text")]
    pub taker_position_size_prev: Option<i64>,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub trade_ts_ms: u64,
}

/// Maximum symbols per trade history request.
pub const MAX_SYMBOLS: usize = 120;

/// One page of trades in up to 120 markets, newest first. History includes every trade with
/// `seq` at or below the response's `seq`.
pub async fn trades(
    client: &impl Transport,
    symbols: &[&str],
    page: &PageRequest,
) -> Result<Reply<Page<PublicTrade>>, ApiError> {
    read(client, page.apply(Call::get("/api/v1/trades").field("symbols", symbols.join(",")))).await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentTrades {
    pub symbol: String,
    #[serde(default, deserialize_with = "lenient::list")]
    pub trades: Vec<PublicTrade>,
}

#[derive(Deserialize)]
struct Items {
    #[serde(default, deserialize_with = "lenient::list")]
    items: Vec<RecentTrades>,
}

/// The latest `limit` trades (1 to 50, 10 by default) for up to 20 markets, newest first.
pub async fn recent(
    client: &impl Transport,
    symbols: &[&str],
    limit: Option<u32>,
) -> Result<Reply<Vec<RecentTrades>>, ApiError> {
    let call = Call::get("/api/v1/trades/batch").field("symbols", symbols.join(",")).maybe("limit", limit);
    Ok(read::<Items>(client, call).await?.map(|items| items.items))
}
