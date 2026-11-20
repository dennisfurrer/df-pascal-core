//! Referral codes and referrals. Writes are signed with a trading key.
//!
//! A referrer receives 10% of a referred trader's fees, up to $10,000 per trader. The referred
//! trader gets 5% off fees until it has saved $250. Generating a code requires a funded account
//! and 2,000 weighted contracts of volume; applying a code is allowed once, below that volume.

use serde::{Deserialize, Serialize};

use crate::client::http::calls::segment;
use crate::client::{ApiError, Call, Client, Credentials, Http, Prepared, Reply, Transport, Write, read};
use crate::utils::Done;
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::lenient;
use crate::utils::permit::{self, Auth, BadPermit};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RewardTerms {
    pub referral_reward_share: Dec,
    pub per_referred_owner_reward_paid_limit_usd: Dec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Referral {
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    pub referred_owner: Pubkey,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub created_ts_ms: u64,
    pub referral_reward_share: Dec,
    pub reward_paid_usd: Dec,
    pub reward_paid_limit_usd: Dec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Referrer {
    pub owner: Pubkey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referral_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Referrals {
    pub referral_reward_terms: RewardTerms,
    #[serde(default)]
    pub reward_paid_total_usd: Dec,
    #[serde(default)]
    pub referral_count_total: u64,
    /// Newest first, up to 500.
    #[serde(default, deserialize_with = "lenient::list")]
    pub referrals: Vec<Referral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referrer: Option<Referrer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_referral_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_referral_code: Option<String>,
    #[serde(default)]
    pub custom_referral_code_revoked: bool,
    #[serde(default)]
    pub can_generate_referral_code: bool,
    #[serde(default)]
    pub can_claim_custom_referral_code: bool,
    #[serde(default)]
    pub can_apply_referral_code: bool,
}

/// The owner's codes, referrals, referrer, and which referral actions are available.
pub async fn referrals(client: &impl Transport, owner: &Pubkey) -> Result<Reply<Referrals>, ApiError> {
    read(client, Call::get(format!("/api/v1/accounts/{}/referrals", segment(&owner.to_base58())))).await
}

/// Whether `code` is `min` to 20 lowercase ASCII letters and digits.
#[must_use]
pub fn is_code(code: &str, min: usize) -> bool {
    (min..=20).contains(&code.len()) && code.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GenerateBody {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generated {
    pub generated_referral_code: String,
}

impl Write for GenerateBody {
    const PATH: &'static str = "/api/v1/referral-codes";
    type Reply = Generated;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClaimBody {
    pub referral_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claimed {
    pub custom_referral_code: String,
}

impl Write for ClaimBody {
    const PATH: &'static str = "/api/v1/referral-codes/custom";
    type Reply = Claimed;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplyBody {
    pub referral_code: String,
}

impl Write for ApplyBody {
    const PATH: &'static str = "/api/v1/referral-codes/apply";
    type Reply = Done;
}

pub fn prepare_generate(auth: Auth) -> Result<Prepared<GenerateBody>, BadPermit> {
    Ok(Prepared::new(GenerateBody {}, auth, permit::generate_referral_code(&auth)?))
}

/// A custom code is 6 to 20 lowercase letters and digits, sent as given.
pub fn prepare_claim(auth: Auth, code: &str) -> Result<Prepared<ClaimBody>, BadPermit> {
    if !is_code(code, 6) {
        return Err(BadPermit::Field("a custom referral code is 6 to 20 lowercase letters and digits".to_owned()));
    }
    Ok(Prepared::new(
        ClaimBody { referral_code: code.to_owned() },
        auth,
        permit::claim_custom_referral_code(&auth, code)?,
    ))
}

pub fn prepare_apply(auth: Auth, code: &str) -> Result<Prepared<ApplyBody>, BadPermit> {
    if !is_code(code, 4) {
        return Err(BadPermit::Field("a referral code is 4 to 20 lowercase letters and digits".to_owned()));
    }
    Ok(Prepared::new(ApplyBody { referral_code: code.to_owned() }, auth, permit::apply_referral_code(&auth, code)?))
}

/// Generates the owner's reusable referral code.
pub async fn generate_code<H: Http, K: Credentials>(client: &Client<H, K>) -> Result<Reply<Generated>, ApiError> {
    let auth = client.trading_auth("generating a referral code")?;
    client.send(&client.sign(prepare_generate(auth)?)?).await
}

/// Claims a custom referral code. An owner can have one.
pub async fn claim_code<H: Http, K: Credentials>(
    client: &Client<H, K>,
    code: &str,
) -> Result<Reply<Claimed>, ApiError> {
    let auth = client.trading_auth("claiming a referral code")?;
    client.send(&client.sign(prepare_claim(auth, code)?)?).await
}

/// Sets a referrer on an account that registered without one. Allowed once and cannot be
/// undone.
pub async fn apply_code<H: Http, K: Credentials>(client: &Client<H, K>, code: &str) -> Result<Reply<Done>, ApiError> {
    let auth = client.trading_auth("applying a referral code")?;
    client.send(&client.sign(prepare_apply(auth, code)?)?).await
}
