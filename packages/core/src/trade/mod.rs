//! Placing, replacing and canceling orders, and returning collateral. Requests are signed with
//! the client's trading key.

pub mod cancel;
pub mod collateral;
pub mod order;
pub mod orders;

pub use crate::utils::permit::CancelBy;
pub use cancel::{CancelBody, MassCancelBody, MassCancelled, cancel, cancel_all, cancel_all_batch, cancel_batch};
pub use collateral::{CollateralReturn, CollateralReturnBody, CollateralReturnPosition, return_collateral};
pub use order::{Fill, Order, OrderProgress, OrderUpdate, Placed};
pub use orders::{OrderRequest, PlaceBody, client_order_id, place, place_batch, prepare_place, sign_place};
