//! Exact decimal arithmetic for venue amounts, parsed from and written as strings without floats.

mod arith;
mod round;
mod wire;

pub use round::Round;
pub use wire::BadDec;

use std::cmp::Ordering;

/// The largest scale this type keeps. Pascal writes six places; the extra places are for
/// products (a fee is a product of four figures).
pub const MAX_SCALE: u32 = 18;

/// An exact decimal, `units / 10^scale`. The venue sends amounts as strings, and converting
/// through a float would change the signed value.
#[derive(Debug, Clone, Copy, Default)]
pub struct Dec {
    units: i128,
    scale: u32,
}

impl Dec {
    pub const ZERO: Dec = Dec { units: 0, scale: 0 };
    pub const ONE: Dec = Dec { units: 1, scale: 0 };

    /// `units / 10^scale`. A scale past [`MAX_SCALE`] is refused.
    #[must_use]
    pub const fn new(units: i128, scale: u32) -> Option<Dec> {
        if scale > MAX_SCALE {
            return None;
        }
        Some(Dec { units, scale })
    }

    /// `units / 10^SCALE` for a literal in code: `Dec::lit::<2>(475)` is 4.75. A scale above
    /// [`MAX_SCALE`] fails at compile time.
    ///
    /// ```compile_fail,E0080
    /// const TOO_FINE: df_pascal_core::utils::Dec = df_pascal_core::utils::Dec::lit::<19>(1);
    /// ```
    #[must_use]
    pub const fn lit<const SCALE: u32>(units: i128) -> Dec {
        const { assert!(SCALE <= MAX_SCALE, "a Dec keeps at most eighteen places") };
        Dec { units, scale: SCALE }
    }

    #[must_use]
    pub const fn from_int(whole: i64) -> Dec {
        Dec { units: whole as i128, scale: 0 }
    }

    /// The same value with no trailing zeros behind the point.
    #[must_use]
    fn normal(mut self) -> Dec {
        while self.scale > 0 && self.units % 10 == 0 {
            self.units /= 10;
            self.scale -= 1;
        }
        if self.units == 0 {
            self.scale = 0;
        }
        self
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.units == 0
    }

    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.units < 0
    }

    #[must_use]
    pub const fn is_positive(self) -> bool {
        self.units > 0
    }

    #[must_use]
    pub fn is_integer(self) -> bool {
        self.normal().scale == 0
    }

    /// The number of decimal places the value needs.
    #[must_use]
    pub fn places(self) -> u32 {
        self.normal().scale
    }

    #[must_use]
    pub const fn abs(self) -> Dec {
        Dec { units: self.units.abs(), scale: self.scale }
    }

    #[must_use]
    pub const fn negate(self) -> Dec {
        Dec { units: -self.units, scale: self.scale }
    }

    fn aligned(self, other: Dec) -> Option<(i128, i128, u32)> {
        let scale = self.scale.max(other.scale);
        Some((
            self.units.checked_mul(ten(scale - self.scale)?)?,
            other.units.checked_mul(ten(scale - other.scale)?)?,
            scale,
        ))
    }
}

/// `10^power`, or `None` past what an `i128` holds.
fn ten(power: u32) -> Option<i128> {
    10i128.checked_pow(power)
}

impl PartialEq for Dec {
    fn eq(&self, other: &Dec) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Dec {}

impl PartialOrd for Dec {
    fn partial_cmp(&self, other: &Dec) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Dec {
    fn cmp(&self, other: &Dec) -> Ordering {
        match self.aligned(*other) {
            Some((a, b, _)) => a.cmp(&b),
            // Only reached near the i128 limit; compare the integer parts.
            None => (self.units / ten(self.scale).unwrap_or(1)).cmp(&(other.units / ten(other.scale).unwrap_or(1))),
        }
    }
}

#[cfg(test)]
#[path = "tests/dec.rs"]
mod tests;
