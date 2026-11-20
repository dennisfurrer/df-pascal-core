//! The [`Client`] for Pascal's REST APIs ([`http`]), plus deployments, two-step writes
//! ([`signed`]) and the native and browser transports.

pub mod deployment;
pub mod http;
#[cfg(feature = "native")]
pub mod native;
pub mod signed;
#[cfg(feature = "web")]
pub mod web;

use std::sync::atomic::{AtomicI64, Ordering};

use crate::utils::keys::Pubkey;
use crate::utils::nonce::Timestamps;
use crate::utils::permit::{Auth, BadPermit, DEFAULT_RECV_WINDOW_MS, MAX_RECV_WINDOW_MS};
use crate::utils::sign::Signer;

pub use deployment::Deployment;
pub use http::{
    ApiError, Call, Context, ErrorCode, Host, Http, Method, OFFLINE, Refusal, Reply, Request, Response, Spent,
    Transport, TransportError, decode, read, read_batch,
};
pub use signed::{Prepared, Signed, Write};

/// Returns unix time in milliseconds. It is passed in because std has no clock in browsers or
/// Workers, and tests need a fixed time.
pub type Clock = fn() -> u64;

/// The signing credentials a client holds: none (`()`) or a [`Trader`].
pub trait Credentials {
    /// The account the requests are for.
    fn owner(&self) -> Option<Pubkey>;
    /// The trading key that signs the requests.
    fn trading_key(&self) -> Option<&dyn Signer>;
}

impl Credentials for () {
    fn owner(&self) -> Option<Pubkey> {
        None
    }

    fn trading_key(&self) -> Option<&dyn Signer> {
        None
    }
}

/// An account owner and a trading key authorized for it. The trading key signs orders, cancels,
/// collateral returns, referral codes, username and bio; withdrawals and trading key changes are
/// signed by the owner's wallet.
#[derive(Debug, Clone)]
pub struct Trader<S> {
    pub owner: Pubkey,
    pub key: S,
}

impl<S: Signer> Trader<S> {
    pub const fn new(owner: Pubkey, key: S) -> Trader<S> {
        Trader { owner, key }
    }
}

impl<S: Signer> Credentials for Trader<S> {
    fn owner(&self) -> Option<Pubkey> {
        Some(self.owner)
    }

    fn trading_key(&self) -> Option<&dyn Signer> {
        Some(&self.key)
    }
}

/// Pascal API client. Read functions in every module take it (`markets::list(&client)`); with a
/// [`Trader`] it also signs the account's trading requests. `K` is `()` for a client that only
/// reads and prepares wallet-signed requests.
///
/// Client timestamps come from one strictly increasing counter, so a mass cancel never covers an
/// order sent after it and wallet-signed requests satisfy Pascal's increasing-nonce rule.
/// Requests are never retried.
pub struct Client<H, K = ()> {
    api: http::Api<H>,
    key: K,
    timestamps: Timestamps,
    /// Pascal's clock minus the local clock, in ms ([`Client::sync_clock`]).
    offset_ms: AtomicI64,
    recv_window_ms: u32,
    metadata: u8,
}

/// Prints only the deployment name, not the key or transport.
impl<H: Http, K> std::fmt::Debug for Client<H, K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").field("deployment", &self.deployment().name).finish_non_exhaustive()
    }
}

impl<H: Http> Client<H> {
    /// A client that reads and prepares wallet-signed requests, without a trading key.
    pub fn reader(http: H, clock: Clock) -> Self {
        Client::new(http, clock, ())
    }
}

impl<H: Http, K> Client<H, K> {
    /// A production client using `http`, signing trading requests with `key`.
    pub fn new(http: H, clock: Clock, key: K) -> Self {
        Client {
            api: http::Api::new(http, Deployment::prod(), clock),
            key,
            timestamps: Timestamps::default(),
            offset_ms: AtomicI64::new(0),
            recv_window_ms: DEFAULT_RECV_WINDOW_MS,
            metadata: 0,
        }
    }

