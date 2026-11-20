//! Account history: fills, transfers, position resolutions, collateral returns and closed
//! positions (paged, newest first), account value and PnL charts, and PnL movers.
//!
//! A request past the indexed range fails with `HISTORY_NOT_READY` (HTTP 503); retry it after a
//! short wait.

use serde::{Deserialize, Serialize};

use crate::client::http::calls::segment;
use crate::client::{ApiError, Call, Reply, Transport, read};
use crate::trade::collateral::CollateralReturn;
use crate::trade::order::Fill;
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::lenient;
use crate::utils::order::{
    MoversOrderBy, MoversWindow, PositionOrderBy, PositionSide, SortOrder, Timeframe, TransferStatus, TransferType,
};
use crate::utils::page::{Page, PageRequest};

fn account_path(owner: &Pubkey, tail: &str) -> String {
    format!("/api/v1/accounts/{}/{tail}", segment(&owner.to_base58()))
}

fn symbols_field(call: Call, symbols: &[&str]) -> Call {
    match symbols.is_empty() {
        true => call,
        false => call.field("symbols", symbols.join(",")),
    }
}

/// The account's fills, optionally in up to 100 markets.
pub async fn fills(
    client: &impl Transport,
    owner: &Pubkey,
    symbols: &[&str],
    page: &PageRequest,
) -> Result<Reply<Page<Fill>>, ApiError> {
    read(client, page.apply(symbols_field(Call::get(account_path(owner, "fills")), symbols))).await
}

/// A deposit, withdrawal or reward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transfer {
    #[serde(rename = "type")]
    pub kind: TransferType,
    pub status: TransferStatus,
    /// For a withdrawal, the full debit including the fee.
    pub collateral_change_usd: Dec,
    #[serde(default)]
    pub fee_usd: Dec,
    /// For a withdrawal, the amount sent, as a negative number; otherwise the amount credited.
    #[serde(default)]
    pub net_usd: Dec,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub transfer_ts_ms: u64,
    #[serde(
        default,
        deserialize_with = "lenient::opt_u64s",
        serialize_with = "lenient::opt_u64_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub chain_seq: Option<u64>,
    /// Reward reason, for example `MIDTERMS_2026_TRADING_REWARD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_reward_reason: Option<String>,
}

/// Deposits, withdrawals and rewards.
pub async fn transfers(
    client: &impl Transport,
    owner: &Pubkey,
    page: &PageRequest,
) -> Result<Reply<Page<Transfer>>, ApiError> {
    read(client, page.apply(Call::get(account_path(owner, "transfers")))).await
}

/// A position settled by its market's resolution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PositionResolution {
    pub symbol: String,
    /// The settlement price.
    pub resolution: Dec,
    #[serde(deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub size: i64,
    pub collateral_change_usd: Dec,
    #[serde(default)]
    pub realized_pnl_usd: Dec,
    #[serde(
        default,
        deserialize_with = "lenient::opt_u64s",
        serialize_with = "lenient::opt_u64_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub position_open_seq: Option<u64>,
    #[serde(
        default,
        deserialize_with = "lenient::opt_u64s",
        serialize_with = "lenient::opt_u64_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub position_open_ts_ms: Option<u64>,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub position_resolution_ts_ms: u64,
}

pub async fn position_resolutions(
    client: &impl Transport,
    owner: &Pubkey,
    symbols: &[&str],
    page: &PageRequest,
) -> Result<Reply<Page<PositionResolution>>, ApiError> {
    read(client, page.apply(symbols_field(Call::get(account_path(owner, "position-resolutions")), symbols))).await
}

/// Collateral returns, each with every position it changed. With a symbol filter, returns that
/// changed any of the symbols are included.
pub async fn collateral_returns(
    client: &impl Transport,
    owner: &Pubkey,
    symbols: &[&str],
    page: &PageRequest,
) -> Result<Reply<Page<CollateralReturn>>, ApiError> {
    read(client, page.apply(symbols_field(Call::get(account_path(owner, "collateral-returns")), symbols))).await
}

/// A closed position, from the fill that opened it to the fill or resolution that closed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClosedPosition {
    pub symbol: String,
    pub position_side: PositionSide,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub position_instance_count: u64,
    #[serde(default)]
    pub held_until_resolution: bool,
    /// Excluding fees.
    pub realized_pnl_usd: Dec,
    #[serde(default, deserialize_with = "lenient::u128s", serialize_with = "lenient::u128_text")]
    pub cumulative_increasing_contracts: u128,
    #[serde(default)]
    pub cumulative_increasing_notional_usd: Dec,
    #[serde(default)]
    pub cumulative_reducing_notional_usd: Dec,
    #[serde(default)]
    pub average_increasing_price: Dec,
    #[serde(default)]
    pub average_reducing_price: Dec,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub open_seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub open_ts_ms: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub close_seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub close_ts_ms: u64,
}

