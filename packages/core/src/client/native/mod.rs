//! HTTP and websockets for native programs. [`Native`] sends requests with reqwest on rustls,
//! [`clock`] reads the system clock, and [`Feed`] and [`OrderSocket`] connect to the market data
//! and order entry websockets with tokio-tungstenite.
//!
//! ```no_run
//! use df_pascal_core::Client;
//! use df_pascal_core::client::native::{Native, clock};
//!
//! let client = Client::reader(Native::new(), clock);
//! ```
//!
//! [`Native`] futures must run inside a tokio runtime. Requests are never retried.

pub mod socket;

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::client::{Client, Http, Method, Request, Response, TransportError};

pub use socket::{Feed, OrderSocket};

/// Default request timeout, covering connection and response.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// reqwest-based HTTP client. Clones share one connection pool.
#[derive(Debug, Clone)]
pub struct Native {
    client: reqwest::Client,
}

impl Native {
    #[must_use]
    pub fn new() -> Self {
        Native::with_timeout(TIMEOUT)
    }

    /// Sets the request timeout. A write that times out returns
    /// [`TransportError::Unreachable`] and may still have been executed.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Self {
        let client = reqwest::Client::builder().timeout(timeout).build().unwrap_or_else(|_| reqwest::Client::new());
        Native { client }
    }

    /// Uses a reqwest client built by the caller.
    #[must_use]
    pub const fn with_client(client: reqwest::Client) -> Self {
        Native { client }
    }
}

impl Default for Native {
    fn default() -> Self {
        Native::new()
    }
}

impl Http for Native {
    async fn send(&self, request: Request) -> Result<Response, TransportError> {
        request.refuse_if_offline()?;
        let unreachable = |error: reqwest::Error| TransportError::Unreachable(error.without_url().to_string());
        let method = match request.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
        };
        let mut outgoing = self.client.request(method, &request.url);
        for (name, value) in &request.headers {
            outgoing = outgoing.header(name, value);
        }
        if !request.body.is_empty() {
            outgoing = outgoing.body(request.body);
        }
        let answer = outgoing.send().await.map_err(unreachable)?;
        let status = answer.status().as_u16();
        let headers = answer
            .headers()
            .iter()
            .map(|(name, value)| (name.as_str().to_owned(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
            .collect();
        let body = answer.bytes().await.map_err(unreachable)?.to_vec();
        Ok(Response { status, headers, body })
    }
}

/// Default interval for [`keep_warm`], shorter than the idle timeout of pooled connections.
pub const KEEP_WARM: Duration = Duration::from_secs(30);

/// Fetches the server time every `every` so the pooled connection to the read host stays open
/// and the next request skips DNS and the TLS handshake. The task ends when all other `Arc`s to
/// `client` are dropped.
pub fn keep_warm<K: Send + Sync + 'static>(
    client: &Arc<Client<Native, K>>,
    every: Duration,
) -> tokio::task::JoinHandle<()> {
    let held = Arc::downgrade(client);
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(every);
        loop {
            ticks.tick().await;
            let Some(client) = held.upgrade() else {
                return;
            };
            let _ = crate::markets::system::server_time(&*client).await;
        }
    })
}

/// Unix milliseconds from the system clock. Returns 0 if the clock is before 1970.
#[must_use]
pub fn clock() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
}