    /// Uses another deployment, such as canary, a proxy or a test server.
    #[must_use]
    pub fn with_deployment(mut self, deployment: Deployment) -> Self {
        self.api = self.api.with_deployment(deployment);
        self
    }

    /// How long a signed request stays valid after its timestamp, in ms. Default 5,000, maximum
    /// 60,000; larger values are clamped.
    #[must_use]
    pub fn with_recv_window(mut self, ms: u32) -> Self {
        self.recv_window_ms = ms.min(MAX_RECV_WINDOW_MS);
        self
    }

    /// Client id byte sent with orders and cancels so Pascal can tell which client sent them (the
    /// Python quickstart uses 2). It is signed into the permit and sent as `metadata`; 0, the
    /// default, is not sent.
    #[must_use]
    pub const fn with_metadata(mut self, metadata: u8) -> Self {
        self.metadata = metadata;
        self
    }

    /// Returns the same client with a different key, for a trading key loaded later.
    pub fn with_key<K2>(self, key: K2) -> Client<H, K2> {
        Client {
            api: self.api,
            key,
            timestamps: self.timestamps,
            offset_ms: self.offset_ms,
            recv_window_ms: self.recv_window_ms,
            metadata: self.metadata,
        }
    }

    #[must_use]
    pub const fn deployment(&self) -> &Deployment {
        self.api.deployment()
    }

    #[must_use]
    pub const fn http(&self) -> &H {
        self.api.http()
    }

    #[must_use]
    pub const fn key(&self) -> &K {
        &self.key
    }

    #[must_use]
    pub const fn recv_window_ms(&self) -> u32 {
        self.recv_window_ms
    }

    #[must_use]
    pub const fn metadata(&self) -> u8 {
        self.metadata
    }

    /// Current time on Pascal's clock as estimated by this client, in unix ms.
    #[must_use]
    pub fn now_ms(&self) -> u64 {
        (self.api.clock())().saturating_add_signed(self.offset_ms.load(Ordering::Relaxed))
    }

    /// Pascal's clock minus the local clock, in ms. 0 until [`Client::sync_clock`] runs.
    #[must_use]
    pub fn clock_offset_ms(&self) -> i64 {
        self.offset_ms.load(Ordering::Relaxed)
    }

    /// Fetches Pascal's server time and stores the offset from the local clock, so timestamps stay
    /// inside the receive window when the local clock is wrong. Returns the offset in ms.
    pub async fn sync_clock(&self) -> Result<i64, ApiError> {
        let before = (self.api.clock())();
        let time = crate::markets::system::server_time(self).await?;
        let after = (self.api.clock())();
        let middle = i64::try_from(before / 2 + after / 2).unwrap_or(i64::MAX);
        let offset = i64::try_from(time.data).unwrap_or(i64::MAX).saturating_sub(middle);
        self.offset_ms.store(offset, Ordering::Relaxed);
        Ok(offset)
    }

    /// Requests sent in the last second, and whether the rate limiter is blocking.
    pub fn spent(&self) -> Spent {
        self.api.spent()
    }

    /// The next client timestamp: Pascal's time, always greater than the previous one.
    #[must_use]
    pub fn next_timestamp(&self) -> u64 {
        self.timestamps.next(self.now_ms())
    }

    /// The `auth` for a request for `owner` signed by `signer`, at the next timestamp.
    #[must_use]
    pub fn auth(&self, owner: Pubkey, signer: Pubkey) -> Auth {
        Auth {
            deployment_id: self.deployment().deployment_id,
            owner,
            signer,
            client_ts_ms: self.next_timestamp(),
            recv_window_ms: self.recv_window_ms,
        }
    }

    /// The `auth` for a request signed by the owner's wallet.
    #[must_use]
    pub fn wallet_auth(&self, owner: Pubkey) -> Auth {
        self.auth(owner, owner)
    }

