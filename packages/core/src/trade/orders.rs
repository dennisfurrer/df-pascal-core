//! Placing and replacing orders, one at a time or in batches of up to 50. Each order is a
//! separately signed permit and succeeds or fails independently.

use serde::Serialize;

use crate::client::{ApiError, Client, Credentials, Http, Prepared, Refusal, Reply, Signed, Write};
use crate::trade::order::Placed;
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::order::{OrderType, Side, TimeInForce};
use crate::utils::permit::{self, Auth, BadPermit, Builder, PlaceOrder};
use crate::utils::price::{Outcome, yes_no};

/// Maximum orders per batch. `MAX_RESTING_PER_MARKET` is the maximum resting orders per account
/// in one market.
pub const MAX_BATCH: usize = 50;
pub const MAX_RESTING_PER_MARKET: usize = 50;

/// An order to place, holding the fields its permit signs. Build with [`OrderRequest::limit`]
/// or [`OrderRequest::market`] and the setters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderRequest(pub PlaceOrder);

impl OrderRequest {
    /// A good-til-canceled limit order.
    #[must_use]
    pub fn limit(symbol: impl Into<String>, side: Side, price: Dec, size: u64, client_order_id: u64) -> OrderRequest {
        OrderRequest(PlaceOrder {
            client_order_id,
            replace_client_order_id: None,
            symbol: symbol.into(),
            side,
            price,
            size,
            tif: TimeInForce::Gtc,
            order_type: OrderType::Limit,
            expires_ts_ms: None,
            post_only: false,
            reduce_only: false,
            allow_missing_replace: false,
            replace_with_open_size: false,
            builder: None,
            metadata: 0,
        })
    }

    /// A market order: immediate-or-cancel, filling only at `worst_price` or better. A bid
    /// with `worst_price` 1 has no price limit.
    #[must_use]
    pub fn market(
        symbol: impl Into<String>,
        side: Side,
        size: u64,
        worst_price: Dec,
        client_order_id: u64,
    ) -> OrderRequest {
        let mut order = OrderRequest::limit(symbol, side, worst_price, size, client_order_id);
        order.0.order_type = OrderType::Market;
        order.0.tif = TimeInForce::Ioc;
        order
    }

    /// Converts a web app Yes/No action to an order: buying Yes is a bid at p, buying No is an
    /// ask at 1 - p, and selling is the opposite side with reduce-only set.
    #[must_use]
    pub fn outcome(
        symbol: impl Into<String>,
        outcome: Outcome,
        buy: bool,
        price: Dec,
        size: u64,
        client_order_id: u64,
    ) -> OrderRequest {
        let (side, price, reduce_only) = yes_no(outcome, buy, price);
        let mut order = OrderRequest::limit(symbol, side, price, size, client_order_id);
        order.0.reduce_only = reduce_only;
        order
    }

    /// Post-only: refused with `WOULD_CROSS` if it would trade immediately.
    #[must_use]
    pub const fn post_only(mut self) -> Self {
        self.0.post_only = true;
        self
    }

    /// Reduce-only: the order can only reduce the position.
    #[must_use]
    pub const fn reduce_only(mut self) -> Self {
        self.0.reduce_only = true;
        self
    }

    /// Immediate-or-cancel: any size that does not fill immediately is canceled.
    #[must_use]
    pub fn ioc(mut self) -> Self {
        self.0.tif = TimeInForce::Ioc;
        self.0.expires_ts_ms = None;
        self
    }

    /// Good-til-time: canceled once `expires_ts_ms` passes.
    #[must_use]
    pub fn good_til(mut self, expires_ts_ms: u64) -> Self {
        self.0.tif = TimeInForce::Gtt;
        self.0.expires_ts_ms = Some(expires_ts_ms);
        self
    }

    /// Atomically cancels the open order `client_order_id` and places this one. The order keeps
    /// its queue priority only if the size is reduced.
    #[must_use]
    pub const fn replacing(mut self, client_order_id: u64) -> Self {
        self.0.replace_client_order_id = Some(client_order_id);
        self
    }

    /// With [`OrderRequest::replacing`]: place the order even if the order to replace no longer
    /// exists.
    #[must_use]
    pub const fn allow_missing_replace(mut self) -> Self {
        self.0.allow_missing_replace = true;
        self
    }

    /// With [`OrderRequest::replacing`]: the new order takes the old order's open size at the
    /// time it is processed. Set `size` to the expected open size, since `size` is what is signed.
    #[must_use]
    pub const fn replace_with_open_size(mut self) -> Self {
        self.0.replace_with_open_size = true;
        self
    }

