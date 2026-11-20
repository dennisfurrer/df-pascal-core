//! The largest open long and short positions in a market.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::account::state::Position;
use crate::client::{ApiError, Call, Reply, Transport, read};
use crate::utils::keys::Pubkey;
use crate::utils::lenient;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holder {
    pub owner: Pubkey,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Holders {
    #[serde(default, deserialize_with = "lenient::list")]
    pub longs: Vec<Holder>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub shorts: Vec<Holder>,
}

#[derive(Deserialize)]
struct Answer {
    holders: BTreeMap<String, Holders>,
}

/// The top `limit` holders per side (1 to 50, default 10) in up to 120 markets, largest first.
/// Resolved markets have none.
pub async fn top_holders(
    client: &impl Transport,
    symbols: &[&str],
    limit: Option<u32>,
) -> Result<Reply<BTreeMap<String, Holders>>, ApiError> {
    let call = Call::get("/api/v1/top-holders").field("symbols", symbols.join(",")).maybe("limit", limit);
    Ok(read::<Answer>(client, call).await?.map(|answer| answer.holders))
}