    /// Sends a signed write. For a batch endpoint it sends a batch of one and returns its result.
    pub async fn send<B: Write>(&self, signed: &Signed<B>) -> Result<Reply<B::Reply>, ApiError> {
        if B::BATCH {
            let mut answers = self.send_batch(std::slice::from_ref(signed)).await?;
            return match answers.pop() {
                Some(answer) => answer.map_err(ApiError::Refused),
                None => {
                    Err(ApiError::Unreadable { path: B::PATH.to_owned(), detail: "an empty batch answer".to_owned() })
                }
            };
        }
        let body = serde_json::to_string(signed)
            .map_err(|error| ApiError::Unreadable { path: B::PATH.to_owned(), detail: error.to_string() })?;
        let mut call = Call::write(B::PATH, body);
        if B::ORDERS {
            call = call.orders();
        }
        read(&Unsigned(self), call).await
    }

    /// Sends signed writes as one batch and returns one result per request, in order. Each item
    /// succeeds or fails on its own; a refusal of the whole request (rate limit, oversized batch)
    /// is returned as the error.
    pub async fn send_batch<B: Write>(
        &self,
        signed: &[Signed<B>],
    ) -> Result<Vec<Result<Reply<B::Reply>, Refusal>>, ApiError> {
        if signed.is_empty() {
            return Ok(Vec::new());
        }
        if !B::BATCH || signed.len() > B::MAX_BATCH {
            return Err(ApiError::Invalid(BadPermit::Field(format!(
                "{} takes at most {} requests at once",
                B::PATH,
                B::MAX_BATCH
            ))));
        }
        let body = serde_json::to_string(signed)
            .map_err(|error| ApiError::Unreadable { path: B::PATH.to_owned(), detail: error.to_string() })?;
        let mut call = Call::write(B::PATH, body);
        if B::ORDERS {
            call = call.orders();
        }
        read_batch(&Unsigned(self), call).await
    }
}

impl<H: Http, K: Credentials> Client<H, K> {
    /// The account this client trades for.
    #[must_use]
    pub fn owner(&self) -> Option<Pubkey> {
        self.key.owner()
    }

    /// The trading key's public key.
    #[must_use]
    pub fn trading_key(&self) -> Option<Pubkey> {
        self.key.trading_key().map(Signer::pubkey)
    }

    /// The `auth` for the next trading request, or an error if there is no trading key.
    pub fn trading_auth(&self, what: &str) -> Result<Auth, ApiError> {
        match (self.key.owner(), self.key.trading_key()) {
            (Some(owner), Some(key)) => Ok(self.auth(owner, key.pubkey())),
            _ => Err(ApiError::Transport(TransportError::NoKey(what.to_owned()))),
        }
    }

    /// Signs a prepared trading request with the client's trading key.
    pub fn sign<B>(&self, prepared: Prepared<B>) -> Result<Signed<B>, ApiError> {
        let key =
            self.key.trading_key().ok_or_else(|| ApiError::Transport(TransportError::NoKey("a request".to_owned())))?;
        let signature = key.sign_message(prepared.message())?;
        Ok(prepared.with_signature_unchecked(signature))
    }
}

impl<H: Http, K> Transport for Client<H, K> {
    async fn call(&self, call: Call) -> Result<(u16, String), TransportError> {
        self.api.send(&call).await
    }
}

/// The client's transport, usable whatever key type the client holds.
struct Unsigned<'a, H, K>(&'a Client<H, K>);

impl<H: Http, K> Transport for Unsigned<'_, H, K> {
    async fn call(&self, call: Call) -> Result<(u16, String), TransportError> {
        self.0.api.send(&call).await
    }
}

#[cfg(test)]
#[cfg(all(feature = "sign", not(target_arch = "wasm32")))]
#[path = "tests/client.rs"]
mod tests;
