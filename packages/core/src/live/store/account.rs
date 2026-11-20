//! Account state built from the `account` channel: the snapshot, then each update merged in.
//! Closed orders, zero-size positions and finished transfers are removed; each update's events
//! are returned to the caller.

use std::collections::BTreeMap;

use crate::account::state::{AccountState, Deposit, LifetimeStats, PendingWithdrawal, Position, Rewards};
use crate::live::account::AccountUpdate;
use crate::live::frame::Kind;
use crate::trade::order::Order;
use crate::utils::dec::Dec;
use crate::utils::order::TransferStatus;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Account {
    pub has_deposited: bool,
    pub username: Option<String>,
    pub collateral_usd: Dec,
    pub rewards: Option<Rewards>,
    pub lifetime_stats: Option<LifetimeStats>,
    /// Open orders by exchange id.
    pub orders: BTreeMap<u64, Order>,
    /// Open positions by symbol.
    pub positions: BTreeMap<String, Position>,
    pub pending_deposits: BTreeMap<u64, Deposit>,
    pub pending_withdrawals: Vec<PendingWithdrawal>,
    pub seq: Option<u64>,
    ready: bool,
}

impl Account {
    /// Applies an `account` message at `seq` and returns the update so the caller can read its
    /// events.
    pub fn apply<'a>(&mut self, kind: Kind, update: &'a AccountUpdate, seq: Option<u64>) -> &'a AccountUpdate {
        let state = &update.state;
        if kind == Kind::Snapshot {
            *self = Account { ready: true, ..Account::default() };
            self.username.clone_from(&state.username);
        }
        if let Some(has_deposited) = state.has_deposited {
            self.has_deposited = has_deposited;
        }
        if let Some(collateral) = state.collateral_usd {
            self.collateral_usd = collateral;
        }
        if state.rewards.is_some() {
            self.rewards.clone_from(&state.rewards);
        }
        if state.lifetime_stats.is_some() {
            self.lifetime_stats.clone_from(&state.lifetime_stats);
        }
        for order in state.orders.iter().chain(update.order_updates.iter().map(|change| &change.order)) {
            match order.is_open() {
                true => {
                    self.orders.insert(order.id, order.clone());
                }
                false => {
                    self.orders.remove(&order.id);
                }
            }
        }
        for position in &state.positions {
            match position.size {
                0 => {
                    self.positions.remove(&position.symbol);
                }
                _ => {
                    self.positions.insert(position.symbol.clone(), position.clone());
                }
            }
        }
        for deposit in &state.pending_deposits {
            match finished(&deposit.transfer_status) {
                true => {
                    self.pending_deposits.remove(&deposit.deposit_id);
                }
                false => {
                    self.pending_deposits.insert(deposit.deposit_id, deposit.clone());
                }
            }
        }
        for withdrawal in &state.pending_withdrawals {
            let same = |held: &PendingWithdrawal| {
                held.amount == withdrawal.amount
                    && held.destination_authority == withdrawal.destination_authority
                    && (held.chain_seq.is_none() || held.chain_seq == withdrawal.chain_seq)
            };
            self.pending_withdrawals.retain(|held| !same(held));
            if !finished(&withdrawal.status) {
                self.pending_withdrawals.push(withdrawal.clone());
            }
        }
        if seq.is_some() {
            self.seq = seq;
        }
        update
    }

    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.ready
    }

    /// The current state in the REST `AccountState` shape.
    #[must_use]
    pub fn to_state(&self) -> AccountState {
        AccountState {
            has_deposited: Some(self.has_deposited),
            username: self.username.clone(),
            twitter_account_connection: None,
            collateral_usd: Some(self.collateral_usd),
            rewards: self.rewards.clone(),
            lifetime_stats: self.lifetime_stats.clone(),
            orders: self.orders.values().cloned().collect(),
            positions: self.positions.values().cloned().collect(),
            pending_deposits: self.pending_deposits.values().cloned().collect(),
            pending_withdrawals: self.pending_withdrawals.clone(),
        }
    }
}

fn finished(status: &TransferStatus) -> bool {
    matches!(status, TransferStatus::Completed | TransferStatus::Rejected)
}
