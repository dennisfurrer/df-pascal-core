//! Deposits. The owner registers a deposit address (signed by the owner's wallet, optionally
//! with a referral code), then sends USDC to `ATA(deposit_address, USDC)`. The deposit address is
//! a program derived address and is never created as an account; Pascal scans its token account
//! and credits the owner.
//!
//! [`deposit_transaction`] builds the transfer as an unsigned Solana transaction for a wallet to
//! sign; [`deposit`] signs and sends it with a local key. Only send USDC.

use serde::{Deserialize, Serialize};

use crate::client::http::calls::segment;
use crate::client::{ApiError, Call, Client, Http, Prepared, Reply, Transport, Write, read};
use crate::utils::dec::Dec;
use crate::utils::keys::Pubkey;
use crate::utils::permit::{self, Auth, BadPermit};
use crate::utils::sign::Signer;
use crate::utils::solana::rpc::{Rpc, RpcError};
use crate::utils::solana::tx::{
    BadTransaction, Instruction, Message, Transaction, create_associated_token_account_idempotent,
    set_compute_unit_limit, set_compute_unit_price, transfer_checked,
};
use crate::utils::solana::{USDC_DECIMALS, associated_token_address};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepositAddress {
    pub owner: Pubkey,
    /// Set once registered and never changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deposit_address: Option<Pubkey>,
    #[serde(default)]
    pub registered: bool,
}

impl DepositAddress {
    /// The deposit address's associated token account for `usd_mint`, where USDC is sent.
    #[must_use]
    pub fn token_account(&self, usd_mint: &Pubkey) -> Option<Pubkey> {
        self.deposit_address.map(|address| associated_token_address(&address, usd_mint))
    }
}

pub async fn deposit_address(client: &impl Transport, owner: &Pubkey) -> Result<Reply<DepositAddress>, ApiError> {
    read(client, Call::get(format!("/api/v1/accounts/{}/deposit-address", segment(&owner.to_base58())))).await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegisterBody {
    pub owner: Pubkey,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referral_code: Option<String>,
}

impl Write for RegisterBody {
    const PATH: &'static str = "/api/v1/deposit-addresses";
    type Reply = DepositAddress;
}

/// Builds the registration request and the JSON message for the owner's wallet to sign. An
/// unknown referral code fails the registration with `REFERRAL_CODE_INVALID` and the account
/// stays unregistered.
pub fn prepare_register(auth: Auth, referral_code: Option<&str>) -> Result<Prepared<RegisterBody>, BadPermit> {
    let message = permit::register_deposit_address(&auth, referral_code)?;
    Ok(Prepared::new(
        RegisterBody { owner: auth.owner, referral_code: referral_code.map(str::to_owned) },
        auth,
        message,
    ))
}

impl<H: Http, K> Client<H, K> {
    pub fn prepare_register_deposit_address(
        &self,
        owner: Pubkey,
        referral_code: Option<&str>,
    ) -> Result<Prepared<RegisterBody>, ApiError> {
        Ok(prepare_register(self.wallet_auth(owner), referral_code)?)
    }
}

/// Registers the deposit address, signing with a local owner `wallet`.
pub async fn register_deposit_address<H: Http, K>(
    client: &Client<H, K>,
    wallet: &impl Signer,
    referral_code: Option<&str>,
) -> Result<Reply<DepositAddress>, ApiError> {
    let prepared = client.prepare_register_deposit_address(wallet.pubkey(), referral_code)?;
    client.send(&prepared.sign(wallet)?).await
}

/// Parameters for building a deposit transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepositPlan {
    /// The owner's wallet, which pays the fee and signs.
    pub owner: Pubkey,
    pub deposit_address: Pubkey,
    pub usd_mint: Pubkey,
    /// In USDC base units (millionths).
    pub amount: u64,
    /// Add an instruction that creates the deposit address's USDC account if it is missing.
    /// Creating it costs the owner about 0.002 SOL of rent.
    pub create_destination: bool,
    /// A priority fee: compute unit limit and micro-lamports per unit.
    pub priority: Option<(u32, u64)>,
}

