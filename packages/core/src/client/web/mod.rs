//! HTTP and websockets for browsers and Workers. [`Web`] sends requests with the global `fetch`,
//! [`clock`] uses `Date.now()`, and [`Feed`] and [`OrderSocket`] use the global `WebSocket`.
//!
//! ```ignore
//! use df_pascal_core::Client;
//! use df_pascal_core::client::web::{Web, clock};
//!
//! let client = Client::reader(Web, clock);
//! ```
//!
//! `fetch`, `WebSocket` and `setTimeout` are read from the global object, so this works in a
//! page, a Web Worker, a Cloudflare Worker and Node 22. Requests are never retried.

pub mod socket;

use crate::client::{Http, Request, Response, TransportError};
use js_sys::{Array, Function, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, RequestInit};

pub use socket::{Feed, OrderSocket, Stopper};

/// HTTP client using the global `fetch`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Web;

impl Http for Web {
    async fn send(&self, request: Request) -> Result<Response, TransportError> {
        request.refuse_if_offline()?;
        let global = js_sys::global();
        let fetch: Function =
            Reflect::get(&global, &JsValue::from_str("fetch")).map_err(unreachable)?.dyn_into().map_err(unreachable)?;
        let headers = Headers::new().map_err(unreachable)?;
        for (name, value) in &request.headers {
            headers.append(name, value).map_err(unreachable)?;
        }
        let init = RequestInit::new();
        init.set_method(request.method.as_str());
        init.set_headers(&headers);
        if !request.body.is_empty() {
            init.set_body(&Uint8Array::from(request.body.as_slice()));
        }
        let promise: Promise = fetch
            .call2(&global, &JsValue::from_str(&request.url), &init)
            .map_err(unreachable)?
            .dyn_into()
            .map_err(unreachable)?;
        let answer: web_sys::Response =
            JsFuture::from(promise).await.map_err(unreachable)?.dyn_into().map_err(unreachable)?;
        let status = answer.status();
        let headers = headers_of(&answer.headers());
        let buffer = JsFuture::from(answer.array_buffer().map_err(unreachable)?).await.map_err(unreachable)?;
        Ok(Response { status, headers, body: Uint8Array::new(&buffer).to_vec() })
    }
}

fn headers_of(headers: &Headers) -> Vec<(String, String)> {
    let Ok(Some(entries)) = js_sys::try_iter(headers) else { return Vec::new() };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let pair: Array = entry.dyn_into().ok()?;
            Some((pair.get(0).as_string()?, pair.get(1).as_string()?))
        })
        .collect()
}

/// `fetch` failed without a response. For a write the outcome is unknown.
fn unreachable(error: JsValue) -> TransportError {
    let message = Reflect::get(&error, &JsValue::from_str("message")).ok().and_then(|message| message.as_string());
    TransportError::Unreachable(message.unwrap_or_else(|| "the request did not complete".to_owned()))
}

/// Unix milliseconds from `Date.now()`.
#[must_use]
pub fn clock() -> u64 {
    // Whole milliseconds below 2^53, so the cast is exact.
    js_sys::Date::now() as u64
}
