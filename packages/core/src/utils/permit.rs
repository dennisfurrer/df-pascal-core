//! Signed bytes (permits) for each request. Orders, cancels, mass cancels and collateral returns
//! sign a little-endian binary permit with an 80-byte header (version, command, deployment,
//! receive window, client timestamp, owner, signer); other requests sign a compact UTF-8 JSON
//! array in a fixed field order with non-ASCII characters unescaped.

use sha2::{Digest, Sha256};

use crate::utils::dec::{Dec, Round};
use crate::utils::keys::Pubkey;
use crate::utils::order::{OrderType, Side, TimeInForce};

pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 80;
pub const PLACE_ORDER_LEN: usize = 232;
pub const CANCEL_ORDER_LEN: usize = 96;
pub const MASS_CANCEL_LEN: usize = 120;
pub const COLLATERAL_RETURN_LEN: usize = 112;

/// The longest receive window Pascal accepts.
pub const MAX_RECV_WINDOW_MS: u32 = 60_000;
/// The receive window the quickstart uses, and this crate's default.
pub const DEFAULT_RECV_WINDOW_MS: u32 = 5_000;
/// `replace_client_order_id` value meaning none (all bits set); client order ids must be below it.
pub const NO_REPLACE: u64 = u64::MAX;
/// The highest client order id.
pub const MAX_CLIENT_ORDER_ID: u64 = u64::MAX - 1;
/// Symbols fit 32 ASCII bytes; event codes 30.
pub const SYMBOL_LEN: usize = 32;
pub const EVENT_CODE_LEN: usize = 30;
pub const TRADING_KEY_NAME_LEN: usize = 16;
/// The builder fee cap: 10%.
pub const MAX_BUILDER_FEE_RATE_PPM: u32 = 100_000;
/// Trading keys expire at most this far ahead.
pub const MAX_TRADING_KEY_LIFETIME_MS: u64 = 31_536_000_000;
/// The hash of `pascal.place_order.offchain_fields.v1`, which every place permit carries.
pub const OFFCHAIN_FIELDS_DIGEST: [u8; 32] = [
    0xdc, 0xaf, 0x53, 0x73, 0x8f, 0x1d, 0x5f, 0x06, 0x16, 0x46, 0x60, 0x49, 0x8a, 0x75, 0x88, 0x8e, 0x7f, 0x0c, 0x78,
    0xed, 0x5f, 0x1b, 0xc1, 0x7b, 0xe1, 0xdd, 0x1c, 0xb3, 0xcc, 0xf5, 0x3c, 0xe3,
];
pub const OFFCHAIN_FIELDS_TAG: &str = "pascal.place_order.offchain_fields.v1";

/// Command types in a binary permit's header.
pub mod command {
    pub const PLACE_ORDER: u8 = 1;
    pub const CANCEL_ORDER: u8 = 2;
    pub const MASS_CANCEL: u8 = 10;
    pub const COLLATERAL_RETURN: u8 = 11;
}

