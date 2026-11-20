//! Decimal arithmetic: exact sums and differences, products exact to 18 places, quotients to a
//! given number of places, and `None` on overflow.

use crate::utils::dec::round::{Round, divide};
use crate::utils::dec::{Dec, MAX_SCALE, ten};

impl Dec {
    #[must_use]
    pub fn checked_add(self, other: Dec) -> Option<Dec> {
        let (a, b, scale) = self.aligned(other)?;
        Some(Dec { units: a.checked_add(b)?, scale }.normal())
    }

    #[must_use]
    pub fn checked_sub(self, other: Dec) -> Option<Dec> {
        self.checked_add(other.negate())
    }

    /// Exact sum of all figures, in order; zero for none. `None` on overflow.
    #[must_use]
    pub fn checked_sum(figures: impl IntoIterator<Item = Dec>) -> Option<Dec> {
        figures.into_iter().try_fold(Dec::ZERO, Dec::checked_add)
    }

    /// Exact while the product fits 18 places; otherwise rounded to the nearest.
    #[must_use]
    pub fn checked_mul(self, other: Dec) -> Option<Dec> {
        let (a, b) = (self.normal(), other.normal());
        let units = a.units.checked_mul(b.units)?;
        let scale = a.scale + b.scale;
        if scale <= MAX_SCALE {
            return Some(Dec { units, scale }.normal());
        }
        let units = divide(units, ten(scale - MAX_SCALE)?, Round::Nearest);
        Some(Dec { units, scale: MAX_SCALE }.normal())
    }

    /// `self / other` rounded to `places` decimal places.
    #[must_use]
    pub fn checked_div(self, other: Dec, places: u32, round: Round) -> Option<Dec> {
        if other.units == 0 || places > MAX_SCALE {
            return None;
        }
        let (a, b) = (self.normal(), other.normal());
        // Divide as integers at a common scale and produce fractional digits one at a time from
        // the remainder. Scaling the numerator first would overflow for values in the hundreds of
        // millions at ten places.
        let scale = a.scale.max(b.scale);
        let top = a.units.checked_mul(ten(scale - a.scale)?)?;
        let bottom = b.units.checked_mul(ten(scale - b.scale)?)?;
        let negative = (top < 0) != (bottom < 0);
        let (top, bottom) = (top.unsigned_abs(), bottom.unsigned_abs());
        let mut units = top / bottom;
        let mut rest = top % bottom;
        for _ in 0..places {
            rest = rest.checked_mul(10)?;
            units = units.checked_mul(10)?.checked_add(rest / bottom)?;
            rest %= bottom;
        }
        let away = match round {
            Round::Down => false,
            Round::Up => rest != 0,
            Round::Nearest => rest != 0 && rest >= bottom - rest,
        };
        let units = i128::try_from(units.checked_add(u128::from(away))?).ok()?;
        let units = if negative { -units } else { units };
        Some(Dec { units, scale: places }.normal())
    }
}

#[cfg(test)]
#[path = "tests/arith.rs"]
mod tests;
