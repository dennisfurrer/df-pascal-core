//! Solana address derivation: program derived addresses, USDC associated token accounts and
//! program ids.

pub mod rpc;
pub mod tx;

use sha2::{Digest, Sha256};

use crate::utils::keys::Pubkey;

/// The SPL Token program, which owns the USDC mint and USDC token accounts. USDC is not a
/// Token-2022 mint.
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
/// The Token-2022 program, for a mint that is not USDC.
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
/// The Associated Token Account program.
pub const ASSOCIATED_TOKEN_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
pub const SYSTEM_PROGRAM: &str = "11111111111111111111111111111111";
pub const COMPUTE_BUDGET_PROGRAM: &str = "ComputeBudget111111111111111111111111111111";
/// USDC mint on Solana mainnet (6 decimals).
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
pub const USDC_DECIMALS: u8 = 6;
/// Pascal's program on mainnet.
pub const PASCAL_PROGRAM: &str = "3vCgc3aL1D6acmtiqtUgRSRDhep4PnVgs4jk86dGpg7j";

/// The most seeds and the longest seed a program derived address takes.
pub const MAX_SEEDS: usize = 16;
pub const MAX_SEED_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadSeeds {
    #[error("at most {MAX_SEEDS} seeds")]
    TooMany,
    #[error("a seed is at most {MAX_SEED_LEN} bytes")]
    TooLong,
    #[error("no bump from 255 to 0 gives an address off the curve")]
    NoBump,
}

/// The address `seeds` and `bump` derive under `program`, or `None` when the result is on the
/// Ed25519 curve (the runtime rejects those).
pub fn create_program_address(seeds: &[&[u8]], bump: u8, program: &Pubkey) -> Result<Option<Pubkey>, BadSeeds> {
    if seeds.len() + 1 > MAX_SEEDS {
        return Err(BadSeeds::TooMany);
    }
    let mut hasher = Sha256::new();
    for seed in seeds {
        if seed.len() > MAX_SEED_LEN {
            return Err(BadSeeds::TooLong);
        }
        hasher.update(seed);
    }
    hasher.update([bump]);
    hasher.update(program.as_bytes());
    hasher.update(b"ProgramDerivedAddress");
    let address = Pubkey::new(hasher.finalize().into());
    Ok((!address.is_on_curve()).then_some(address))
}

/// The program derived address of `seeds` under `program`, and its bump: the first bump down
/// from 255 whose address is off the curve.
pub fn find_program_address(seeds: &[&[u8]], program: &Pubkey) -> Result<(Pubkey, u8), BadSeeds> {
    for bump in (0..=u8::MAX).rev() {
        if let Some(address) = create_program_address(seeds, bump, program)? {
            return Ok((address, bump));
        }
    }
    Err(BadSeeds::NoBump)
}

/// The associated token account of `wallet` for `mint` under `token_program`: seeds
/// `[wallet, token_program, mint]` under the Associated Token Account program.
#[must_use]
pub fn associated_token_address_with_program(wallet: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    let program = Pubkey::from_literal(ASSOCIATED_TOKEN_PROGRAM);
    // With three 32-byte seeds a bump is always found in practice (failure odds are 2^-256).
    find_program_address(&[wallet.as_bytes(), token_program.as_bytes(), mint.as_bytes()], &program)
        .map_or(Pubkey::ZERO, |(address, _)| address)
}

/// The associated token account of `wallet` for `mint`, under the SPL Token program (USDC's).
#[must_use]
pub fn associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    associated_token_address_with_program(wallet, mint, &Pubkey::from_literal(TOKEN_PROGRAM))
}

/// `wallet`'s USDC account on mainnet: where a withdrawal to `wallet` is paid.
#[must_use]
pub fn usdc_account(wallet: &Pubkey) -> Pubkey {
    associated_token_address(wallet, &Pubkey::from_literal(USDC_MINT))
}

#[cfg(test)]
#[path = "tests/solana.rs"]
mod tests;
