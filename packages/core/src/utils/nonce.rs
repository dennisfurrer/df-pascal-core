//! Client timestamps (`client_ts_ms`, unix milliseconds). For wallet-signed requests the timestamp
//! is also the replay nonce and must increase per account (`CLIENT_TS_NOT_INCREASING`), so
//! [`Timestamps::next`] never returns the same value twice.

use std::sync::{Mutex, PoisonError};

#[derive(Debug, Default)]
pub struct Timestamps(Mutex<u64>);

impl Timestamps {
    /// Returns the larger of `now_ms` and one more than the last value returned.
    pub fn next(&self, now_ms: u64) -> u64 {
        let mut last = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let next = now_ms.max(last.saturating_add(1));
        *last = next;
        next
    }

    /// Returns `now_ms` unchanged (for order batches and mass cancels) and records it so a later
    /// [`Timestamps::next`] is greater.
    pub fn now(&self, now_ms: u64) -> u64 {
        let mut last = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *last = (*last).max(now_ms);
        now_ms
    }

    #[must_use]
    pub fn last(&self) -> u64 {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
