//! Collateral returns for mutually exclusive, collectively exhaustive events. Pascal converts
//! the account's positions across the event's unresolved markets to free as much collateral as
//! possible; there is no amount or mode parameter.

use serde::{Deserialize, Serialize};

use crate::client::{ApiError, Client, Credentials, Http, Prepared, Reply, Signed, Write};
use crate::utils::dec::Dec;
use crate::utils::lenient;
use crate::utils::permit::{self, Auth, BadPermit};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CollateralReturnBody {
    pub event_code: String,
}

/// One collateral return: the collateral freed, the size change applied to every position, and
/// per-market details. `positions` is in no particular order; match entries by symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollateralReturn {
    pub collateral_change_usd: Dec,
    #[serde(deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub position_size_change: i64,
    #[serde(default, deserialize_with = "lenient::list")]
    pub positions: Vec<CollateralReturnPosition>,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub collateral_return_ts_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollateralReturnPosition {
    pub symbol: String,
    #[serde(deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub position_size_prev: i64,
    /// The price the change is booked at, like a fill price. Across the event these prices sum
    /// to one contract's payout.
    pub accounting_price: Dec,
    pub collateral_change_usd: Dec,
    #[serde(default)]
    pub realized_pnl_usd: Dec,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub position_open_seq: Option<u64>,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub position_open_ts_ms: Option<u64>,
}

impl Write for CollateralReturnBody {
    const PATH: &'static str = "/api/v1/collateral-returns";
    type Reply = CollateralReturn;
}

pub fn prepare_collateral_return(auth: Auth, event_code: &str) -> Result<Prepared<CollateralReturnBody>, BadPermit> {
    let message = permit::collateral_return(&auth, event_code)?;
    Ok(Prepared::new(CollateralReturnBody { event_code: event_code.to_owned() }, auth, message))
}

pub fn sign_collateral_return<H: Http, K: Credentials>(
    client: &Client<H, K>,
    event_code: &str,
) -> Result<Signed<CollateralReturnBody>, ApiError> {
    let auth = client.trading_auth("a collateral return")?;
    client.sign(prepare_collateral_return(auth, event_code)?)
}

/// Returns collateral for `event_code` (the `EVENT` part of `EVENT.MARKET`).
pub async fn return_collateral<H: Http, K: Credentials>(
    client: &Client<H, K>,
    event_code: &str,
) -> Result<Reply<CollateralReturn>, ApiError> {
    client.send(&sign_collateral_return(client, event_code)?).await
}
