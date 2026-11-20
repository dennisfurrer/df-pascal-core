//! Deserializers for loosely typed response fields: numbers sent as strings or JSON numbers, and
//! lists or strings sent as `null`.

use serde::{Deserialize, Deserializer};

/// A string; absent or `null` reads as empty.
pub(crate) fn text<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// A list; absent or `null` reads as empty.
pub(crate) fn list<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Number {
    Unsigned(u64),
    Signed(i64),
    Text(String),
}

fn whole<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i128>, D::Error> {
    match Option::<Number>::deserialize(deserializer)? {
        None => Ok(None),
        Some(Number::Unsigned(value)) => Ok(Some(i128::from(value))),
        Some(Number::Signed(value)) => Ok(Some(i128::from(value))),
        Some(Number::Text(text)) if text.trim().is_empty() => Ok(None),
        Some(Number::Text(text)) => text.trim().parse::<i128>().map(Some).map_err(serde::de::Error::custom),
    }
}

/// A `u64` written as a string (Pascal's way) or a number; absent or `null` reads as 0.
pub(crate) fn u64s<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    whole(deserializer)?.map_or(Ok(0), |value| u64::try_from(value).map_err(serde::de::Error::custom))
}

/// An `i64` written as a string or a number; absent or `null` reads as 0.
pub(crate) fn i64s<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    whole(deserializer)?.map_or(Ok(0), |value| i64::try_from(value).map_err(serde::de::Error::custom))
}

/// A `u128` written as a string or a number; absent or `null` reads as 0.
pub(crate) fn u128s<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
    whole(deserializer)?.map_or(Ok(0), |value| u128::try_from(value).map_err(serde::de::Error::custom))
}

/// An optional `u64` written as a string or a number.
pub(crate) fn opt_u64s<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u64>, D::Error> {
    whole(deserializer)?.map(|value| u64::try_from(value).map_err(serde::de::Error::custom)).transpose()
}

/// An optional `i64` written as a string or a number.
pub(crate) fn opt_i64s<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    whole(deserializer)?.map(|value| i64::try_from(value).map_err(serde::de::Error::custom)).transpose()
}

/// Serializes a `u64` as a string.
pub(crate) fn u64_text<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

pub(crate) fn i64_text<S: serde::Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

pub(crate) fn u128_text<S: serde::Serializer>(value: &u128, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

pub(crate) fn opt_u64_text<S: serde::Serializer>(value: &Option<u64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.collect_str(value),
        None => serializer.serialize_none(),
    }
}

pub(crate) fn opt_i64_text<S: serde::Serializer>(value: &Option<i64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.collect_str(value),
        None => serializer.serialize_none(),
    }
}

/// A response whose fields are ignored, such as `{}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, serde::Serialize)]
pub struct Done {}