/// Flag bits in a place permit.
pub mod flags {
    pub const POST_ONLY: u8 = 1;
    pub const REDUCE_ONLY: u8 = 2;
    pub const ALLOW_MISSING_REPLACE: u8 = 4;
    pub const REPLACE_WITH_OPEN_SIZE: u8 = 8;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadPermit {
    #[error("recv_window_ms is at most {MAX_RECV_WINDOW_MS}")]
    RecvWindow,
    #[error("`{0}` is not up to {SYMBOL_LEN} ASCII bytes")]
    Symbol(String),
    #[error("`{0}` is not up to {EVENT_CODE_LEN} ASCII bytes")]
    EventCode(String),
    #[error("a price is 0 to 1 with at most six places; `{0}` is not")]
    Price(String),
    #[error("a size is at least one contract")]
    Size,
    #[error("a client order id is at most {MAX_CLIENT_ORDER_ID}")]
    ClientOrderId,
    #[error("{0} is not a side, time in force or order type this build can sign")]
    Unknown(String),
    #[error("a GTT order needs expires_ts_ms, and only a GTT order takes one")]
    Expiry,
    #[error("a market order is IOC")]
    MarketTif,
    #[error("allow_missing_replace and replace_with_open_size each need replace_client_order_id, and not each other")]
    Replace,
    #[error("a builder fee rate is 0 to 0.1 with at most six places; `{0}` is not")]
    BuilderFee(String),
    #[error("the builder is not the all-zero key, and not the order's own owner")]
    Builder,
    #[error("a wallet-signed request is signed by its owner")]
    NotOwner,
    #[error("a trading key name is up to {TRADING_KEY_NAME_LEN} UTF-8 bytes")]
    KeyName,
    #[error("an amount is positive with at most six places; `{0}` is not")]
    Amount(String),
    #[error("{0}")]
    Field(String),
}

/// Deployment, owner, signer, timestamp and receive window: the fields in every permit header,
/// also sent as the request's `auth`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Auth {
    pub deployment_id: u8,
    pub owner: Pubkey,
    pub signer: Pubkey,
    pub client_ts_ms: u64,
    pub recv_window_ms: u32,
}

impl Auth {
    fn check(&self) -> Result<(), BadPermit> {
        match self.recv_window_ms > MAX_RECV_WINDOW_MS {
            true => Err(BadPermit::RecvWindow),
            false => Ok(()),
        }
    }

    fn wallet(&self) -> Result<(), BadPermit> {
        self.check()?;
        match self.owner == self.signer {
            true => Ok(()),
            false => Err(BadPermit::NotOwner),
        }
    }

    /// The 80-byte header of a binary permit.
    pub fn header(&self, command: u8) -> Result<[u8; HEADER_LEN], BadPermit> {
        self.check()?;
        let mut header = [0u8; HEADER_LEN];
        header[0] = VERSION;
        header[1] = command;
        header[2] = self.deployment_id;
        header[4..8].copy_from_slice(&self.recv_window_ms.to_le_bytes());
        header[8..16].copy_from_slice(&self.client_ts_ms.to_le_bytes());
        header[16..48].copy_from_slice(self.owner.as_bytes());
        header[48..80].copy_from_slice(self.signer.as_bytes());
        Ok(header)
    }

    /// The fields a JSON permit ends with: version, deployment, window, timestamp, owner, signer.
    fn tail(&self) -> [serde_json::Value; 6] {
        [
            VERSION.into(),
            self.deployment_id.into(),
            self.recv_window_ms.into(),
            self.client_ts_ms.into(),
            self.owner.to_base58().into(),
            self.signer.to_base58().into(),
        ]
    }
}

/// A symbol as 32 zero-padded bytes.
pub fn symbol_bytes(symbol: &str) -> Result<[u8; SYMBOL_LEN], BadPermit> {
    fixed::<SYMBOL_LEN>(symbol).ok_or_else(|| BadPermit::Symbol(symbol.to_owned()))
}

/// An event code as 30 zero-padded bytes.
pub fn event_code_bytes(event_code: &str) -> Result<[u8; EVENT_CODE_LEN], BadPermit> {
    fixed::<EVENT_CODE_LEN>(event_code).ok_or_else(|| BadPermit::EventCode(event_code.to_owned()))
}

fn fixed<const N: usize>(text: &str) -> Option<[u8; N]> {
    if !text.is_ascii() || text.len() > N || text.is_empty() {
        return None;
    }
    let mut bytes = [0u8; N];
    bytes[..text.len()].copy_from_slice(text.as_bytes());
    Some(bytes)
}

/// A price in millionths of a dollar: 0 to 1,000,000.
pub fn price_micros(price: Dec) -> Result<u64, BadPermit> {
    let bad = || BadPermit::Price(price.to_string());
    if price.places() > 6 || price.is_negative() || price > Dec::ONE {
        return Err(bad());
    }
    price.units_at(6, Round::Down).and_then(|units| u64::try_from(units).ok()).ok_or_else(bad)
}

