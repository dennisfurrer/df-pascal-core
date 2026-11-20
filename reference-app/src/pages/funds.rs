//! Funds page: deposit address registration, USDC deposits and withdrawals through the wallet, and
//! pending transfers.

use df_pascal_core::account::{self, StateQuery};
use df_pascal_core::client::web::Web;
use df_pascal_core::transfer::{self, DepositPlan, WITHDRAWAL_FEE_USD, net_of_fee};
use df_pascal_core::utils::solana::rpc::Rpc;
use df_pascal_core::utils::solana::{USDC_DECIMALS, associated_token_address};
use df_pascal_core::{Dec, Pubkey};
use leptos::prelude::*;

use crate::fmt;
use crate::pages::keys::wallet_send;
use crate::state::{describe, use_app};
use crate::ui::{NeedAccount, Notice};
use crate::wallet;

#[component]
pub fn Funds() -> impl IntoView {
    let app = use_app();
    move || match app.owner.get() {
        None => view! { <NeedAccount /> }.into_any(),
        Some(owner) => view! { <FundsFor owner=owner /> }.into_any(),
    }
}

fn usdc(amount: u64) -> Dec {
    Dec::new(i128::from(amount), u32::from(USDC_DECIMALS)).unwrap_or(Dec::ZERO)
}

