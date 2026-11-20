use super::*;
use crate::utils::solana::USDC_MINT;

// getAccountInfo data for a Pascal deposit address's USDC account and for the USDC mint, read from
// mainnet.
const TOKEN_ACCOUNT: &str = "xvp6877brTo9ZfNqq8l0MbG75MLS9uDkfKYCA0UvXWFw/rtiQB37SlskWZAGGLJ122jkm0Wv63ncDe+U/cqAhAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const USDC_MINT_ACCOUNT: &str =
    "AQAAAJj+huiNm+Lqi8HMpIeLKYjCQPUrhCS/tA7Rot3LXhmbFjeJUm2gHQAGAQEAAABicKqKWcWUBbRShshncubNEm6bil06OFNtN/e0FOi2Zw==";

#[test]
fn a_usdc_token_account_reads_its_mint_and_amount() {
    let data = base64::decode(TOKEN_ACCOUNT).unwrap();
    assert_eq!(data.len(), 165);
    let (mint, amount) = token_account(&data).unwrap();
    assert_eq!(mint, Pubkey::from_literal(USDC_MINT));
    assert_eq!(amount, 0);
}

#[test]
fn the_amount_is_little_endian_at_byte_64() {
    let mut data = base64::decode(TOKEN_ACCOUNT).unwrap();
    data[64..72].copy_from_slice(&12_345_678_u64.to_le_bytes());
    assert_eq!(token_account(&data).unwrap().1, 12_345_678);
    assert!(token_account(&data[..71]).is_none());
}

#[test]
fn the_usdc_mint_has_six_decimals() {
    let data = base64::decode(USDC_MINT_ACCOUNT).unwrap();
    assert_eq!(mint_decimals(&data), Some(6));
    assert_eq!(mint_decimals(&data[..81]), None);
}
