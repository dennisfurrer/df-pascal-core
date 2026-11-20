//! Account profile: username and bio (set with a trading key), leaderboard ranks, trader search
//! by X handle, builder totals and taker discount terms.

use serde::{Deserialize, Serialize};

use crate::account::state::TwitterConnection;
use crate::client::http::calls::segment;
use crate::client::{ApiError, Call, Client, Credentials, Http, Prepared, Reply, Transport, Write, read};
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::lenient;
use crate::utils::order::{Category, RankBy, RankWindow};
use crate::utils::permit::{self, Auth, BadPermit};

fn account_path(owner: &Pubkey, tail: &str) -> String {
    format!("/api/v1/accounts/{}/{tail}", segment(&owner.to_base58()))
}

/// Rank and PnL in one window; absent when the PnL is zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PnlRank {
    pub rank: u64,
    pub pnl_usd: Dec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeRank {
    pub rank: u64,
    #[serde(deserialize_with = "lenient::u128s", serialize_with = "lenient::u128_text")]
    pub volume_contracts: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowRanks {
    pub window: RankWindow,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pnl: Option<PnlRank>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<VolumeRank>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ranks {
    /// The 30-minute boundary the ranks were computed at.
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub as_of_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<Pubkey>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub windows: Vec<WindowRanks>,
}

/// The account's leaderboard ranks in each window.
pub async fn ranks(
    client: &impl Transport,
    owner: &Pubkey,
    category: Option<&Category>,
) -> Result<Reply<Ranks>, ApiError> {
    read(client, Call::get(account_path(owner, "ranks")).maybe("category", category)).await
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DisplayInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub twitter_account_connection: Option<TwitterConnection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trader {
    pub owner: Pubkey,
    #[serde(default)]
    pub display_info: DisplayInfo,
    /// The query matched after a one-character correction.
    #[serde(default)]
    pub typo_corrected: bool,
    #[serde(default, deserialize_with = "lenient::list")]
    pub windows: Vec<WindowRanks>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraderSearchResults {
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub as_of_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Category>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub results: Vec<Trader>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TraderSearch {
    pub query: String,
    pub category: Option<Category>,
    pub window: Option<RankWindow>,
    pub order_by: Option<RankBy>,
    /// 1 to 50; 20 by default.
    pub limit: Option<u32>,
}

/// Searches accounts by connected X handle, in leaderboard order. Rate limit: 60 requests a
/// minute per IP.
pub async fn search_traders(
    client: &impl Transport,
    search: &TraderSearch,
) -> Result<Reply<TraderSearchResults>, ApiError> {
    let call = Call::get("/api/v1/traders/search")
        .field("query", &search.query)
        .maybe("category", search.category.as_ref())
        .maybe("window", search.window.as_ref())
        .maybe("order_by", search.order_by.as_ref())
        .maybe("limit", search.limit);
    read(client, call).await
}

/// Lifetime totals over fills of orders that set the account as builder.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BuilderStats {
    #[serde(default, deserialize_with = "lenient::u128s", serialize_with = "lenient::u128_text")]
    pub builder_volume_contracts: u128,
    #[serde(default)]
    pub builder_volume_notional_usd: Dec,
    /// Sum of `size * price * (1 - price)` over fills. The builder fee is the rate times this.
    #[serde(default)]
    pub builder_fee_base_usd: Dec,
    #[serde(default)]
    pub builder_fees_received_usd: Dec,
}

pub async fn builder_stats(client: &impl Transport, owner: &Pubkey) -> Result<Reply<BuilderStats>, ApiError> {
    read(client, Call::get(account_path(owner, "builder-stats"))).await
}

#[derive(Deserialize)]
struct Bio {
    #[serde(default)]
    account_bio: Option<String>,
}

/// The account's bio; `None` when none is set.
pub async fn bio(client: &impl Transport, owner: &Pubkey) -> Result<Reply<Option<String>>, ApiError> {
    Ok(read::<Bio>(client, Call::get(account_path(owner, "bio"))).await?.map(|bio| bio.account_bio))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TakerDiscountTerms {
    pub taker_fee_discount_share: Dec,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub created_ts_ms: u64,
    pub fee_savings_limit_usd: Dec,
    /// The discount ends once this reaches `fee_savings_limit_usd`.
    pub fee_savings_usd: Dec,
}

#[derive(Deserialize)]
struct Terms {
    #[serde(default)]
    taker_discount_terms: Option<TakerDiscountTerms>,
}

/// The account's taker discount, if it has one.
pub async fn taker_discount_terms(
    client: &impl Transport,
    owner: &Pubkey,
) -> Result<Reply<Option<TakerDiscountTerms>>, ApiError> {
    Ok(read::<Terms>(client, Call::get(account_path(owner, "taker-discount-terms")))
        .await?
        .map(|terms| terms.taker_discount_terms))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsernameBody {
    pub username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsernameSet {
    #[serde(default)]
    pub username: Option<String>,
}

impl Write for UsernameBody {
    const PATH: &'static str = "/api/v1/usernames";
    type Reply = UsernameSet;
}

/// Whether `username` is valid: 3 to 20 ASCII letters, digits, `_` or `-`, or empty to clear.
/// Uniqueness is case-insensitive.
#[must_use]
pub fn is_valid_username(username: &str) -> bool {
    username.is_empty()
        || ((3..=20).contains(&username.len())
            && username.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'))
}

pub fn prepare_username(auth: Auth, username: &str) -> Result<Prepared<UsernameBody>, BadPermit> {
    if !is_valid_username(username) {
        return Err(BadPermit::Field("a username is 3 to 20 letters, digits, `_` or `-`".to_owned()));
    }
    let message = permit::set_username(&auth, username)?;
    Ok(Prepared::new(UsernameBody { username: username.to_owned() }, auth, message))
}

/// Sets the username, or clears it when `username` is empty. The account must be funded.
pub async fn set_username<H: Http, K: Credentials>(
    client: &Client<H, K>,
    username: &str,
) -> Result<Reply<UsernameSet>, ApiError> {
    let auth = client.trading_auth("setting a username")?;
    client.send(&client.sign(prepare_username(auth, username)?)?).await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BioBody {
    pub account_bio: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BioSet {
    #[serde(default)]
    pub account_bio: Option<String>,
}

impl Write for BioBody {
    const PATH: &'static str = "/api/v1/account-bios";
    type Reply = BioSet;
}

pub fn prepare_bio(auth: Auth, bio: &str) -> Result<Prepared<BioBody>, BadPermit> {
    if bio.chars().count() > 80 {
        return Err(BadPermit::Field("a bio is at most 80 characters".to_owned()));
    }
    let message = permit::set_account_bio(&auth, bio)?;
    Ok(Prepared::new(BioBody { account_bio: bio.to_owned() }, auth, message))
}

/// Sets the bio (up to 80 characters), or clears it when `bio` is empty. The account must be
/// funded.
pub async fn set_bio<H: Http, K: Credentials>(client: &Client<H, K>, bio: &str) -> Result<Reply<BioSet>, ApiError> {
    let auth = client.trading_auth("setting a bio")?;
    client.send(&client.sign(prepare_bio(auth, bio)?)?).await
}
