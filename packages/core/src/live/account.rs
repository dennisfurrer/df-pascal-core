//! The account channel's message: account state fields (all of them in a snapshot, only changed
//! ones in an update) and the events since the last message.

use serde::{Deserialize, Serialize};

use crate::account::history::{ClosedPosition, PositionResolution, Transfer};
use crate::account::state::AccountState;
use crate::trade::collateral::CollateralReturn;
use crate::trade::order::{Fill, OrderUpdate};
use crate::utils::lenient;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AccountUpdate {
    #[serde(flatten)]
    pub state: AccountState,
    /// Order changes, each with its reason.
    #[serde(default, deserialize_with = "lenient::list", skip_serializing_if = "Vec::is_empty")]
    pub order_updates: Vec<OrderUpdate>,
    #[serde(default, deserialize_with = "lenient::list", skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<Fill>,
    #[serde(default, deserialize_with = "lenient::list", skip_serializing_if = "Vec::is_empty")]
    pub resolutions: Vec<PositionResolution>,
    /// Positions closed, flipped or resolved since the last message, newest close first.
    #[serde(default, deserialize_with = "lenient::list", skip_serializing_if = "Vec::is_empty")]
    pub closed_positions: Vec<ClosedPosition>,
    #[serde(default, deserialize_with = "lenient::list", skip_serializing_if = "Vec::is_empty")]
    pub transfers: Vec<Transfer>,
    /// The same objects the write API returns; match them by `seq` to avoid counting one twice.
    #[serde(default, deserialize_with = "lenient::list", skip_serializing_if = "Vec::is_empty")]
    pub collateral_returns: Vec<CollateralReturn>,
}
