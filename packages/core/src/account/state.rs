//! Account state: free collateral, open orders, positions, pending deposits and withdrawals,
//! reward totals and lifetime volume. The account websocket channel sends the same shape with
//! only changed fields.

use serde::{Deserialize, Serialize};

use crate::client::http::calls::segment;
use crate::client::{ApiError, Call, Reply, Transport, read};
use crate::trade::order::Order;
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::lenient;
use crate::utils::order::TransferStatus;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AccountState {
    /// True once the first deposit is credited, even after everything is withdrawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_deposited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub twitter_account_connection: Option<TwitterConnection>,
    /// Free collateral. On the websocket, absent when unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collateral_usd: Option<Dec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewards: Option<Rewards>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime_stats: Option<LifetimeStats>,
    /// Open orders. On the websocket, only changed orders.
    #[serde(default, deserialize_with = "lenient::list")]
    pub orders: Vec<Order>,
    /// Open positions. On the websocket, only changed positions; a closed position has size 0.
    #[serde(default, deserialize_with = "lenient::list")]
    pub positions: Vec<Position>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub pending_deposits: Vec<Deposit>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub pending_withdrawals: Vec<PendingWithdrawal>,
}

impl AccountState {
    /// The position in `symbol`, if open.
    #[must_use]
    pub fn position(&self, symbol: &str) -> Option<&Position> {
        self.positions.iter().find(|position| position.symbol == symbol && position.size != 0)
    }

    /// Open orders in `symbol`.
    pub fn orders_in<'a>(&'a self, symbol: &'a str) -> impl Iterator<Item = &'a Order> + 'a {
        self.orders.iter().filter(move |order| order.symbol == symbol)
    }

    /// Total unrealized PnL of all positions.
    #[must_use]
    pub fn unrealized_pnl(&self) -> Dec {
        Dec::checked_sum(self.positions.iter().map(|position| position.unrealized_pnl_usd)).unwrap_or(Dec::ZERO)
    }

    /// Value of open positions at mark: a long is `size * mark`, a short `|size| * (1 - mark)`.
    #[must_use]
    pub fn positions_value(&self) -> Dec {
        Dec::checked_sum(self.positions.iter().map(Position::value)).unwrap_or(Dec::ZERO)
    }

    /// Free collateral plus position value at mark. This is the account value in Pascal's charts.
    #[must_use]
    pub fn account_value(&self) -> Dec {
        self.collateral_usd.unwrap_or(Dec::ZERO).checked_add(self.positions_value()).unwrap_or(Dec::ZERO)
    }
}

/// The X account connected to an owner. Identify it by `twitter_user_id`; `twitter_handle` can
/// be out of date after a rename.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TwitterConnection {
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub twitter_user_id: u64,
    pub twitter_handle: String,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub connect_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rewards {
    #[serde(default)]
    pub referral_rewards_received_usd: Dec,
    #[serde(default)]
    pub builder_fees_received_usd: Dec,
    #[serde(default)]
    pub user_rewards_received_usd: Dec,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LifetimeStats {
    pub maker: SideStats,
    pub taker: SideStats,
}

impl LifetimeStats {
    /// Maker plus taker weighted volume, as used for referral eligibility.
    #[must_use]
    pub fn weighted_volume(&self) -> Option<Dec> {
        self.maker.weighted_volume?.checked_add(self.taker.weighted_volume?)
    }

    /// Net venue fees paid, after maker rebates.
    #[must_use]
    pub fn fees_paid(&self) -> Dec {
        self.maker.fees_paid_usd.checked_add(self.taker.fees_paid_usd).unwrap_or(Dec::ZERO)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SideStats {
    #[serde(default, deserialize_with = "lenient::u128s", serialize_with = "lenient::u128_text")]
    pub volume_contracts: u128,
    /// Absent until historical weighted volume is complete.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_volume: Option<Dec>,
    #[serde(default)]
    pub volume_notional_usd: Dec,
    #[serde(default)]
    pub fees_paid_usd: Dec,
}

/// A position. A position instance opens when size moves away from zero and closes when size
/// reaches or crosses zero. Use `unrealized_pnl_usd` for PnL, since `average_entry_price` is
/// floored to six decimal places.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    /// Signed: positive long, negative short.
    #[serde(deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub size: i64,
    #[serde(default, deserialize_with = "lenient::u128s", serialize_with = "lenient::u128_text")]
    pub cumulative_increasing_contracts: u128,
    #[serde(default)]
    pub cumulative_increasing_notional_usd: Dec,
    #[serde(default, deserialize_with = "lenient::u128s", serialize_with = "lenient::u128_text")]
    pub cumulative_reducing_contracts: u128,
    #[serde(default)]
    pub cumulative_reducing_notional_usd: Dec,
    #[serde(default)]
    pub cumulative_fees_usd: Dec,
    #[serde(default)]
    pub realized_pnl_usd: Dec,
    #[serde(default)]
    pub unrealized_pnl_usd: Dec,
    #[serde(default)]
    pub mark_price: Dec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub average_entry_price: Option<Dec>,
    /// Entry cost of the open size. Not in Pascal's docs, but sent by the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_entry_notional_usd: Option<Dec>,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub update_seq: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub open_ts_ms: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub update_ts_ms: u64,
}

impl Position {
    #[must_use]
    pub const fn is_long(&self) -> bool {
        self.size > 0
    }

    /// Value at mark: a long is `size * mark`, a short `|size| * (1 - mark)`.
    #[must_use]
    pub fn value(&self) -> Dec {
        let contracts = Dec::from_int(self.size.saturating_abs());
        let per = match self.size >= 0 {
            true => self.mark_price,
            false => Dec::ONE.checked_sub(self.mark_price).unwrap_or(Dec::ZERO),
        };
        contracts.checked_mul(per).unwrap_or(Dec::ZERO)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deposit {
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub deposit_id: u64,
    pub owner: Pubkey,
    pub amount: Dec,
    pub transfer_status: TransferStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingWithdrawal {
    pub owner: Pubkey,
    pub status: TransferStatus,
    pub amount: Dec,
    /// The destination wallet, not its token account.
    pub destination_authority: Pubkey,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub chain_seq: Option<u64>,
}

/// Filters for an account state read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StateQuery {
    /// Return orders and positions for this market only. Collateral is always account-wide.
    pub symbol: Option<String>,
    /// Return an empty `orders` list.
    pub exclude_orders: bool,
}

/// The account's state. It can be up to a second old; the account websocket channel is real
/// time.
pub async fn state(
    client: &impl Transport,
    owner: &Pubkey,
    query: &StateQuery,
) -> Result<Reply<AccountState>, ApiError> {
    let call = Call::get(format!("/api/v1/accounts/{}", segment(&owner.to_base58())))
        .maybe("symbol", query.symbol.as_deref())
        .maybe("exclude_orders", query.exclude_orders.then_some("true"));
    read(client, call).await
}
