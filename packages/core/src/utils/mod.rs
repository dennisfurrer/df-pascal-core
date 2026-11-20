//! Shared helpers: exact decimals, base58 keys and signatures, order enums, permit encoding, tick
//! and fee math, and Solana address derivation.

pub mod base58;
pub mod base64;
pub mod dec;
pub mod keys;
pub(crate) mod lenient;
pub mod nonce;
pub mod order;
pub mod page;
pub mod permit;
pub mod price;
pub mod sign;
pub mod solana;
pub mod time;

pub use dec::{BadDec, Dec, Round};
pub use keys::{BadKey, Pubkey, Signature, parse_secret};
pub use lenient::Done;
pub use nonce::Timestamps;
pub use order::{
    Category, Interval, Liquidity, Lookback, MarketSort, MarketStatus, MarketUpdateType, MoversOrderBy, MoversWindow,
    OrderType, PositionOrderBy, PositionSide, RankBy, RankWindow, ResolvedOrderBy, SearchOrderBy, Side, SortOrder,
    TimeInForce, Timeframe, TransferStatus, TransferType, UnknownValue, UpdateReason,
};
pub use page::{Page, PageRequest};
pub use permit::{Auth, BadPermit, Builder, CancelBy};
pub use price::Outcome;
#[cfg(feature = "sign")]
pub use sign::{Keypair, verify};
pub use sign::{SignError, Signer};
