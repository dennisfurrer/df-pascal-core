//! Rate-limited calls to Pascal's REST APIs for a [`crate::client::Client`]. This layer builds
//! the URLs; the [`Http`] transport does not know about hosts.

use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::client::Clock;
use crate::client::deployment::Deployment;
use crate::client::http::calls::{Call, Host};
use crate::client::http::limit::{Gate, Spent};
use crate::client::http::wire::{Http, Method, Request, TransportError};

pub(crate) struct Api<H> {
    http: H,
    deployment: Deployment,
    clock: Clock,
    gate: Mutex<Gate>,
}

impl<H: Http> Api<H> {
    pub(crate) fn new(http: H, deployment: Deployment, clock: Clock) -> Self {
        Api { http, deployment, clock, gate: Mutex::new(Gate::default()) }
    }

    pub(crate) fn with_deployment(mut self, deployment: Deployment) -> Self {
        self.deployment = deployment;
        self
    }

    pub(crate) const fn deployment(&self) -> &Deployment {
        &self.deployment
    }

    pub(crate) const fn http(&self) -> &H {
        &self.http
    }

    pub(crate) const fn clock(&self) -> Clock {
        self.clock
    }

    fn gate(&self) -> MutexGuard<'_, Gate> {
        self.gate.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn spent(&self) -> Spent {
        self.gate().spent((self.clock)())
    }

    /// The full URL for a call, with the query string encoded.
    pub(crate) fn url(&self, call: &Call) -> String {
        let root = match call.host {
            Host::Read => &self.deployment.read_url,
            Host::Write => &self.deployment.write_url,
        };
        let query = call.query_string();
        match query.is_empty() {
            true => format!("{root}{}", call.path),
            false => format!("{root}{}?{query}", call.path),
        }
    }

    pub(crate) fn request(&self, call: &Call) -> Request {
        let url = self.url(call);
        match call.method {
            Method::Get => Request::get(url),
            Method::Post => {
                let request = Request::post_json(url, call.body.clone().unwrap_or_default());
                match call.host {
                    Host::Write => request.write(),
                    Host::Read => request,
                }
            }
        }
    }

    /// Sends a call if the rate limiter allows it. A 429 blocks further requests and is returned
    /// as [`TransportError::RateLimited`]. Nothing is retried.
    pub(crate) async fn send(&self, call: &Call) -> Result<(u16, String), TransportError> {
        self.gate()
            .admit((self.clock)(), call.orders)
            .map_err(|wait| TransportError::RateLimited { retry_after_ms: Some(wait) })?;
        let response = self.http.send(self.request(call)).await?;
        if response.status == 429 {
            let wait = self.gate().struck((self.clock)(), response.retry_after_ms());
            return Err(TransportError::RateLimited { retry_after_ms: Some(wait) });
        }
        Ok((response.status, response.text()))
    }
}