/// A builder fee rate in parts per million: 0 to 100,000.
pub fn builder_fee_ppm(rate: Dec) -> Result<u32, BadPermit> {
    let bad = || BadPermit::BuilderFee(rate.to_string());
    if rate.places() > 6 || rate.is_negative() {
        return Err(bad());
    }
    let ppm = rate.units_at(6, Round::Down).and_then(|units| u32::try_from(units).ok()).ok_or_else(bad)?;
    match ppm > MAX_BUILDER_FEE_RATE_PPM {
        true => Err(bad()),
        false => Ok(ppm),
    }
}

/// A USD amount in millionths, positive.
pub fn usd_micros(amount: Dec) -> Result<u64, BadPermit> {
    let bad = || BadPermit::Amount(amount.to_string());
    if amount.places() > 6 || !amount.is_positive() {
        return Err(bad());
    }
    amount.units_at(6, Round::Down).and_then(|units| u64::try_from(units).ok()).ok_or_else(bad)
}

/// A builder named on an order, and its fee rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Builder {
    pub pubkey: Pubkey,
    pub fee_rate: Dec,
}

/// Fields of a place permit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceOrder {
    pub client_order_id: u64,
    pub replace_client_order_id: Option<u64>,
    pub symbol: String,
    pub side: Side,
    pub price: Dec,
    pub size: u64,
    pub tif: TimeInForce,
    pub order_type: OrderType,
    pub expires_ts_ms: Option<u64>,
    pub post_only: bool,
    pub reduce_only: bool,
    pub allow_missing_replace: bool,
    pub replace_with_open_size: bool,
    pub builder: Option<Builder>,
    /// A byte identifying the client that sent the order; 0 unless set.
    pub metadata: u8,
}

