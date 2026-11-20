//! Tick size, rounding to ticks, fees, weighted volume, collateral and Yes/No conversion. Prices
//! are $0 to $1; a long receives the settlement price and a short receives 1 minus it.

use crate::utils::dec::{Dec, Round};
use crate::utils::order::Side;

/// The tick at `price` for a market with `tick_size_min` and `tick_sig_figs`:
/// `max(10^(floor(log10(p')) + 1 - sig_figs), tick_size_min)` where `p' = min(p, 1 - p)`.
/// At 0 or 1 (and anything outside) it is `tick_size_min`.
#[must_use]
pub fn tick_size(price: Dec, tick_size_min: Dec, tick_sig_figs: u32) -> Dec {
    let mirrored = price.min(Dec::ONE.checked_sub(price).unwrap_or(Dec::ZERO));
    if !mirrored.is_positive() {
        return tick_size_min;
    }
    // floor(log10(p')): the position of the first significant digit.
    let first = i64::from(digits_before_point(mirrored));
    let exponent = first + 1 - i64::from(tick_sig_figs);
    let tick = match exponent {
        e if e >= 0 => Dec::ONE,
        e => Dec::new(1, u32::try_from(-e).unwrap_or(u32::MAX)).unwrap_or(tick_size_min),
    };
    tick.max(tick_size_min)
}

/// `floor(log10(value))` for a positive value.
fn digits_before_point(value: Dec) -> i32 {
    // `value` is units / 10^scale, so floor(log10(value)) is digits(units) - scale - 1.
    let places = i32::try_from(value.places()).unwrap_or(0);
    let units = value.units_at(value.places(), Round::Down).unwrap_or(0).unsigned_abs();
    let mut digits = 0i32;
    let mut left = units;
    while left > 0 {
        left /= 10;
        digits += 1;
    }
    digits - places - 1
}

/// Whether `price` is on its tick grid.
#[must_use]
pub fn is_on_tick(price: Dec, tick_size_min: Dec, tick_sig_figs: u32) -> bool {
    let tick = tick_size(price, tick_size_min, tick_sig_figs);
    if !tick.is_positive() || price.is_negative() || price > Dec::ONE {
        return false;
    }
    price.checked_div(tick, 0, Round::Down).and_then(|steps| steps.checked_mul(tick)).is_some_and(|back| back == price)
}

/// Rounds `price` to the tick grid in the direction `round`, clamped to 0 to 1. A price already
/// on the grid is returned unchanged.
#[must_use]
pub fn round_to_tick(price: Dec, tick_size_min: Dec, tick_sig_figs: u32, round: Round) -> Dec {
    let clamp = |value: Dec| value.max(Dec::ZERO).min(Dec::ONE);
    let price = clamp(price);
    let tick = tick_size(price, tick_size_min, tick_sig_figs);
    if !tick.is_positive() {
        return price;
    }
    let snapped = price.checked_div(tick, 0, round).and_then(|steps| steps.checked_mul(tick)).unwrap_or(price);
    let snapped = clamp(snapped);
    // Rounding can move the value into a band with a finer tick where it is off the grid, so round again.
    match is_on_tick(snapped, tick_size_min, tick_sig_figs) {
        true => snapped,
        false => {
            let tick = tick_size(snapped, tick_size_min, tick_sig_figs);
            clamp(snapped.checked_div(tick, 0, round).and_then(|steps| steps.checked_mul(tick)).unwrap_or(snapped))
        }
    }
}

/// The adjacent price above (or below) `price` on the grid. At a band boundary the step uses the
/// finer tick: up from 0.099 is 0.1, down from 0.1 is 0.099.
#[must_use]
pub fn step(price: Dec, tick_size_min: Dec, tick_sig_figs: u32, up: bool) -> Dec {
    let on = round_to_tick(price, tick_size_min, tick_sig_figs, if up { Round::Down } else { Round::Up });
    let beside = if up { on.checked_add(tick_size_min) } else { on.checked_sub(tick_size_min) }.unwrap_or(on);
    let tick = tick_size(on, tick_size_min, tick_sig_figs).min(tick_size(beside, tick_size_min, tick_sig_figs));
    let next = if up { on.checked_add(tick) } else { on.checked_sub(tick) }.unwrap_or(on);
    round_to_tick(next, tick_size_min, tick_sig_figs, if up { Round::Up } else { Round::Down })
}

/// `size * price * (1 - price)`: a fill's weighted contracts, which fee rates apply to.
#[must_use]
pub fn weighted(size: u64, price: Dec) -> Dec {
    let one_less = Dec::ONE.checked_sub(price).unwrap_or(Dec::ZERO);
    Dec::from_int(i64::try_from(size).unwrap_or(i64::MAX))
        .checked_mul(price)
        .and_then(|product| product.checked_mul(one_less))
        .unwrap_or(Dec::ZERO)
}

/// The taker's venue fee: `weighted * taker_fee_rate`, to six places.
#[must_use]
pub fn taker_fee(size: u64, price: Dec, taker_fee_rate: Dec) -> Dec {
    weighted(size, price).checked_mul(taker_fee_rate).unwrap_or(Dec::ZERO).round(6, Round::Nearest)
}

/// The maker's rebate: the taker fee's `maker_rebate_share`, to six places.
#[must_use]
pub fn maker_rebate(size: u64, price: Dec, taker_fee_rate: Dec, maker_rebate_share: Dec) -> Dec {
    weighted(size, price)
        .checked_mul(taker_fee_rate)
        .and_then(|fee| fee.checked_mul(maker_rebate_share))
        .unwrap_or(Dec::ZERO)
        .round(6, Round::Nearest)
}

/// What an order's owner pays its builder on a fill: `weighted * builder_fee_rate`, rounded toward zero.
#[must_use]
pub fn builder_fee(size: u64, price: Dec, builder_fee_rate: Dec) -> Dec {
    weighted(size, price).checked_mul(builder_fee_rate).unwrap_or(Dec::ZERO).round(6, Round::Down)
}

/// The collateral a new position locks: `size * price` long, `size * (1 - price)` short.
#[must_use]
pub fn collateral(side: &Side, size: u64, price: Dec) -> Dec {
    let per = match side {
        Side::Ask => Dec::ONE.checked_sub(price).unwrap_or(Dec::ZERO),
        _ => price,
    };
    Dec::from_int(i64::try_from(size).unwrap_or(i64::MAX)).checked_mul(per).unwrap_or(Dec::ZERO)
}

/// `1 - price`: converts between Yes and No prices.
#[must_use]
pub fn complement(price: Dec) -> Dec {
    Dec::ONE.checked_sub(price).unwrap_or(Dec::ZERO)
}

/// The web app's Yes/No outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    /// Long.
    Yes,
    /// Short.
    No,
}

/// Converts a web app Yes/No action to a side, price and reduce-only flag. Buy Yes at p is a bid
/// at p; buy No at p is an ask at 1 - p; selling is the opposite side with reduce-only set.
#[must_use]
pub fn yes_no(outcome: Outcome, buy: bool, price: Dec) -> (Side, Dec, bool) {
    match (outcome, buy) {
        (Outcome::Yes, true) => (Side::Bid, price, false),
        (Outcome::Yes, false) => (Side::Ask, price, true),
        (Outcome::No, true) => (Side::Ask, complement(price), false),
        (Outcome::No, false) => (Side::Bid, complement(price), true),
    }
}

#[cfg(test)]
#[path = "tests/price.rs"]
mod tests;
