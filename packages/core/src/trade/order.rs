//! Order, fill and placement types returned by Pascal.

use serde::{Deserialize, Serialize};

use crate::utils::dec::{Dec, Round};
use crate::utils::keys::Pubkey;
use crate::utils::lenient;
use crate::utils::order::{Liquidity, OrderType, Side, TimeInForce, UpdateReason};

/// An order's state. `size_remaining` is the open size (FIX `LeavesQty`). The order is closed
/// when it reaches zero whether or not it filled, so use `size_filled` for fills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    /// Exchange-assigned order id.
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub id: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub client_order_id: u64,
    #[serde(
        default,
        deserialize_with = "lenient::opt_u64s",
        serialize_with = "lenient::opt_u64_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub replace_client_order_id: Option<u64>,
    pub symbol: String,
    pub side: Side,
    pub price: Dec,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub size_original: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub size_remaining: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub size_filled: u64,
    /// The sum of fill price * fill size, in USD.
    #[serde(default)]
    pub notional_filled: Dec,
    pub tif: TimeInForce,
    #[serde(rename = "type")]
    pub order_type: OrderType,
    #[serde(default)]
    pub post_only: bool,
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(
        default,
        deserialize_with = "lenient::opt_u64s",
        serialize_with = "lenient::opt_u64_text",
        skip_serializing_if = "Option::is_none"
    )]
    pub expires_ts_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_pubkey: Option<Pubkey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_fee_rate: Option<Dec>,
    /// Sequence number at placement.
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub update_seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub place_ts_ms: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub update_ts_ms: u64,
}

impl Order {
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.size_remaining > 0
    }

    /// Average fill price, or `None` if nothing filled.
    #[must_use]
    pub fn average_fill_price(&self) -> Option<Dec> {
        if self.size_filled == 0 {
            return None;
        }
        self.notional_filled.checked_div(Dec::from_int(i64::try_from(self.size_filled).ok()?), 6, Round::Nearest)
    }
}

/// An order's sizes as of one fill. A reduce-only order can be resized, so filled plus remaining
/// can differ from the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderProgress {
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub size_original: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub size_filled: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub size_remaining: u64,
}

/// One account's side of a trade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub trade_id: u64,
    pub symbol: String,
    pub side: Side,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<bool>,
    pub fill_price: Dec,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub fill_size: u64,
    pub liquidity: Liquidity,
    /// The owner on the other side of the trade.
    pub counterparty: Pubkey,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub order_id: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub client_order_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_progress: Option<OrderProgress>,
    /// Venue fee, excluding builder fees. Positive is paid, negative is a rebate.
    pub fee_usd: Dec,
    /// Position collateral change less this side's fees.
    pub collateral_change_usd: Dec,
    #[serde(default, deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub position_size_prev: i64,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_pubkey: Option<Pubkey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub builder_fee_paid_usd: Option<Dec>,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub trade_ts_ms: u64,
}

impl Fill {
    /// Signed position size after this fill.
    #[must_use]
    pub fn position_size_after(&self) -> i64 {
        let size = i64::try_from(self.fill_size).unwrap_or(i64::MAX);
        match self.side {
            Side::Ask => self.position_size_prev.saturating_sub(size),
            _ => self.position_size_prev.saturating_add(size),
        }
    }
}

/// Response to a placement: the order after placement, its immediate fills, and the update
/// reason if the order changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placed {
    pub order: Order,
    #[serde(default, deserialize_with = "lenient::list")]
    pub fills: Vec<Fill>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_reason: Option<UpdateReason>,
}

/// An order update from the account websocket channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderUpdate {
    pub order: Order,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_reason: Option<UpdateReason>,
}