impl PlaceOrder {
    /// Field checks Pascal makes that need no account state.
    pub fn check(&self, owner: &Pubkey) -> Result<(), BadPermit> {
        symbol_bytes(&self.symbol)?;
        price_micros(self.price)?;
        if self.size == 0 {
            return Err(BadPermit::Size);
        }
        if self.client_order_id > MAX_CLIENT_ORDER_ID
            || self.replace_client_order_id.is_some_and(|id| id > MAX_CLIENT_ORDER_ID)
        {
            return Err(BadPermit::ClientOrderId);
        }
        for (byte, name) in [
            (self.side.byte(), self.side.as_str()),
            (self.tif.byte(), self.tif.as_str()),
            (self.order_type.byte(), self.order_type.as_str()),
        ] {
            if byte.is_none() {
                return Err(BadPermit::Unknown(name.to_owned()));
            }
        }
        if (self.tif == TimeInForce::Gtt) != self.expires_ts_ms.is_some() {
            return Err(BadPermit::Expiry);
        }
        if self.order_type == OrderType::Market && self.tif != TimeInForce::Ioc {
            return Err(BadPermit::MarketTif);
        }
        let replacing = self.replace_client_order_id.is_some();
        if ((self.allow_missing_replace || self.replace_with_open_size) && !replacing)
            || (self.allow_missing_replace && self.replace_with_open_size)
        {
            return Err(BadPermit::Replace);
        }
        if let Some(builder) = &self.builder {
            if builder.pubkey == Pubkey::ZERO || builder.pubkey == *owner {
                return Err(BadPermit::Builder);
            }
            builder_fee_ppm(builder.fee_rate)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn flags(&self) -> u8 {
        let mut bits = 0;
        if self.post_only {
            bits |= flags::POST_ONLY;
        }
        if self.reduce_only {
            bits |= flags::REDUCE_ONLY;
        }
        if self.allow_missing_replace {
            bits |= flags::ALLOW_MISSING_REPLACE;
        }
        if self.replace_with_open_size {
            bits |= flags::REPLACE_WITH_OPEN_SIZE;
        }
        bits
    }
}

/// The 232-byte place permit.
pub fn place_order(auth: &Auth, order: &PlaceOrder) -> Result<Vec<u8>, BadPermit> {
    order.check(&auth.owner)?;
    let mut out = Vec::with_capacity(PLACE_ORDER_LEN);
    out.extend_from_slice(&auth.header(command::PLACE_ORDER)?);
    out.extend_from_slice(&order.size.to_le_bytes());
    out.extend_from_slice(&price_micros(order.price)?.to_le_bytes());
    out.extend_from_slice(&order.client_order_id.to_le_bytes());
    out.extend_from_slice(&order.replace_client_order_id.unwrap_or(NO_REPLACE).to_le_bytes());
    out.extend_from_slice(&order.expires_ts_ms.unwrap_or(0).to_le_bytes());
    let (builder, fee_ppm) = match &order.builder {
        Some(builder) => (builder.pubkey, builder_fee_ppm(builder.fee_rate)?),
        None => (Pubkey::ZERO, 0),
    };
    out.extend_from_slice(builder.as_bytes());
    out.extend_from_slice(&symbol_bytes(&order.symbol)?);
    // Encoded as a signed 32-bit integer; the cap keeps it well below `i32::MAX`.
    out.extend_from_slice(&i32::try_from(fee_ppm).unwrap_or(i32::MAX).to_le_bytes());
    out.extend_from_slice(&OFFCHAIN_FIELDS_DIGEST);
    out.push(order.metadata);
    let byte = |value: Option<u8>| value.ok_or_else(|| BadPermit::Unknown("an enum".to_owned()));
    out.push(byte(order.side.byte())?);
    out.push(byte(order.order_type.byte())?);
    out.push(byte(order.tif.byte())?);
    out.push(order.flags());
    out.extend_from_slice(&[0u8; 7]);
    Ok(out)
}

/// What a cancel names: a client order id or an exchange order id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CancelBy {
    ClientOrderId(u64),
    OrderId(u64),
}

/// The 96-byte cancel permit.
pub fn cancel_order(auth: &Auth, target: CancelBy, metadata: u8) -> Result<Vec<u8>, BadPermit> {
    let (by, id) = match target {
        CancelBy::ClientOrderId(id) => (0u8, id),
        CancelBy::OrderId(id) => (1u8, id),
    };
    let mut out = Vec::with_capacity(CANCEL_ORDER_LEN);
    out.extend_from_slice(&auth.header(command::CANCEL_ORDER)?);
    out.push(by);
    out.push(metadata);
    out.extend_from_slice(&[0u8; 6]);
    out.extend_from_slice(&id.to_le_bytes());
    Ok(out)
}

/// The 120-byte mass cancel permit: one market, every resting order in it.
pub fn mass_cancel(auth: &Auth, symbol: &str) -> Result<Vec<u8>, BadPermit> {
    let mut out = Vec::with_capacity(MASS_CANCEL_LEN);
    out.extend_from_slice(&auth.header(command::MASS_CANCEL)?);
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&symbol_bytes(symbol)?);
    Ok(out)
}

/// The 112-byte collateral return permit, which returns the maximum collateral the event's
/// positions release.
pub fn collateral_return(auth: &Auth, event_code: &str) -> Result<Vec<u8>, BadPermit> {
    let mut out = Vec::with_capacity(COLLATERAL_RETURN_LEN);
    out.extend_from_slice(&auth.header(command::COLLATERAL_RETURN)?);
    out.extend_from_slice(&event_code_bytes(event_code)?);
    out.extend_from_slice(&[0u8; 2]);
    Ok(out)
}

fn json_array(fields: Vec<serde_json::Value>) -> Vec<u8> {
    // serde_json writes compact JSON with non-ASCII unescaped, matching Pascal.
    serde_json::Value::Array(fields).to_string().into_bytes()
}

fn with_tail(mut head: Vec<serde_json::Value>, auth: &Auth) -> Vec<u8> {
    head.extend(auth.tail());
    json_array(head)
}

