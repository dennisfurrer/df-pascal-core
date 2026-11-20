//! Client-side rate limiter. Pascal accepts up to 100 order requests per second (orders, cancels,
//! mass cancels) and returns HTTP 429 `RATE_LIMITED` above any limit. The limiter rejects order
//! requests over that budget before sending, and after a 429 blocks all requests until its
//! `Retry-After` has passed (1 second if none is given).

use std::collections::VecDeque;

/// Order requests per second, from Pascal's docs.
pub const ORDER_REQUESTS_PER_SECOND: usize = 100;
/// How long requests are blocked after a 429 with no `Retry-After`, in ms.
pub const DEFAULT_COOLDOWN_MS: u64 = 1_000;

#[derive(Debug, Default)]
pub struct Gate {
    closed_until_ms: u64,
    orders: VecDeque<u64>,
}

/// Recent request count and remaining block time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Spent {
    /// Order requests sent in the last second.
    pub order_requests: usize,
    /// Milliseconds until requests are allowed again; 0 when not blocked.
    pub cooldown_ms: u64,
}

impl Gate {
    /// Admits a request at `now_ms`, or returns how many ms to wait.
    pub fn admit(&mut self, now_ms: u64, orders: bool) -> Result<(), u64> {
        if now_ms < self.closed_until_ms {
            return Err(self.closed_until_ms - now_ms);
        }
        if !orders {
            return Ok(());
        }
        self.forget(now_ms);
        if self.orders.len() >= ORDER_REQUESTS_PER_SECOND {
            let oldest = self.orders.front().copied().unwrap_or(now_ms);
            return Err((oldest + 1_000).saturating_sub(now_ms).max(1));
        }
        self.orders.push_back(now_ms);
        Ok(())
    }

    /// Records a 429 and blocks requests for `retry_after_ms`. Returns the wait in ms.
    pub fn struck(&mut self, now_ms: u64, retry_after_ms: Option<u64>) -> u64 {
        let wait = retry_after_ms.unwrap_or(DEFAULT_COOLDOWN_MS);
        self.closed_until_ms = self.closed_until_ms.max(now_ms + wait);
        wait
    }

    pub fn spent(&mut self, now_ms: u64) -> Spent {
        self.forget(now_ms);
        Spent { order_requests: self.orders.len(), cooldown_ms: self.closed_until_ms.saturating_sub(now_ms) }
    }

    fn forget(&mut self, now_ms: u64) {
        while self.orders.front().is_some_and(|at| *at + 1_000 <= now_ms) {
            self.orders.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hundred_and_first_order_request_in_a_second_waits_and_reads_never_do() {
        let mut gate = Gate::default();
        for _ in 0..ORDER_REQUESTS_PER_SECOND {
            assert!(gate.admit(10_000, true).is_ok());
        }
        assert_eq!(gate.admit(10_500, true), Err(500));
        assert!(gate.admit(10_500, false).is_ok());
        assert!(gate.admit(11_000, true).is_ok());
    }

    #[test]
    fn a_429_closes_the_gate_for_its_retry_after() {
        let mut gate = Gate::default();
        assert_eq!(gate.struck(1_000, Some(3_000)), 3_000);
        assert_eq!(gate.admit(2_000, false), Err(2_000));
        assert!(gate.admit(4_000, false).is_ok());
        gate.struck(5_000, None);
        assert_eq!(gate.spent(5_500).cooldown_ms, 500);
    }
}
