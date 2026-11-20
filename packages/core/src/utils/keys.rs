//! Solana public keys and Ed25519 signatures in base58, and private key parsing from a Solana CLI
//! keypair JSON array, base58 or hex.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use zeroize::Zeroizing;

use crate::utils::base58;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadKey {
    #[error("`{text}` is not a base58 public key: {reason}")]
    Pubkey { text: String, reason: String },
    #[error("`{text}` is not a base58 signature: {reason}")]
    Signature { text: String, reason: String },
    #[error("the private key is empty")]
    EmptySecret,
    #[error("the private key could not be read: {0}")]
    Secret(String),
    #[error("expected a 32-byte Ed25519 seed or a 64-byte Solana secret key, got {0} bytes")]
    SecretLength(usize),
    #[error("the 64-byte secret key's public half does not match its seed")]
    SecretMismatch,
}

/// A 32-byte Solana public key or address.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Pubkey([u8; 32]);

impl Pubkey {
    /// The all-zero key (the system program's address). Never a valid builder.
    pub const ZERO: Pubkey = Pubkey([0; 32]);

    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Pubkey {
        Pubkey(bytes)
    }

    #[must_use]
    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses base58 (case-sensitive).
    pub fn parse(text: &str) -> Result<Pubkey, BadKey> {
        base58::decode_array::<32>(text.trim())
            .map(Pubkey)
            .map_err(|error| BadKey::Pubkey { text: text.to_owned(), reason: error.to_string() })
    }

    /// Parses a base58 literal from source code; an invalid one returns [`Pubkey::ZERO`].
    #[must_use]
    pub fn from_literal(text: &str) -> Pubkey {
        Pubkey::parse(text).unwrap_or(Pubkey::ZERO)
    }

    #[must_use]
    pub fn to_base58(&self) -> String {
        base58::encode(&self.0)
    }

    /// Whether the bytes are a point on the Ed25519 curve. Program derived addresses are not.
    #[must_use]
    pub fn is_on_curve(&self) -> bool {
        curve25519_dalek::edwards::CompressedEdwardsY(self.0).decompress().is_some()
    }

    /// The first and last four characters, for display: `GmaD..AEGB`.
    #[must_use]
    pub fn short(&self) -> String {
        let text = self.to_base58();
        let chars: Vec<char> = text.chars().collect();
        if chars.len() <= 10 {
            return text;
        }
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}..{tail}")
    }
}

impl fmt::Display for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_base58())
    }
}

impl fmt::Debug for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Pubkey({})", self.to_base58())
    }
}

impl FromStr for Pubkey {
    type Err = BadKey;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Pubkey::parse(text)
    }
}

impl From<[u8; 32]> for Pubkey {
    fn from(bytes: [u8; 32]) -> Self {
        Pubkey(bytes)
    }
}

impl Serialize for Pubkey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Pubkey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Pubkey::parse(&text).map_err(de::Error::custom)
    }
}

/// A 64-byte Ed25519 signature, written in base58.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Signature([u8; 64]);

impl Signature {
    #[must_use]
    pub const fn new(bytes: [u8; 64]) -> Signature {
        Signature(bytes)
    }

    #[must_use]
    pub const fn to_bytes(self) -> [u8; 64] {
        self.0
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// From 64 raw bytes, as a wallet returns them.
    pub fn from_slice(bytes: &[u8]) -> Result<Signature, BadKey> {
        <[u8; 64]>::try_from(bytes).map(Signature).map_err(|_| BadKey::Signature {
            text: format!("{} bytes", bytes.len()),
            reason: "a signature is 64 bytes".to_owned(),
        })
    }

    pub fn parse(text: &str) -> Result<Signature, BadKey> {
        base58::decode_array::<64>(text.trim())
            .map(Signature)
            .map_err(|error| BadKey::Signature { text: text.to_owned(), reason: error.to_string() })
    }

    #[must_use]
    pub fn to_base58(&self) -> String {
        base58::encode(&self.0)
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_base58())
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signature({})", self.to_base58())
    }
}

impl FromStr for Signature {
    type Err = BadKey;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Signature::parse(text)
    }
}

impl Serialize for Signature {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        Signature::parse(&text).map_err(de::Error::custom)
    }
}

/// Parses a private key into its 32-byte seed, plus the public key when the input is a 64-byte
/// Solana secret key. Accepts:
///
/// - a JSON byte array of 32 or 64 values, as `solana-keygen` writes a keypair file;
/// - 64 hex characters (a 32-byte seed);
/// - base58 of 32 or 64 bytes, as wallets export a secret key.
pub fn parse_secret(text: &str) -> Result<(Zeroizing<[u8; 32]>, Option<Pubkey>), BadKey> {
    let text = text.trim();
    if text.is_empty() {
        return Err(BadKey::EmptySecret);
    }
    let raw: Zeroizing<Vec<u8>> = if text.starts_with('[') {
        let values: Zeroizing<Vec<u16>> = Zeroizing::new(
            serde_json::from_str(text)
                .map_err(|_| BadKey::Secret("a JSON private key is a list of bytes".to_owned()))?,
        );
        let mut bytes = Zeroizing::new(Vec::with_capacity(values.len()));
        for value in values.iter() {
            bytes.push(u8::try_from(*value).map_err(|_| BadKey::Secret("a byte is 0 to 255".to_owned()))?);
        }
        bytes
    } else if text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Zeroizing::new(hex(text).ok_or_else(|| BadKey::Secret("bad hex".to_owned()))?)
    } else {
        Zeroizing::new(base58::decode(text).map_err(|error| BadKey::Secret(error.to_string()))?)
    };
    match raw.len() {
        32 => {
            let mut seed = Zeroizing::new([0u8; 32]);
            seed.copy_from_slice(&raw);
            Ok((seed, None))
        }
        64 => {
            let mut seed = Zeroizing::new([0u8; 32]);
            seed.copy_from_slice(&raw[..32]);
            let mut public = [0u8; 32];
            public.copy_from_slice(&raw[32..]);
            Ok((seed, Some(Pubkey(public))))
        }
        length => Err(BadKey::SecretLength(length)),
    }
}

fn hex(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    bytes
        .chunks(2)
        .map(|pair| {
            let high = char::from(*pair.first()?).to_digit(16)?;
            let low = char::from(*pair.get(1)?).to_digit(16)?;
            u8::try_from(high * 16 + low).ok()
        })
        .collect()
}

#[cfg(test)]
#[path = "tests/keys.rs"]
mod tests;
