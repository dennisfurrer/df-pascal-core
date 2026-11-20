//! Cancels by client order id or exchange order id (up to 50 per batch), and mass cancels of all
//! resting orders in a market (up to 10 markets per batch).
//!
//! A mass cancel also cancels orders in the same market signed at or before its timestamp in the
//! same matching round. This client's timestamps always increase, so an order signed after a
//! mass cancel is not affected by it.

use serde::{Deserialize, Serialize};

use crate::client::{ApiError, Client, Credentials, Http, Prepared, Refusal, Reply, Signed, Write};
use crate::trade::order::Order;
use crate::utils::lenient;
use crate::utils::permit::{self, Auth, BadPermit, CancelBy};

pub const MAX_BATCH: usize = 50;
pub const MAX_MASS_CANCEL_BATCH: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CancelBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

impl Write for CancelBody {
    const PATH: &'static str = "/api/v1/cancels";
    const BATCH: bool = true;
    const ORDERS: bool = true;
    const MAX_BATCH: usize = MAX_BATCH;
    /// The canceled order's final state.
    type Reply = Order;
}

/// Builds the cancel request for `target` and the 96-byte message to sign. No network calls.
pub fn prepare_cancel(auth: Auth, target: CancelBy, metadata: u8) -> Result<Prepared<CancelBody>, BadPermit> {
    let message = permit::cancel_order(&auth, target, metadata)?;
    let (client_order_id, order_id) = match target {
        CancelBy::ClientOrderId(id) => (Some(id.to_string()), None),
        CancelBy::OrderId(id) => (None, Some(id.to_string())),
    };
    let metadata = (metadata != 0).then(|| metadata.to_string());
    Ok(Prepared::new(CancelBody { client_order_id, order_id, metadata }, auth, message))
}

pub fn sign_cancel<H: Http, K: Credentials>(
    client: &Client<H, K>,
    target: CancelBy,
) -> Result<Signed<CancelBody>, ApiError> {
    let auth = client.trading_auth("a cancel")?;
    client.sign(prepare_cancel(auth, target, client.metadata())?)
}

pub async fn cancel<H: Http, K: Credentials>(
    client: &Client<H, K>,
    target: CancelBy,
) -> Result<Reply<Order>, ApiError> {
    client.send(&sign_cancel(client, target)?).await
}

/// Up to 50 cancels in one request. Returns one result per cancel, in order.
pub async fn cancel_batch<H: Http, K: Credentials>(
    client: &Client<H, K>,
    targets: &[CancelBy],
) -> Result<Vec<Result<Reply<Order>, Refusal>>, ApiError> {
    let signed = targets.iter().map(|target| sign_cancel(client, *target)).collect::<Result<Vec<_>, _>>()?;
    client.send_batch(&signed).await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MassCancelBody {
    pub symbol: String,
}

/// The orders that were resting when the mass cancel ran. An order placed in the same round and
/// canceled by it is reported in its own placement response as `MASS_CANCELLED`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MassCancelled {
    #[serde(default, deserialize_with = "lenient::list")]
    pub orders: Vec<Order>,
}

impl Write for MassCancelBody {
    const PATH: &'static str = "/api/v1/mass-cancels";
    const BATCH: bool = true;
    const ORDERS: bool = true;
    const MAX_BATCH: usize = MAX_MASS_CANCEL_BATCH;
    type Reply = MassCancelled;
}

pub fn prepare_mass_cancel(auth: Auth, symbol: &str) -> Result<Prepared<MassCancelBody>, BadPermit> {
    let message = permit::mass_cancel(&auth, symbol)?;
    Ok(Prepared::new(MassCancelBody { symbol: symbol.to_owned() }, auth, message))
}

pub fn sign_mass_cancel<H: Http, K: Credentials>(
    client: &Client<H, K>,
    symbol: &str,
) -> Result<Signed<MassCancelBody>, ApiError> {
    let auth = client.trading_auth("a mass cancel")?;
    client.sign(prepare_mass_cancel(auth, symbol)?)
}

/// Cancels all of the account's resting orders in `symbol`, from any trading key.
pub async fn cancel_all<H: Http, K: Credentials>(
    client: &Client<H, K>,
    symbol: &str,
) -> Result<Reply<MassCancelled>, ApiError> {
    client.send(&sign_mass_cancel(client, symbol)?).await
}

/// Mass cancels in up to 10 markets in one request.
pub async fn cancel_all_batch<H: Http, K: Credentials>(
    client: &Client<H, K>,
    symbols: &[&str],
) -> Result<Vec<Result<Reply<MassCancelled>, Refusal>>, ApiError> {
    let signed = symbols.iter().map(|symbol| sign_mass_cancel(client, symbol)).collect::<Result<Vec<_>, _>>()?;
    client.send_batch(&signed).await
}