/// How to list closed positions. Keep `symbols`, `order_by` and `sort_order` the same while
/// following `before_cursor`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClosedQuery {
    pub symbols: Vec<String>,
    pub order_by: Option<PositionOrderBy>,
    pub sort_order: Option<SortOrder>,
    /// Up to 1,000 (default 1,000).
    pub limit: Option<u32>,
    pub before_cursor: Option<String>,
}

/// Closed positions. The result set is fixed at the first page, so no position is skipped or
/// repeated while paging.
pub async fn closed_positions(
    client: &impl Transport,
    owner: &Pubkey,
    query: &ClosedQuery,
) -> Result<Reply<Page<ClosedPosition>>, ApiError> {
    let mut call = Call::get(format!("/api/v2/accounts/{}/position-instances", segment(&owner.to_base58())));
    if !query.symbols.is_empty() {
        call = call.field("symbols", query.symbols.join(","));
    }
    let call = call
        .maybe("order_by", query.order_by.as_ref())
        .maybe("sort_order", query.sort_order.as_ref())
        .maybe("limit", query.limit)
        .maybe("before_cursor", query.before_cursor.as_deref());
    read(client, call).await
}

/// A chart point: unix ms timestamp and value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub ts_ms: u64,
    pub value: Dec,
}

impl Serialize for Point {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (self.ts_ms.to_string(), self.value).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Point {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (ts, value): (serde_json::Value, Dec) = Deserialize::deserialize(deserializer)?;
        let ts_ms = match ts {
            serde_json::Value::String(text) => text.parse().map_err(serde::de::Error::custom)?,
            serde_json::Value::Number(number) => {
                number.as_u64().ok_or_else(|| serde::de::Error::custom("a timestamp"))?
            }
            _ => return Err(serde::de::Error::custom("a timestamp")),
        };
        Ok(Point { ts_ms, value })
    }
}

/// Chart series for one timeframe. Total PnL is trading PnL plus referral rewards, builder fees
/// and user rewards.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HistorySeries {
    #[serde(default, deserialize_with = "lenient::list")]
    pub account_value_history: Vec<Point>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub total_pnl_history: Vec<Point>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub trading_pnl_history: Vec<Point>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub referral_rewards_received_history: Vec<Point>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub builder_fees_received_history: Vec<Point>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub user_rewards_received_history: Vec<Point>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeseries {
    pub timeframe: Timeframe,
    pub data: HistorySeries,
}

#[derive(Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Items<T> {
    #[serde(default, deserialize_with = "lenient::list")]
    items: Vec<T>,
}

/// Account value and PnL over a day (30-minute points), a week (4 hours), a month (12 hours)
/// and all time (daily). Points come from stored history up to the last closed 30-minute
/// boundary; the caller can add a current point from the account state.
pub async fn pnl_history(client: &impl Transport, owner: &Pubkey) -> Result<Reply<Vec<Timeseries>>, ApiError> {
    Ok(read::<Items<Timeseries>>(client, Call::get(account_path(owner, "history"))).await?.map(|items| items.items))
}

/// One market's contribution to the PnL change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketPnl {
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub market_id: u64,
    pub symbol: String,
    /// Realized and unrealized, after venue and builder fees.
    pub pnl_change_usd: Dec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_price: Option<Dec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_price_start: Option<Dec>,
    #[serde(default, deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub position_size_start: i64,
    #[serde(default, deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub position_size: i64,
    #[serde(default, deserialize_with = "lenient::u128s", serialize_with = "lenient::u128_text")]
    pub volume_contracts: u128,
    #[serde(default)]
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PnlMovers {
    pub owner: Pubkey,
    pub window: MoversWindow,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub start_ms: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub as_of_ms: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub total_markets: u64,
    pub total_pnl_change_usd: Dec,
    #[serde(default, deserialize_with = "lenient::list")]
    pub entries: Vec<MarketPnl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoversQuery {
    pub window: MoversWindow,
    /// 1 to 100; 5 by default.
    pub limit: Option<u32>,
    pub symbols: Vec<String>,
    pub order_by: Option<MoversOrderBy>,
    pub sort_order: Option<SortOrder>,
}

impl MoversQuery {
    #[must_use]
    pub const fn new(window: MoversWindow) -> MoversQuery {
        MoversQuery { window, limit: None, symbols: Vec::new(), order_by: None, sort_order: None }
    }
}

/// The markets with the largest trading PnL change over the window. Rate limit: 60 requests a
/// minute per IP.
pub async fn pnl_movers(
    client: &impl Transport,
    owner: &Pubkey,
    query: &MoversQuery,
) -> Result<Reply<PnlMovers>, ApiError> {
    let mut call =
        Call::get(account_path(owner, "pnl-movers")).field("window", &query.window).maybe("limit", query.limit);
    if !query.symbols.is_empty() {
        call = call.field("symbols", query.symbols.join(","));
    }
    read(client, call.maybe("order_by", query.order_by.as_ref()).maybe("sort_order", query.sort_order.as_ref())).await
}
