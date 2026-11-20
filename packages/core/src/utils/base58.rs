//! Base58 with the Bitcoin alphabet, used by Solana for public keys, addresses and signatures.
//! Base58 is case-sensitive.

const ALPHABET: &[u8; 58] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadBase58 {
    #[error("`{0}` is not a base58 character")]
    Character(char),
    #[error("the text is empty")]
    Empty,
}

/// `bytes` in base58. Each leading zero byte is one leading `1`.
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let zeros = bytes.iter().take_while(|byte| **byte == 0).count();
    // Base-58 digits, least significant first.
    let mut digits: Vec<u8> = Vec::with_capacity(bytes.len() * 138 / 100 + 1);
    for &byte in &bytes[zeros..] {
        let mut carry = u32::from(byte);
        for digit in &mut digits {
            carry += u32::from(*digit) << 8;
            *digit = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut text = String::with_capacity(zeros + digits.len());
    text.extend(std::iter::repeat_n('1', zeros));
    text.extend(digits.iter().rev().map(|digit| char::from(ALPHABET[usize::from(*digit)])));
    text
}

/// The bytes a base58 string spells. Each leading `1` is one leading zero byte.
pub fn decode(text: &str) -> Result<Vec<u8>, BadBase58> {
    if text.is_empty() {
        return Err(BadBase58::Empty);
    }
    let ones = text.bytes().take_while(|byte| *byte == b'1').count();
    // Bytes, least significant first.
    let mut bytes: Vec<u8> = Vec::with_capacity(text.len() * 733 / 1000 + 1);
    for character in text.chars().skip(ones) {
        let value = digit(character).ok_or(BadBase58::Character(character))?;
        let mut carry = u32::from(value);
        for byte in &mut bytes {
            carry += u32::from(*byte) * 58;
            *byte = (carry & 0xff) as u8;
            carry >>= 8;
        }
        while carry > 0 {
            bytes.push((carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let mut out = vec![0u8; ones];
    out.extend(bytes.iter().rev());
    Ok(out)
}

/// Decodes into exactly `N` bytes; otherwise the error reports the decoded length.
pub fn decode_array<const N: usize>(text: &str) -> Result<[u8; N], BadLength> {
    let bytes = decode(text).map_err(BadLength::Base58)?;
    let length = bytes.len();
    bytes.try_into().map_err(|_| BadLength::Length { expected: N, found: length })
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadLength {
    #[error(transparent)]
    Base58(BadBase58),
    #[error("expected {expected} bytes, found {found}")]
    Length { expected: usize, found: usize },
}

fn digit(character: char) -> Option<u8> {
    let byte = u8::try_from(character).ok()?;
    ALPHABET.iter().position(|candidate| *candidate == byte).and_then(|index| u8::try_from(index).ok())
}

#[cfg(test)]
#[path = "tests/base58.rs"]
mod tests;
