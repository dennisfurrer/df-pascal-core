//! Events and errors returned by the native and browser feeds.

use crate::live::frame::{BadFrame, Frame};
use crate::live::session::{Refused, Retry};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The socket opened; all subscribed channels are being subscribed again.
    Opened,
    Frame(Box<Frame>),
    /// A frame that could not be decoded. The socket is still open.
    Unreadable(BadFrame),
    /// The socket closed or failed to open. The feed reconnects after `retry.after_ms`.
    Closed {
        reason: String,
        retry: Retry,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FeedError {
    #[error(transparent)]
    Refused(#[from] Refused),
    #[error("the feed has stopped")]
    Stopped,
}
