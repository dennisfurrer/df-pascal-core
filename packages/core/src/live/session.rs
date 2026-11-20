//! Market data connection state without I/O: subscriptions (kept across reconnects), frames to
//! send, keepalive pings, the silence timeout and reconnect backoff. A feed runs it over a
//! socket; a program with its own socket can drive it directly.

use std::collections::BTreeSet;

use crate::live::channel::{self, Channel};
use crate::live::frame::{self, BadFrame, Frame};

/// Maximum subscriptions per connection.
pub const MAX_SUBSCRIPTIONS: usize = 5_000;
/// Interval between pings, in ms.
pub const PING_EVERY_MS: u64 = 20_000;
/// A socket that receives nothing, including pongs, for this many ms is reconnected.
pub const SILENCE_MS: u64 = 60_000;
/// Delay before the first reconnect, in ms. Each further failure doubles it, up to
/// [`MAX_RETRY_MS`], plus up to 25% random jitter.
pub const FIRST_RETRY_MS: u64 = 500;
pub const MAX_RETRY_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("a connection takes at most {MAX_SUBSCRIPTIONS} subscriptions")]
    TooMany,
}

/// Delay before the next reconnect attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retry {
    pub after_ms: u64,
    /// Consecutive failures, including this one.
    pub attempt: u32,
}

/// The action the session needs now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tick {
    Wait,
    /// Frames are waiting: send [`Session::owed`].
    Send,
    /// Nothing received for [`SILENCE_MS`]: close and reconnect.
    Reconnect,
}

#[derive(Debug, Clone)]
pub struct Session {
    url: String,
    wanted: BTreeSet<Channel>,
    owed: Vec<String>,
    open: bool,
    last_heard_ms: u64,
    last_ping_ms: u64,
    failures: u32,
}

impl Session {
    #[must_use]
    pub fn new(url: impl Into<String>) -> Session {
        Session {
            url: url.into(),
            wanted: BTreeSet::new(),
            owed: Vec::new(),
            open: false,
            last_heard_ms: 0,
            last_ping_ms: 0,
            failures: 0,
        }
    }

    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    pub fn wanted(&self) -> impl Iterator<Item = &Channel> {
        self.wanted.iter()
    }

    /// Subscribes to `channel`. The frame is sent now if the socket is open, otherwise on the
    /// next connection.
    pub fn subscribe(&mut self, channel: Channel) -> Result<(), Refused> {
        if self.wanted.contains(&channel) {
            return Ok(());
        }
        if self.wanted.len() >= MAX_SUBSCRIPTIONS {
            return Err(Refused::TooMany);
        }
        if self.open {
            self.owed.push(channel::subscribe(std::slice::from_ref(&channel)));
        }
        self.wanted.insert(channel);
        Ok(())
    }

    pub fn unsubscribe(&mut self, channel: &Channel) {
        if self.wanted.remove(channel) && self.open {
            self.owed.push(channel::unsubscribe(std::slice::from_ref(channel)));
        }
    }

    /// Call when the socket opens. Returns subscribe frames for all channels, at most 100
    /// channels per frame.
    pub fn opened(&mut self, now_ms: u64) -> Vec<String> {
        self.open = true;
        self.last_heard_ms = now_ms;
        self.last_ping_ms = now_ms;
        self.owed.clear();
        let wanted: Vec<Channel> = self.wanted.iter().cloned().collect();
        wanted.chunks(100).map(channel::subscribe).collect()
    }

    /// Call when a text frame arrives. Returns the decoded frame.
    pub fn received(&mut self, now_ms: u64, text: &str) -> Result<Frame, BadFrame> {
        self.last_heard_ms = now_ms;
        let frame = frame::decode(text);
        if frame.is_ok() {
            self.failures = 0;
        }
        frame
    }

    /// Call when a non-text frame, such as a protocol ping, arrives. Resets the silence timer.
    pub fn heard(&mut self, now_ms: u64) {
        self.last_heard_ms = now_ms;
    }

    /// Frames to send now, including a ping if one is due.
    pub fn owed(&mut self, now_ms: u64) -> Vec<String> {
        if !self.open {
            return Vec::new();
        }
        if now_ms.saturating_sub(self.last_ping_ms) >= PING_EVERY_MS {
            self.last_ping_ms = now_ms;
            self.owed.push(channel::PING.to_owned());
        }
        std::mem::take(&mut self.owed)
    }

    /// The action needed at `now_ms`.
    pub fn tick(&mut self, now_ms: u64) -> Tick {
        if !self.open {
            return Tick::Wait;
        }
        if now_ms.saturating_sub(self.last_heard_ms) >= SILENCE_MS {
            return Tick::Reconnect;
        }
        match !self.owed.is_empty() || now_ms.saturating_sub(self.last_ping_ms) >= PING_EVERY_MS {
            true => Tick::Send,
            false => Tick::Wait,
        }
    }

    /// Time in ms at which [`Session::tick`] next has something to do.
    #[must_use]
    pub fn due(&self) -> Option<u64> {
        self.open.then(|| (self.last_ping_ms + PING_EVERY_MS).min(self.last_heard_ms + SILENCE_MS))
    }

    /// Call when the socket closes or fails to open. Returns the reconnect delay; `random` is
    /// used for jitter and can be any random bits.
    pub fn closed(&mut self, _now_ms: u64, random: u64) -> Retry {
        self.open = false;
        self.owed.clear();
        self.failures = self.failures.saturating_add(1);
        let base = FIRST_RETRY_MS.saturating_mul(1u64 << self.failures.saturating_sub(1).min(16)).min(MAX_RETRY_MS);
        let jitter = random % (base / 4 + 1);
        Retry { after_ms: base + jitter, attempt: self.failures }
    }
}

#[cfg(test)]
#[path = "tests/session.rs"]
mod tests;
