//! Rust SDK for Pascal, a prediction market exchange on Solana.
//!
//! - [`client`]: the [`Client`], HTTP read and write APIs, rate limiting, two-step signed
//!   writes, deployments, and native and browser transports.
//! - [`markets`]: markets, order books, candles, trades and holders.
//! - [`account`]: account holdings, history, trading keys, profile and referrals.
//! - [`trade`]: orders, replaces, cancels, mass cancels and collateral returns.
//! - [`transfer`]: the deposit address, USDC deposits on Solana and withdrawals.
//! - [`live`]: the market data and order entry websockets, and local state built from them.
//! - [`utils`]: decimals, keys, permits, ticks and fees, enums and Solana addresses.
//!
//! Prices and amounts are exact decimals, not floats. Features: `sign` and `native` (default),
//! `web`, `offline`.

pub mod account;
pub mod client;
pub mod live;
pub mod markets;
pub mod trade;
pub mod transfer;
pub mod utils;

pub use client::{Client, Deployment, Trader};
#[cfg(feature = "sign")]
pub use utils::Keypair;
pub use utils::{Dec, Pubkey, Signature};
