//! Trading keys: Ed25519 keys an owner authorizes to place and cancel orders, but not to withdraw.
//! A key name is at most 16 bytes and public on chain, a key expires at most 365 days ahead, and
//! an account holds at most 10 unrevoked keys (expired keys count until revoked).

use serde::{Deserialize, Serialize};

use crate::client::http::calls::segment;
use crate::client::{ApiError, Call, Client, Http, Prepared, Reply, Transport, Write, read};
use crate::utils::keys::Pubkey;
use crate::utils::lenient;
use crate::utils::permit::{self, Auth, BadPermit, MAX_TRADING_KEY_LIFETIME_MS};
use crate::utils::sign::Signer;
use crate::utils::time::DAY_MS;

pub const MAX_KEYS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TradingKey {
    pub trading_key: Pubkey,
    pub name: String,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub expiration_ts_ms: u64,
    /// Number of the account's open orders placed with this key.
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub open_order_count: u64,
}

impl TradingKey {
    #[must_use]
    pub const fn is_expired(&self, now_ms: u64) -> bool {
        self.expiration_ts_ms <= now_ms
    }
}

/// The owner's active trading keys.
pub async fn trading_keys(client: &impl Transport, owner: &Pubkey) -> Result<Reply<Vec<TradingKey>>, ApiError> {
    read(client, Call::get(format!("/api/v1/accounts/{}/trading-keys", segment(&owner.to_base58())))).await
}

/// The owners for which `trading_key` is active. One key can trade for several accounts.
pub async fn trading_key_accounts(
    client: &impl Transport,
    trading_key: &Pubkey,
) -> Result<Reply<Vec<Pubkey>>, ApiError> {
    read(client, Call::get(format!("/api/v1/trading-keys/{}/accounts", segment(&trading_key.to_base58())))).await
}

/// Whether `trading_key` is active for `owner`. Orders signed with an inactive key are refused
/// with `unauthorized_signer`.
pub async fn is_active(client: &impl Transport, owner: &Pubkey, trading_key: &Pubkey) -> Result<bool, ApiError> {
    Ok(trading_keys(client, owner).await?.data.iter().any(|key| key.trading_key == *trading_key))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateTradingKeyBody {
    pub trading_key: Pubkey,
    pub name: String,
    pub expiration_ts_ms: String,
}

impl Write for CreateTradingKeyBody {
    const PATH: &'static str = "/api/v1/trading-keys";
    type Reply = TradingKey;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevokeTradingKeyBody {
    pub trading_key: Pubkey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revoked {
    pub trading_key: Pubkey,
}

impl Write for RevokeTradingKeyBody {
    const PATH: &'static str = "/api/v1/trading-key-revocations";
    type Reply = Revoked;
}

/// Builds the create request and the JSON message for the owner's wallet to sign. No network
/// calls.
pub fn prepare_create(
    auth: Auth,
    trading_key: &Pubkey,
    name: &str,
    expiration_ts_ms: u64,
) -> Result<Prepared<CreateTradingKeyBody>, BadPermit> {
    if expiration_ts_ms > auth.client_ts_ms.saturating_add(MAX_TRADING_KEY_LIFETIME_MS) {
        return Err(BadPermit::Field("a trading key expires at most 365 days ahead".to_owned()));
    }
    let message = permit::create_trading_key(&auth, name, trading_key, expiration_ts_ms)?;
    let body = CreateTradingKeyBody {
        trading_key: *trading_key,
        name: name.to_owned(),
        expiration_ts_ms: expiration_ts_ms.to_string(),
    };
    Ok(Prepared::new(body, auth, message))
}

/// Builds the revoke request and the JSON message for the owner's wallet to sign. No network
/// calls.
pub fn prepare_revoke(auth: Auth, trading_key: &Pubkey) -> Result<Prepared<RevokeTradingKeyBody>, BadPermit> {
    let message = permit::revoke_trading_key(&auth, trading_key)?;
    Ok(Prepared::new(RevokeTradingKeyBody { trading_key: *trading_key }, auth, message))
}

impl<H: Http, K> Client<H, K> {
    /// Builds a request authorizing `trading_key` for `owner` for `valid_days` (1 to 365), for
    /// the owner's wallet to sign.
    pub fn prepare_trading_key(
        &self,
        owner: Pubkey,
        trading_key: &Pubkey,
        name: &str,
        valid_days: u64,
    ) -> Result<Prepared<CreateTradingKeyBody>, ApiError> {
        if !(1..=365).contains(&valid_days) {
            return Err(ApiError::Invalid(BadPermit::Field("a trading key is valid for 1 to 365 days".to_owned())));
        }
        let auth = self.wallet_auth(owner);
        let expiration = auth.client_ts_ms.saturating_add(valid_days * DAY_MS);
        Ok(prepare_create(auth, trading_key, name, expiration)?)
    }

    /// Builds a request revoking `trading_key`, for the owner's wallet to sign. Orders placed
    /// with the key can no longer match.
    pub fn prepare_revoke_trading_key(
        &self,
        owner: Pubkey,
        trading_key: &Pubkey,
    ) -> Result<Prepared<RevokeTradingKeyBody>, ApiError> {
        Ok(prepare_revoke(self.wallet_auth(owner), trading_key)?)
    }
}

/// Authorizes `trading_key`, signing with a local owner `wallet`. The account must be funded.
pub async fn create_trading_key<H: Http, K>(
    client: &Client<H, K>,
    wallet: &impl Signer,
    trading_key: &Pubkey,
    name: &str,
    valid_days: u64,
) -> Result<Reply<TradingKey>, ApiError> {
    let prepared = client.prepare_trading_key(wallet.pubkey(), trading_key, name, valid_days)?;
    client.send(&prepared.sign(wallet)?).await
}

/// Revokes `trading_key`, signing with a local owner `wallet`.
pub async fn revoke_trading_key<H: Http, K>(
    client: &Client<H, K>,
    wallet: &impl Signer,
    trading_key: &Pubkey,
) -> Result<Reply<Revoked>, ApiError> {
    let prepared = client.prepare_revoke_trading_key(wallet.pubkey(), trading_key)?;
    client.send(&prepared.sign(wallet)?).await
}
