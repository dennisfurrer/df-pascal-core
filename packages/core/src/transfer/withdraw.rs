//! Withdrawals of collateral to a wallet's USDC account, signed by the owner's wallet.
//!
//! `amount` is the debit; Pascal sends it minus the withdrawal fee ($0.50 at the time of
//! writing). The destination token account is derived before signing because both addresses are
//! part of the permit.

use serde::{Deserialize, Serialize};

use crate::client::{ApiError, Client, Http, Prepared, Reply, Write};
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::permit::{self, Auth, BadPermit};
use crate::utils::sign::Signer;
use crate::utils::solana::associated_token_address;

/// The withdrawal fee from Pascal's docs. Pascal charges its current fee, which can differ.
pub const WITHDRAWAL_FEE_USD: Dec = Dec::lit::<2>(50);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WithdrawBody {
    pub amount: String,
    pub destination_authority: Pubkey,
    pub destination_token_account: Pubkey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Withdrawn {
    pub owner: Pubkey,
    pub amount: Dec,
    pub destination_authority: Pubkey,
    pub destination_token_account: Pubkey,
}

impl Write for WithdrawBody {
    const PATH: &'static str = "/api/v1/withdrawals";
    type Reply = Withdrawn;
}

/// Builds the withdrawal request and the JSON message for the owner's wallet to sign. No network
/// calls. `usd_mint` is the deployment's ([`crate::client::Deployment::usd_mint`]).
pub fn prepare_withdrawal(
    auth: Auth,
    amount: Dec,
    destination_authority: &Pubkey,
    usd_mint: &Pubkey,
) -> Result<Prepared<WithdrawBody>, BadPermit> {
    let token_account = associated_token_address(destination_authority, usd_mint);
    prepare_withdrawal_to_account(auth, amount, destination_authority, &token_account)
}

/// Builds a withdrawal to an explicit token account, for a deployment whose USD mint is not
/// known here. Pascal refuses an account that is not `destination_authority`'s associated token
/// account for its mint.
pub fn prepare_withdrawal_to_account(
    auth: Auth,
    amount: Dec,
    destination_authority: &Pubkey,
    destination_token_account: &Pubkey,
) -> Result<Prepared<WithdrawBody>, BadPermit> {
    let micros = permit::usd_micros(amount)?;
    let message = permit::withdrawal(&auth, micros, destination_authority, destination_token_account)?;
    let body = WithdrawBody {
        amount: amount.to_fixed(6),
        destination_authority: *destination_authority,
        destination_token_account: *destination_token_account,
    };
    Ok(Prepared::new(body, auth, message))
}

/// `amount` minus [`WITHDRAWAL_FEE_USD`], floored at zero.
#[must_use]
pub fn net_of_fee(amount: Dec) -> Dec {
    amount.checked_sub(WITHDRAWAL_FEE_USD).unwrap_or(Dec::ZERO).max(Dec::ZERO)
}

impl<H: Http, K> Client<H, K> {
    /// Builds a withdrawal of `amount` from `owner`'s account to `destination`'s USDC account,
    /// for the owner's wallet to sign. `destination` is a wallet address, not a token account.
    pub fn prepare_withdrawal(
        &self,
        owner: Pubkey,
        amount: Dec,
        destination: &Pubkey,
    ) -> Result<Prepared<WithdrawBody>, ApiError> {
        Ok(prepare_withdrawal(self.wallet_auth(owner), amount, destination, &self.deployment().usd_mint)?)
    }
}

/// Withdraws, signing with a local owner `wallet`.
pub async fn withdraw<H: Http, K>(
    client: &Client<H, K>,
    wallet: &impl Signer,
    amount: Dec,
    destination: &Pubkey,
) -> Result<Reply<Withdrawn>, ApiError> {
    let prepared = client.prepare_withdrawal(wallet.pubkey(), amount, destination)?;
    client.send(&prepared.sign(wallet)?).await
}
