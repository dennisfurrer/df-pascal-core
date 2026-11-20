//! Order book built from the `book` channel: the snapshot, then each update's levels applied
//! (size 0 removes a level).

use std::collections::BTreeMap;

use crate::live::frame::Kind;
use crate::markets::book::{Level, OrderBook};
use crate::markets::info::MarketSpec;
use crate::utils::dec::Dec;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Book {
    bids: BTreeMap<Dec, u64>,
    asks: BTreeMap<Dec, u64>,
    spec: Option<MarketSpec>,
    seq: Option<u64>,
    ready: bool,
}

/// The result of applying a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// A snapshot replaced the book.
    Reset,
    /// An update changed the book.
    Changed,
    /// An update before any snapshot, or older than the current state, was ignored.
    Ignored,
}

impl Book {
    /// Applies a `book` message at `seq`.
    pub fn apply(&mut self, kind: Kind, book: &OrderBook, seq: Option<u64>) -> Applied {
        match kind {
            Kind::Snapshot => {
                self.bids =
                    book.bids.iter().filter(|level| level.size > 0).map(|level| (level.price, level.size)).collect();
                self.asks =
                    book.asks.iter().filter(|level| level.size > 0).map(|level| (level.price, level.size)).collect();
                if book.spec.is_some() {
                    self.spec.clone_from(&book.spec);
                }
                self.seq = seq;
                self.ready = true;
                Applied::Reset
            }
            Kind::Update => {
                if !self.ready || matches!((seq, self.seq), (Some(new), Some(held)) if new < held) {
                    return Applied::Ignored;
                }
                for level in &book.bids {
                    set(&mut self.bids, level);
                }
                for level in &book.asks {
                    set(&mut self.asks, level);
                }
                if book.spec.is_some() {
                    self.spec.clone_from(&book.spec);
                }
                if seq.is_some() {
                    self.seq = seq;
                }
                Applied::Changed
            }
        }
    }

    #[must_use]
    pub const fn is_ready(&self) -> bool {
        self.ready
    }

    #[must_use]
    pub const fn seq(&self) -> Option<u64> {
        self.seq
    }

    #[must_use]
    pub const fn spec(&self) -> Option<&MarketSpec> {
        self.spec.as_ref()
    }

    /// Bids, highest first.
    pub fn bids(&self) -> impl Iterator<Item = Level> + '_ {
        self.bids.iter().rev().map(|(price, size)| Level { price: *price, size: *size })
    }

    /// Asks, lowest first.
    pub fn asks(&self) -> impl Iterator<Item = Level> + '_ {
        self.asks.iter().map(|(price, size)| Level { price: *price, size: *size })
    }

    #[must_use]
    pub fn best_bid(&self) -> Option<Level> {
        self.bids().next()
    }

    #[must_use]
    pub fn best_ask(&self) -> Option<Level> {
        self.asks().next()
    }

    /// The book as an [`OrderBook`] snapshot with up to `depth` levels per side.
    #[must_use]
    pub fn to_order_book(&self, depth: usize) -> OrderBook {
        OrderBook {
            bids: self.bids().take(depth).collect(),
            asks: self.asks().take(depth).collect(),
            spec: self.spec.clone(),
        }
    }
}

fn set(side: &mut BTreeMap<Dec, u64>, level: &Level) {
    match level.size {
        0 => {
            side.remove(&level.price);
        }
        size => {
            side.insert(level.price, size);
        }
    }
}
