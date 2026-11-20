//! Websocket protocol code without I/O. A program with its own socket can send [`channel`]
//! frames, decode with [`frame::decode`] and keep state with [`store`]; the native and browser
//! feeds (`client::native::Feed`, `client::web::Feed`) run a [`Session`] over a real socket.
//!
//! - [`channel`]: market data channels and subscribe frames.
//! - [`frame`]: decoding received frames into [`Frame`]s.
//! - [`account`]: the account channel's message.
//! - [`session`]: connection state: subscriptions, pings, reconnect delays.
//! - [`feed`]: events returned by a feed.
//! - [`store`]: an order book, an account and the market list built from messages.
//! - [`orders`]: order entry websocket frames and responses.

pub mod account;
pub mod channel;
pub mod feed;
pub mod frame;
pub mod orders;
pub mod session;
pub mod store;

pub use account::AccountUpdate;
pub use channel::Channel;
pub use feed::{Event, FeedError};
pub use frame::{BadFrame, Data, Frame, Kind, MarketUpdate, decode};
pub use orders::{OrderAnswer, OrderSocketError, decode_answer};
pub use session::{Refused, Retry, Session, Tick};