    /// Pays `builder` a fee of `fee_rate` (at most 0.1) times each fill's
    /// `size * price * (1 - price)`. The builder must be another account's wallet key with at
    /// least $100 of free collateral.
    #[must_use]
    pub const fn builder(mut self, builder: Pubkey, fee_rate: Dec) -> Self {
        self.0.builder = Some(Builder { pubkey: builder, fee_rate });
        self
    }

    /// Sets the metadata byte, overriding the client's default.
    #[must_use]
    pub const fn metadata(mut self, metadata: u8) -> Self {
        self.0.metadata = metadata;
        self
    }
}

/// Place request body: numbers as strings, prices with six decimals, and optional fields only
/// when they differ from the default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlaceBody {
    pub client_order_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace_client_order_id: Option<String>,
    pub symbol: String,
    pub side: Side,
    pub price: String,
    pub size: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tif: Option<TimeInForce>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub post_only: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub reduce_only: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub allow_missing_replace: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub replace_with_open_size: bool,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub order_type: Option<OrderType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_ts_ms: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builder_info: Option<BuilderInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuilderInfo {
    pub builder_pubkey: Pubkey,
    pub builder_fee_rate: String,
}

impl Write for PlaceBody {
    const PATH: &'static str = "/api/v1/orders";
    const BATCH: bool = true;
    const ORDERS: bool = true;
    const MAX_BATCH: usize = MAX_BATCH;
    type Reply = Placed;
}

impl PlaceBody {
    #[must_use]
    pub fn of(order: &PlaceOrder) -> PlaceBody {
        PlaceBody {
            client_order_id: order.client_order_id.to_string(),
            replace_client_order_id: order.replace_client_order_id.map(|id| id.to_string()),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            price: order.price.to_fixed(6),
            size: order.size.to_string(),
            tif: (order.tif != TimeInForce::Gtc).then(|| order.tif.clone()),
            post_only: order.post_only,
            reduce_only: order.reduce_only,
            allow_missing_replace: order.allow_missing_replace,
            replace_with_open_size: order.replace_with_open_size,
            order_type: (order.order_type != OrderType::Limit).then(|| order.order_type.clone()),
            expires_ts_ms: order.expires_ts_ms.map(|ts| ts.to_string()),
            builder_info: order.builder.map(|builder| BuilderInfo {
                builder_pubkey: builder.pubkey,
                builder_fee_rate: builder.fee_rate.to_fixed(6),
            }),
            metadata: (order.metadata != 0).then(|| order.metadata.to_string()),
        }
    }
}

/// Builds the place request for `order` and the 232-byte message to sign. No network calls.
pub fn prepare_place(auth: Auth, order: &OrderRequest) -> Result<Prepared<PlaceBody>, BadPermit> {
    let message = permit::place_order(&auth, &order.0)?;
    Ok(Prepared::new(PlaceBody::of(&order.0), auth, message))
}

/// Signs `order` with the client's trading key without sending it, for the order websocket or
/// for display.
pub fn sign_place<H: Http, K: Credentials>(
    client: &Client<H, K>,
    order: &OrderRequest,
) -> Result<Signed<PlaceBody>, ApiError> {
    let mut order = order.clone();
    if order.0.metadata == 0 {
        order.0.metadata = client.metadata();
    }
    let auth = client.trading_auth("an order")?;
    client.sign(prepare_place(auth, &order)?)
}

/// Places one order.
pub async fn place<H: Http, K: Credentials>(
    client: &Client<H, K>,
    order: &OrderRequest,
) -> Result<Reply<Placed>, ApiError> {
    client.send(&sign_place(client, order)?).await
}

/// Places up to 50 orders in one request and returns one result per order, in order. Each order
/// is signed with its own increasing timestamp. A duplicate client order id is refused before
/// sending.
pub async fn place_batch<H: Http, K: Credentials>(
    client: &Client<H, K>,
    orders: &[OrderRequest],
) -> Result<Vec<Result<Reply<Placed>, Refusal>>, ApiError> {
    let mut seen = std::collections::HashSet::new();
    if let Some(repeat) = orders.iter().find(|order| !seen.insert(order.0.client_order_id)) {
        return Err(ApiError::Invalid(BadPermit::Field(format!(
            "client_order_id {} is in the batch twice",
            repeat.0.client_order_id
        ))));
    }
    let signed = orders.iter().map(|order| sign_place(client, order)).collect::<Result<Vec<_>, _>>()?;
    client.send_batch(&signed).await
}

/// A client order id from the clock: `now_ms * 1000 + n % 1000`. Gives distinct, increasing ids
/// for up to 1,000 orders per millisecond.
#[must_use]
pub const fn client_order_id(now_ms: u64, n: u16) -> u64 {
    now_ms.saturating_mul(1_000).saturating_add((n % 1_000) as u64)
}
