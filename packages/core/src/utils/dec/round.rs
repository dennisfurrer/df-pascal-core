//! Rounding to decimal places, to significant figures, or to an integer count of a smaller unit.

use std::cmp::Ordering;

use crate::utils::dec::{Dec, ten};

/// Rounding direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Round {
    /// Toward zero. Used for sizes so they never exceed the requested amount.
    Down,
    /// Away from zero.
    Up,
    /// To the nearer, and away from zero on a tie.
    Nearest,
}

impl Dec {
    /// The value as an integer count of `10^-places`, rounded as given. Used for on-chain
    /// integer amounts (USDC in millionths).
    #[must_use]
    pub fn units_at(self, places: u32, round: Round) -> Option<i128> {
        match places.cmp(&self.scale) {
            Ordering::Equal => Some(self.units),
            Ordering::Greater => self.units.checked_mul(ten(places - self.scale)?),
            Ordering::Less => Some(divide(self.units, ten(self.scale - places)?, round)),
        }
    }

    /// Rounded to at most `places` decimal places.
    #[must_use]
    pub fn round(self, places: u32, round: Round) -> Dec {
        if places >= self.scale {
            return self.normal();
        }
        self.units_at(places, round).map_or(self, |units| Dec { units, scale: places }.normal())
    }

    /// The decimal places that `figures` significant digits reach for this value: 2 for
    /// `123.456` at five figures, 6 for `0.01234567`; 0 when they end in the integer part
    /// (`123456.7`).
    #[must_use]
    pub fn sig_places(self, figures: u32) -> u32 {
        let places = i64::from(figures) - i64::from(digits(self.units)) + i64::from(self.scale);
        u32::try_from(places.max(0)).unwrap_or(0)
    }

    /// Rounded to at most `figures` significant digits. Dropped integer digits become zeros, so
    /// `123456.7` to five figures is `123450`.
    #[must_use]
    pub fn round_sig(self, figures: u32, round: Round) -> Dec {
        let digits = digits(self.units);
        if figures == 0 || digits <= figures {
            return self.normal();
        }
        let dropped = digits - figures;
        let Some(step) = ten(dropped) else {
            return self;
        };
        let kept = divide(self.units, step, round);
        match kept.checked_mul(step) {
            Some(units) => Dec { units, scale: self.scale }.normal(),
            None => self,
        }
    }
}

fn digits(units: i128) -> u32 {
    let mut left = units.unsigned_abs();
    let mut count = 0;
    while left > 0 {
        left /= 10;
        count += 1;
    }
    count
}

/// Integer division with the rounding asked for; `by` is positive.
pub(super) fn divide(value: i128, by: i128, round: Round) -> i128 {
    let quotient = value / by;
    let rest = value % by;
    if rest == 0 {
        return quotient;
    }
    let away = if value < 0 { -1 } else { 1 };
    match round {
        Round::Down => quotient,
        Round::Up => quotient + away,
        Round::Nearest => match (rest.unsigned_abs() * 2).cmp(&by.unsigned_abs()) {
            Ordering::Less => quotient,
            _ => quotient + away,
        },
    }
}

#[cfg(test)]
#[path = "tests/round.rs"]
mod tests;