/// `create_trading_key`, signed by the owner wallet.
pub fn create_trading_key(
    auth: &Auth,
    name: &str,
    trading_key: &Pubkey,
    expiration_ts_ms: u64,
) -> Result<Vec<u8>, BadPermit> {
    auth.wallet()?;
    if name.len() > TRADING_KEY_NAME_LEN || name.is_empty() {
        return Err(BadPermit::KeyName);
    }
    Ok(with_tail(
        vec!["create_trading_key".into(), name.into(), trading_key.to_base58().into(), expiration_ts_ms.into()],
        auth,
    ))
}

/// `revoke_trading_key`, signed by the owner wallet.
pub fn revoke_trading_key(auth: &Auth, trading_key: &Pubkey) -> Result<Vec<u8>, BadPermit> {
    auth.wallet()?;
    Ok(with_tail(vec!["revoke_trading_key".into(), trading_key.to_base58().into()], auth))
}

/// `withdrawal`, signed by the owner wallet: the gross amount in micro-USD.
pub fn withdrawal(
    auth: &Auth,
    gross_micros: u64,
    destination_authority: &Pubkey,
    destination_token_account: &Pubkey,
) -> Result<Vec<u8>, BadPermit> {
    auth.wallet()?;
    Ok(with_tail(
        vec![
            "withdrawal".into(),
            gross_micros.into(),
            destination_authority.to_base58().into(),
            destination_token_account.to_base58().into(),
        ],
        auth,
    ))
}

/// `register_deposit_address`, signed by the owner wallet. With a referral code the array ends
/// with the literal `"referral_code"` and the code.
pub fn register_deposit_address(auth: &Auth, referral_code: Option<&str>) -> Result<Vec<u8>, BadPermit> {
    auth.wallet()?;
    let mut fields = vec!["register_deposit_address".into(), auth.owner.to_base58().into()];
    fields.extend(auth.tail());
    if let Some(code) = referral_code {
        fields.push("referral_code".into());
        fields.push(code.into());
    }
    Ok(json_array(fields))
}

/// `generate_referral_code`, signed by a trading key.
pub fn generate_referral_code(auth: &Auth) -> Result<Vec<u8>, BadPermit> {
    auth.check()?;
    Ok(with_tail(vec!["generate_referral_code".into()], auth))
}

/// `claim_custom_referral_code`, signed by a trading key.
pub fn claim_custom_referral_code(auth: &Auth, code: &str) -> Result<Vec<u8>, BadPermit> {
    auth.check()?;
    Ok(with_tail(vec!["claim_custom_referral_code".into(), code.into()], auth))
}

/// `apply_referral_code`, signed by a trading key.
pub fn apply_referral_code(auth: &Auth, code: &str) -> Result<Vec<u8>, BadPermit> {
    auth.check()?;
    Ok(with_tail(vec!["apply_referral_code".into(), code.into()], auth))
}

/// `set_username`, signed by a trading key. Empty clears it.
pub fn set_username(auth: &Auth, username: &str) -> Result<Vec<u8>, BadPermit> {
    auth.check()?;
    Ok(with_tail(vec!["set_username".into(), username.into()], auth))
}

/// `set_account_bio`, signed by a trading key. Empty clears it.
pub fn set_account_bio(auth: &Auth, bio: &str) -> Result<Vec<u8>, BadPermit> {
    auth.check()?;
    Ok(with_tail(vec!["set_account_bio".into(), bio.into()], auth))
}

/// SHA-256 of [`OFFCHAIN_FIELDS_TAG`], which must be [`OFFCHAIN_FIELDS_DIGEST`].
#[must_use]
pub fn offchain_fields_digest() -> [u8; 32] {
    Sha256::digest(OFFCHAIN_FIELDS_TAG.as_bytes()).into()
}

#[cfg(test)]
#[path = "tests/permit.rs"]
mod tests;