impl DepositPlan {
    /// A plan to send `amount` USD from `owner`'s USDC account to `deposit_address`.
    pub fn new(
        owner: Pubkey,
        deposit_address: Pubkey,
        usd_mint: Pubkey,
        amount: Dec,
    ) -> Result<DepositPlan, BadPermit> {
        Ok(DepositPlan {
            owner,
            deposit_address,
            usd_mint,
            amount: permit::usd_micros(amount)?,
            create_destination: false,
            priority: None,
        })
    }

    #[must_use]
    pub fn source(&self) -> Pubkey {
        associated_token_address(&self.owner, &self.usd_mint)
    }

    #[must_use]
    pub fn destination(&self) -> Pubkey {
        associated_token_address(&self.deposit_address, &self.usd_mint)
    }

    #[must_use]
    pub fn instructions(&self) -> Vec<Instruction> {
        let mut instructions = Vec::new();
        if let Some((units, price)) = self.priority {
            instructions.push(set_compute_unit_limit(units));
            instructions.push(set_compute_unit_price(price));
        }
        if self.create_destination {
            instructions.push(create_associated_token_account_idempotent(
                self.owner,
                self.destination(),
                self.deposit_address,
                self.usd_mint,
            ));
        }
        instructions.push(transfer_checked(
            self.source(),
            self.usd_mint,
            self.destination(),
            self.owner,
            self.amount,
            USDC_DECIMALS,
        ));
        instructions
    }
}

/// Builds the deposit as an unsigned transaction with `blockhash`. Pass the
/// [`Transaction::serialize`] bytes to a wallet's `signAndSendTransaction`.
pub fn deposit_transaction(plan: &DepositPlan, blockhash: [u8; 32]) -> Result<Transaction, BadTransaction> {
    Ok(Transaction::new(Message::compile(&plan.owner, &plan.instructions(), blockhash)?))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DepositError {
    #[error(transparent)]
    Rpc(#[from] RpcError),
    #[error(transparent)]
    Transaction(#[from] BadTransaction),
    #[error("the owner's USDC account holds {held} base units; the deposit is {wanted}")]
    Insufficient { held: u64, wanted: u64 },
    #[error("the signer is {signer}, but the plan's owner is {owner}")]
    WrongSigner { signer: Pubkey, owner: Pubkey },
    #[error(transparent)]
    Sign(#[from] crate::utils::sign::SignError),
}

/// Checks the owner's USDC balance and whether the destination account exists, and returns the
/// plan with `create_destination` set accordingly.
pub async fn check_deposit<H: Http>(rpc: &Rpc<H>, mut plan: DepositPlan) -> Result<DepositPlan, DepositError> {
    let held = rpc.token_balance(&plan.source()).await?.map_or(0, |balance| balance.amount);
    if held < plan.amount {
        return Err(DepositError::Insufficient { held, wanted: plan.amount });
    }
    plan.create_destination = !rpc.account_exists(&plan.destination()).await?;
    Ok(plan)
}

/// Checks, builds, signs with `signer` (the owner's local key) and sends the deposit. Returns
/// the transaction signature; poll it with [`Rpc::signature_status`].
pub async fn deposit<H: Http>(
    rpc: &Rpc<H>,
    signer: &impl Signer,
    plan: DepositPlan,
) -> Result<crate::utils::keys::Signature, DepositError> {
    if signer.pubkey() != plan.owner {
        return Err(DepositError::WrongSigner { signer: signer.pubkey(), owner: plan.owner });
    }
    let plan = check_deposit(rpc, plan).await?;
    let blockhash = rpc.latest_blockhash().await?;
    let mut transaction = deposit_transaction(&plan, blockhash.hash)?;
    let signature = signer.sign_message(&transaction.message.serialize())?;
    transaction.add_signature(&plan.owner, &signature)?;
    Ok(rpc.send_transaction(&transaction.serialize()?).await?)
}
