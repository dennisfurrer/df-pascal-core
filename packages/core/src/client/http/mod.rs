//! HTTP access to Pascal's REST APIs: the [`Http`] transport, the [`Transport`] trait used by
//! all reads and writes, response envelopes, error codes and the rate limiter.

mod api;
pub mod calls;
pub mod codes;
pub mod limit;
pub mod wire;

pub(crate) use api::Api;
pub use calls::{ApiError, Call, Context, Host, Refusal, Reply, Transport, decode, decode_batch, read, read_batch};
pub use codes::ErrorCode;
pub use limit::Spent;
pub use wire::{Http, Method, OFFLINE, Request, Response, TransportError};
