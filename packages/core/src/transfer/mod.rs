//! Deposit address registration, USDC deposits on Solana and withdrawals. The owner's wallet
//! signs each request.

pub mod deposit;
pub mod withdraw;

pub use deposit::{
    DepositAddress, DepositError, DepositPlan, RegisterBody, check_deposit, deposit, deposit_address,
    deposit_transaction, register_deposit_address,
};
pub use withdraw::{
    WITHDRAWAL_FEE_USD, WithdrawBody, Withdrawn, net_of_fee, prepare_withdrawal, prepare_withdrawal_to_account,
    withdraw,
};