#[component]
fn FundsFor(owner: Pubkey) -> impl IntoView {
    let app = use_app();
    let shown = RwSignal::new(None::<String>);
    let address = LocalResource::new(move || {
        let _ = app.revision.get();
        let client = app.client();
        async move {
            transfer::deposit_address(&*client, &owner).await.map(|reply| reply.data).map_err(|error| describe(&error))
        }
    });
    let state = LocalResource::new(move || {
        let _ = app.revision.get();
        let client = app.client();
        async move {
            account::state(&*client, &owner, &StateQuery { exclude_orders: true, ..StateQuery::default() })
                .await
                .map(|reply| reply.data)
                .map_err(|error| describe(&error))
        }
    });
    let mint = app.deployment_config().usd_mint;
    let balances = LocalResource::new(move || {
        let _ = app.revision.get();
        let rpc = Rpc::new(Web, app.rpc_url.get());
        async move {
            let usdc_balance =
                rpc.token_balance(&associated_token_address(&owner, &mint)).await.map_err(|error| error.to_string())?;
            let sol = rpc.sol_balance(&owner).await.map_err(|error| error.to_string())?;
            Ok::<_, String>((usdc_balance.map_or(0, |balance| balance.amount), sol))
        }
    });

    let referral = RwSignal::new(String::new());
    let register = move |_| {
        let code = referral.get().trim().to_owned();
        let prepared =
            match app.client().prepare_register_deposit_address(owner, (!code.is_empty()).then_some(code.as_str())) {
                Ok(prepared) => prepared,
                Err(error) => return app.fail(describe(&error)),
            };
        leptos::task::spawn_local(async move {
            match wallet_send(app, prepared, shown).await {
                Ok(_) => {
                    app.toast("Deposit address registered", true);
                    app.bump();
                }
                Err(error) => app.fail(error),
            }
        });
    };

    let deposit_amount = RwSignal::new(String::new());
    let deposit_status = RwSignal::new(None::<String>);
    let deposit = move |deposit_address: Pubkey| {
        let amount = match Dec::parse(deposit_amount.get().trim()) {
            Ok(amount) => amount,
            Err(_) => return app.fail("Amount is a number of dollars"),
        };
        if app.wallet.get_untracked().is_none() {
            return app.fail("Deposits are sent by the owner's wallet: connect it first.");
        }
        let plan = match DepositPlan::new(owner, deposit_address, mint, amount) {
            Ok(plan) => plan,
            Err(error) => return app.fail(error),
        };
        let rpc = Rpc::new(Web, app.rpc_url.get_untracked());
        deposit_status.set(Some("Checking the wallet's USDC".to_owned()));
        leptos::task::spawn_local(async move {
            let result: Result<String, String> = async {
                let plan = transfer::check_deposit(&rpc, plan).await.map_err(|error| error.to_string())?;
                let blockhash = rpc.latest_blockhash().await.map_err(|error| error.to_string())?;
                let transaction =
                    transfer::deposit_transaction(&plan, blockhash.hash).map_err(|error| error.to_string())?;
                let bytes = transaction.serialize().map_err(|error| error.to_string())?;
                deposit_status.set(Some("Waiting for the wallet".to_owned()));
                let signature = wallet::sign_and_send(&bytes).await?;
                deposit_status.set(Some(format!("Sent {}; waiting for confirmation", signature.to_base58())));
                for _ in 0..30 {
                    gloo_sleep(2_000).await;
                    if let Ok(Some(status)) = rpc.signature_status(&signature).await {
                        if let Some(error) = status.error {
                            return Err(format!("The transaction failed: {error}"));
                        }
                        if status.is_confirmed() {
                            return Ok(format!(
                                "Deposit confirmed on Solana ({}). Pascal credits it once it has scanned the address.",
                                signature.to_base58()
                            ));
                        }
                    }
                }
                Ok(format!("Sent {}, not yet confirmed. Check it on a Solana explorer.", signature.to_base58()))
            }
            .await;
            match result {
                Ok(text) => {
                    deposit_status.set(Some(text.clone()));
                    app.toast(text, true);
                    app.bump();
                }
                Err(error) => {
                    deposit_status.set(Some(error.clone()));
                    app.fail(error);
                }
            }
        });
    };

    let withdraw_amount = RwSignal::new(String::new());
    let destination = RwSignal::new(owner.to_base58());
    let withdraw = move |_| {
        let amount = match Dec::parse(withdraw_amount.get().trim()) {
            Ok(amount) => amount,
            Err(_) => return app.fail("Amount is a number of dollars"),
        };
        let to = match Pubkey::parse(destination.get().trim()) {
            Ok(to) => to,
            Err(error) => return app.fail(error),
        };
        if !to.is_on_curve() {
            return app.fail("The destination is a program address, not a wallet: withdraw to a wallet address.");
        }
        let prepared = match app.client().prepare_withdrawal(owner, amount, &to) {
            Ok(prepared) => prepared,
            Err(error) => return app.fail(describe(&error)),
        };
        leptos::task::spawn_local(async move {
            match wallet_send(app, prepared, shown).await {
                Ok(reply) => {
                    app.toast(
                        format!(
                            "Withdrawal of {} requested to {}",
                            fmt::usd(reply.data.amount),
                            reply.data.destination_token_account.short()
                        ),
                        true,
                    );
                    app.bump();
                }
                Err(error) => app.fail(error),
            }
        });
    };

    view! {
        <div class="card">
            <h1>"Funds"</h1>
            <div class="stats">
                <crate::ui::Stat label="Free collateral on Pascal" value=Signal::derive(move || match state.get() {
                    Some(Ok(state)) => fmt::usd(state.collateral_usd.unwrap_or(Dec::ZERO)),
                    _ => "-".to_owned(),
                }) />
                <crate::ui::Stat label="USDC in the wallet" value=Signal::derive(move || match balances.get() {
                    Some(Ok((amount, _))) => fmt::usd(usdc(amount)),
                    Some(Err(_)) => "unavailable".to_owned(),
                    None => "-".to_owned(),
                }) />
                <crate::ui::Stat label="SOL for fees" value=Signal::derive(move || match balances.get() {
                    Some(Ok((_, lamports))) => Dec::new(i128::from(lamports), 9).unwrap_or(Dec::ZERO).round(4, df_pascal_core::utils::Round::Down).to_string(),
                    _ => "-".to_owned(),
                }) />
            </div>
            {move || match balances.get() {
                Some(Err(error)) => view! { <p class="faint">{format!("The Solana node did not answer: {error}. Set another RPC in Settings.")}</p> }.into_any(),
                _ => ().into_any(),
            }}
        </div>
        <div class="grid halves">
            <div class="card">
                <h2>"Deposit USDC"</h2>
                {move || match address.get() {
                    None => view! { <p class="empty">"Loading"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                    Some(Ok(registered)) => match registered.deposit_address {
                        None => view! {
                            <p class="muted">"This account has no deposit address yet. The owner's wallet registers one; a referral code, if given, must resolve or registration fails."</p>
                            <input placeholder="Referral code (optional)" prop:value=move || referral.get() on:input=move |ev| referral.set(event_target_value(&ev)) />
                            <button class="primary" disabled=move || app.wallet.get().is_none() on:click=register>"Register deposit address"</button>
                        }.into_any(),
                        Some(deposit_address) => {
                            let account = associated_token_address(&deposit_address, &mint);
                            view! {
                                <p class="muted">"Send only USDC on Solana. The address is fixed for this account."</p>
                                <label class="field">"Deposit address"<code class="mono">{deposit_address.to_base58()}</code></label>
                                <label class="field">"Its USDC account"<code class="mono">{account.to_base58()}</code></label>
                                <div class="row">
                                    <input inputmode="decimal" placeholder="Amount in USDC" prop:value=move || deposit_amount.get() on:input=move |ev| deposit_amount.set(event_target_value(&ev)) />
                                    <button class="primary" disabled=move || app.wallet.get().is_none() on:click=move |_| deposit(deposit_address)>"Deposit from wallet"</button>
                                </div>
                                {move || deposit_status.get().map(|text| view! { <p class="faint">{text}</p> })}
                            }.into_any()
                        }
                    },
                }}
            </div>
            <div class="card">
                <h2>"Withdraw"</h2>
                <p class="muted">{format!("The amount leaves the account; the destination receives it less the withdrawal fee ({} today). The owner's wallet signs.", fmt::usd(WITHDRAWAL_FEE_USD))}</p>
                <label class="field">"Amount (USD)"<input inputmode="decimal" prop:value=move || withdraw_amount.get() on:input=move |ev| withdraw_amount.set(event_target_value(&ev)) /></label>
                <label class="field">"To wallet address"<input prop:value=move || destination.get() on:input=move |ev| destination.set(event_target_value(&ev)) /></label>
                <p class="faint">{move || {
                    let to = Pubkey::parse(destination.get().trim()).ok().map(|to| associated_token_address(&to, &mint).short()).unwrap_or_default();
                    let net = Dec::parse(withdraw_amount.get().trim()).ok().map(net_of_fee);
                    match net {
                        Some(net) => format!("Paid to USDC account {to}; arrives as {}", fmt::usd(net)),
                        None => format!("Paid to USDC account {to}"),
                    }
                }}</p>
                <button class="primary" disabled=move || app.wallet.get().is_none() on:click=withdraw>"Withdraw"</button>
            </div>
        </div>
        <div class="card">
            <h2>"Pending"</h2>
            {move || match state.get() {
                Some(Ok(state)) if state.pending_deposits.is_empty() && state.pending_withdrawals.is_empty() => view! { <p class="empty">"Nothing pending."</p> }.into_any(),
                Some(Ok(state)) => view! {
                    <div class="table-wrap"><table><tbody>
                        {state.pending_deposits.into_iter().map(|deposit| view! {
                            <tr><td>"Deposit"</td><td class="r num">{fmt::usd(deposit.amount)}</td><td>{deposit.transfer_status.to_string()}</td></tr>
                        }).collect_view()}
                        {state.pending_withdrawals.into_iter().map(|withdrawal| view! {
                            <tr><td>"Withdrawal"</td><td class="r num">{fmt::usd(withdrawal.amount)}</td><td>{format!("{} to {}", withdrawal.status, withdrawal.destination_authority.short())}</td></tr>
                        }).collect_view()}
                    </tbody></table></div>
                }.into_any(),
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                None => view! { <p class="empty">"Loading"</p> }.into_any(),
            }}
        </div>
        {move || shown.get().map(|message| view! {
            <div class="card"><h2>"Last message sent to the wallet"</h2><pre>{message}</pre></div>
        })}
        <Notice kind="">"Deposits need a little SOL in the wallet for the network fee, and about 0.002 SOL more the first time the deposit address's USDC account is created."</Notice>
    }
}

/// Sleeps for `ms` using setTimeout.
async fn gloo_sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
