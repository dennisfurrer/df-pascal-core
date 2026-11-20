//! The market list from a REST read, kept current with `market_update` messages.
//!
//! Subscribe first, then fetch `GET /markets`. [`Markets`] holds updates received before the
//! REST result and applies only those with a `seq` above it.

use std::collections::BTreeMap;

use crate::live::frame::MarketUpdate;
use crate::markets::info::Market;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Markets {
    markets: BTreeMap<String, Market>,
    held: Vec<(u64, MarketUpdate)>,
    seq: Option<u64>,
}

impl Markets {
    /// Sets the list from a REST read at `seq` and applies held updates with a later `seq`.
    pub fn snapshot(&mut self, markets: Vec<Market>, seq: u64) {
        self.markets = markets.into_iter().map(|market| (market.symbol.clone(), market)).collect();
        self.seq = Some(seq);
        for (at, update) in std::mem::take(&mut self.held) {
            if at > seq {
                self.merge(update);
            }
        }
    }

    /// Applies one update at `seq`, or holds it until the REST read arrives.
    pub fn update(&mut self, update: MarketUpdate, seq: u64) {
        match self.seq {
            None => self.held.push((seq, update)),
            Some(_) => {
                self.seq = Some(seq);
                self.merge(update);
            }
        }
    }

    fn merge(&mut self, update: MarketUpdate) {
        let symbol = update.market.symbol.clone();
        let mut market = update.market;
        // The websocket omits stats and the best bid and ask, so keep the REST values.
        if let Some(held) = self.markets.get(&symbol) {
            if market.stats.is_none() {
                market.stats.clone_from(&held.stats);
            }
            if market.best_bid.is_none() {
                market.best_bid = held.best_bid;
            }
            if market.best_ask.is_none() {
                market.best_ask = held.best_ask;
            }
        }
        self.markets.insert(symbol, market);
    }

    #[must_use]
    pub fn get(&self, symbol: &str) -> Option<&Market> {
        self.markets.get(symbol)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Market> {
        self.markets.values()
    }

    /// Markets that are not resolved.
    pub fn live(&self) -> impl Iterator<Item = &Market> {
        self.markets.values().filter(|market| !market.is_resolved())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.markets.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.markets.is_empty()
    }
}
