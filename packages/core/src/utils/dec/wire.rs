//! Decimal string parsing and formatting (optional sign, at most one point, no exponent, no
//! trailing zeros), and serde support.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::utils::dec::{Dec, MAX_SCALE};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadDec {
    #[error("`{0}` is not a decimal number")]
    NotANumber(String),
    #[error("`{0}` has more than eighteen decimal places")]
    TooFine(String),
    #[error("`{0}` is larger than this build can count")]
    OutOfRange(String),
}

impl Dec {
    /// Parses an optional sign, digits and at most one decimal point. Exponents are rejected; the
    /// venue never sends one.
    pub fn parse(text: &str) -> Result<Dec, BadDec> {
        let bad = || BadDec::NotANumber(text.to_owned());
        let body = text.trim();
        let (negative, body) = match body.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, body.strip_prefix('+').unwrap_or(body)),
        };
        let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
        if whole.is_empty() && fraction.is_empty() {
            return Err(bad());
        }
        if !whole.bytes().all(|b| b.is_ascii_digit()) || !fraction.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        let fraction = fraction.trim_end_matches('0');
        let scale = u32::try_from(fraction.len()).map_err(|_| BadDec::TooFine(text.to_owned()))?;
        if scale > MAX_SCALE {
            return Err(BadDec::TooFine(text.to_owned()));
        }
        let mut units: i128 = 0;
        for digit in whole.bytes().chain(fraction.bytes()) {
            units = units
                .checked_mul(10)
                .and_then(|u| u.checked_add(i128::from(digit - b'0')))
                .ok_or_else(|| BadDec::OutOfRange(text.to_owned()))?;
        }
        Ok(Dec { units: if negative { -units } else { units }, scale }.normal())
    }

    /// The canonical spelling: no trailing zeros, no exponent, `0` for zero.
    #[must_use]
    pub fn to_wire(self) -> String {
        self.to_string()
    }

    /// Written with exactly `places` decimal places, rounded toward zero: `0.55` at six places is
    /// `0.550000`, the format Pascal uses for prices and USD amounts.
    #[must_use]
    pub fn to_fixed(self, places: u32) -> String {
        let kept = self.round(places, crate::utils::dec::Round::Down);
        let Some(units) = kept.units_at(places, crate::utils::dec::Round::Down) else {
            return kept.to_string();
        };
        let sign = if units < 0 { "-" } else { "" };
        let magnitude = units.unsigned_abs();
        if places == 0 {
            return format!("{sign}{magnitude}");
        }
        let step = 10u128.pow(places);
        let width = places as usize;
        format!("{sign}{}.{:0width$}", magnitude / step, magnitude % step)
    }
}

impl fmt::Display for Dec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let normal = self.normal();
        let sign = if normal.units < 0 { "-" } else { "" };
        let magnitude = normal.units.unsigned_abs();
        if normal.scale == 0 {
            return write!(f, "{sign}{magnitude}");
        }
        let step = 10u128.pow(normal.scale);
        let places = normal.scale as usize;
        write!(f, "{sign}{}.{:0places$}", magnitude / step, magnitude % step)
    }
}

impl FromStr for Dec {
    type Err = BadDec;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Dec::parse(text)
    }
}

impl Serialize for Dec {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Deserializes from a decimal string or a JSON integer. A JSON fraction is rejected because it
/// has already been parsed as a float.
impl<'de> Deserialize<'de> for Dec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = Dec;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal string or an integer")
            }

            fn visit_str<E: de::Error>(self, text: &str) -> Result<Dec, E> {
                Dec::parse(text).map_err(E::custom)
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Dec, E> {
                Ok(Dec { units: i128::from(value), scale: 0 })
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Dec, E> {
                Ok(Dec { units: i128::from(value), scale: 0 })
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(test)]
#[path = "tests/wire.rs"]
mod tests;
