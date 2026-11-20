//! Paged history requests. A page's `next_cursor` is sent back as `before_cursor` to get the next
//! older page; cursors are opaque strings.

use serde::{Deserialize, Serialize};

use crate::client::Call;
use crate::utils::lenient;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Page<T> {
    #[serde(default, deserialize_with = "lenient::list")]
    pub items: Vec<T>,
    /// The cursor for the next older page; absent on the last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

impl<T> Default for Page<T> {
    fn default() -> Self {
        Page { items: Vec::new(), next_cursor: None }
    }
}

impl<T> Page<T> {
    #[must_use]
    pub const fn is_last(&self) -> bool {
        self.next_cursor.is_none()
    }

    /// The request for the page after this one, or `None` on the last.
    #[must_use]
    pub fn next(&self, request: &PageRequest) -> Option<PageRequest> {
        self.next_cursor.as_ref().map(|cursor| PageRequest {
            limit: request.limit,
            before_cursor: Some(cursor.clone()),
            at_or_before_seq: None,
        })
    }
}

/// Paging parameters for a history request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageRequest {
    /// 1 to 500; Pascal's default is 100.
    pub limit: Option<u32>,
    /// The previous page's `next_cursor`.
    pub before_cursor: Option<String>,
    /// Start at this sequence number, inclusive. Cannot be combined with `before_cursor`.
    pub at_or_before_seq: Option<u64>,
}

impl PageRequest {
    #[must_use]
    pub fn limit(limit: u32) -> PageRequest {
        PageRequest { limit: Some(limit), ..PageRequest::default() }
    }

    #[must_use]
    pub fn before(mut self, cursor: impl Into<String>) -> PageRequest {
        self.before_cursor = Some(cursor.into());
        self.at_or_before_seq = None;
        self
    }

    #[must_use]
    pub fn at_or_before(mut self, seq: u64) -> PageRequest {
        self.at_or_before_seq = Some(seq);
        self.before_cursor = None;
        self
    }

    pub(crate) fn apply(&self, call: Call) -> Call {
        call.maybe("limit", self.limit)
            .maybe("before_cursor", self.before_cursor.as_deref())
            .maybe("at_or_before_seq", self.at_or_before_seq)
    }
}
