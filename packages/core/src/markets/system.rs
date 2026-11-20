//! Server time, usernames, and whether a referral code exists.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::client::http::calls::segment;
use crate::client::{ApiError, Call, Reply, Transport, read};
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::lenient;

#[derive(Deserialize)]
struct Time {
    #[serde(deserialize_with = "lenient::u64s")]
    server_time_ms: u64,
}

/// Server time in unix milliseconds.
pub async fn server_time(client: &impl Transport) -> Result<Reply<u64>, ApiError> {
    Ok(read::<Time>(client, Call::get("/api/v1/time")).await?.map(|time| time.server_time_ms))
}

#[derive(Deserialize)]
struct Usernames {
    #[serde(default)]
    usernames: BTreeMap<Pubkey, String>,
}

/// Usernames for up to 100 distinct owners. Owners without a username are omitted.
pub async fn usernames(
    client: &impl Transport,
    owners: &[Pubkey],
) -> Result<Reply<BTreeMap<Pubkey, String>>, ApiError> {
    let owners: Vec<String> = owners.iter().map(Pubkey::to_base58).collect();
    let call = Call::get("/api/v1/usernames").field("owners", owners.join(","));
    Ok(read::<Usernames>(client, call).await?.map(|answer| answer.usernames))
}

/// Whether a referral code exists, and the discount it gives a referred trader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferralCodeInfo {
    pub exists: bool,
    pub taker_fee_discount_share: Dec,
    pub fee_savings_limit_usd: Dec,
}

/// Looks up a referral code: 4 to 20 lowercase letters and digits.
pub async fn referral_code(client: &impl Transport, code: &str) -> Result<Reply<ReferralCodeInfo>, ApiError> {
    read(client, Call::get(format!("/api/v1/referral-codes/{}", segment(code)))).await
}
