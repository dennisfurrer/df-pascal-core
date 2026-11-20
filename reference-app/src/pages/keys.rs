//! Trading keys page: creates a key in the browser and has the wallet authorize it (a trading key
//! can place and cancel orders but cannot withdraw).

use df_pascal_core::account::{self, TradingKey};
use df_pascal_core::client::Prepared;
use df_pascal_core::client::signed::Write;
use df_pascal_core::{Keypair, Pubkey};
use leptos::prelude::*;

use crate::fmt;
use crate::state::{AppState, describe, use_app};
use crate::ui::{NeedAccount, Notice};
use crate::wallet;

/// Shows the prepared message, has the wallet sign it, then sends it.
pub async fn wallet_send<B: Write + Clone>(
    app: AppState,
    prepared: Prepared<B>,
    shown: RwSignal<Option<String>>,
) -> Result<df_pascal_core::client::Reply<B::Reply>, String> {
    if app.wallet.get_untracked().is_none() {
        return Err("This needs the owner's wallet: connect it first.".to_owned());
    }
    shown.set(prepared.message_text().map(str::to_owned));
    let signature = wallet::sign_message(prepared.message()).await?;
    let signed = prepared
        .with_signature(signature)
        .map_err(|error| format!("The wallet's signature did not verify: {error}"))?;
    app.client().send(&signed).await.map_err(|error| describe(&error))
}

#[component]
pub fn Keys() -> impl IntoView {
    let app = use_app();
    move || match app.owner.get() {
        None => view! { <NeedAccount /> }.into_any(),
        Some(owner) => view! { <KeysFor owner=owner /> }.into_any(),
    }
}

#[component]
fn KeysFor(owner: Pubkey) -> impl IntoView {
    let app = use_app();
    let keys = LocalResource::new(move || {
        let _ = app.revision.get();
        let client = app.client();
        async move {
            account::trading_keys(&*client, &owner).await.map(|reply| reply.data).map_err(|error| describe(&error))
        }
    });
    let shown = RwSignal::new(None::<String>);
    let name = RwSignal::new("df-pascal-web".to_owned());
    let days = RwSignal::new("30".to_owned());
    let remember = RwSignal::new(true);
    let secret = RwSignal::new(String::new());
    let busy = RwSignal::new(false);

    let create = move |_| {
        let days: u64 = match days.get().trim().parse() {
            Ok(days) => days,
            Err(_) => return app.fail("Days is a whole number from 1 to 365"),
        };
        let key = match Keypair::generate() {
            Ok(key) => key,
            Err(error) => return app.fail(error),
        };
        let prepared = match app.client().prepare_trading_key(owner, &key.pubkey(), name.get().trim(), days) {
            Ok(prepared) => prepared,
            Err(error) => return app.fail(describe(&error)),
        };
        busy.set(true);
        let keep = remember.get();
        leptos::task::spawn_local(async move {
            match wallet_send(app, prepared, shown).await {
                Ok(reply) => {
                    app.set_key(Some(key), keep);
                    app.toast(
                        format!(
                            "Trading key {} authorized until {}",
                            reply.data.trading_key.short(),
                            fmt::date(reply.data.expiration_ts_ms)
                        ),
                        true,
                    );
                    app.bump();
                }
                Err(error) => app.fail(error),
            }
            busy.set(false);
        });
    };

    let load_key = move |_| {
        let key = match Keypair::parse(secret.get().trim()) {
            Ok(key) => key,
            Err(error) => return app.fail(error),
        };
        secret.set(String::new());
        let keep = remember.get();
        let client = app.client();
        leptos::task::spawn_local(async move {
            match account::is_active(&*client, &owner, &key.pubkey()).await {
                Ok(true) => {
                    app.set_key(Some(key), keep);
                    app.toast("Trading key loaded", true);
                }
                Ok(false) => {
                    app.fail(format!("{} is not an active trading key for this account", key.pubkey().short()))
                }
                Err(error) => app.fail(describe(&error)),
            }
        });
    };

    let revoke = move |key: Pubkey| {
        let prepared = match app.client().prepare_revoke_trading_key(owner, &key) {
            Ok(prepared) => prepared,
            Err(error) => return app.fail(describe(&error)),
        };
        leptos::task::spawn_local(async move {
            match wallet_send(app, prepared, shown).await {
                Ok(_) => {
                    if app.trading.get_untracked() == Some(key) {
                        app.set_key(None, false);
                    }
                    app.toast(format!("Revoked {}", key.short()), true);
                    app.bump();
                }
                Err(error) => app.fail(error),
            }
        });
    };

    view! {
        <div class="card">
            <h1>"Trading keys"</h1>
            <p class="muted">"A trading key signs orders, cancels, collateral returns, referral codes, the username and the bio. It cannot move money. The owner's wallet authorizes and revokes it, and signs withdrawals."</p>
            {move || match app.trading.get() {
                Some(key) => view! {
                    <Notice kind="ok">
                        {format!("This browser holds trading key {}.", key.to_base58())}
                        " "
                        <button class="small" on:click=move |_| app.set_key(None, false)>"Forget it here"</button>
                    </Notice>
                }.into_any(),
                None => view! { <Notice kind="warn">"This browser holds no trading key for this account. Create one or load one below."</Notice> }.into_any(),
            }}
        </div>
        <div class="card">
            <h2>"Active keys"</h2>
            {move || match keys.get() {
                None => view! { <p class="empty">"Loading"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                Some(Ok(keys)) if keys.is_empty() => view! { <p class="empty">"No active trading keys."</p> }.into_any(),
                Some(Ok(keys)) => {
                    let now = app.client().now_ms();
                    view! {
                        <div class="table-wrap"><table>
                            <thead><tr><th>"Name"</th><th>"Key"</th><th class="r">"Open orders"</th><th class="r">"Expires"</th><th></th></tr></thead>
                            <tbody>{keys.into_iter().map(|key: TradingKey| {
                                let public = key.trading_key;
                                let here = app.trading.get_untracked() == Some(public);
                                view! {
                                    <tr>
                                        <td>{key.name.clone()}{here.then(|| view! { <span class="badge long">"this browser"</span> })}</td>
                                        <td class="mono">{public.short()}</td>
                                        <td class="r num">{key.open_order_count}</td>
                                        <td class=if key.is_expired(now) { "r short" } else { "r" }>{fmt::date(key.expiration_ts_ms)}</td>
                                        <td class="r"><button class="small" disabled=move || app.wallet.get().is_none() on:click=move |_| revoke(public)>"Revoke"</button></td>
                                    </tr>
                                }
                            }).collect_view()}</tbody>
                        </table></div>
                    }.into_any()
                }
            }}
        </div>
        <div class="grid halves">
            <div class="card">
                <h2>"Create a trading key"</h2>
                <p class="muted">"A new key is made in this browser. The wallet signs one message to authorize it. The account must be funded; a name is at most 16 bytes and public on chain."</p>
                <div class="field-row">
                    <label class="field">"Name"<input prop:value=move || name.get() on:input=move |ev| name.set(event_target_value(&ev)) /></label>
                    <label class="field">"Valid for days"<input inputmode="numeric" prop:value=move || days.get() on:input=move |ev| days.set(event_target_value(&ev)) /></label>
                </div>
                <label class="check"><input type="checkbox" prop:checked=move || remember.get() on:change=move |ev| remember.set(event_target_checked(&ev)) />"Keep it in this browser's storage"</label>
                <button class="primary" disabled=move || busy.get() || app.wallet.get().is_none() on:click=create>
                    {move || if app.wallet.get().is_none() { "Connect the owner's wallet first" } else if busy.get() { "Waiting for the wallet" } else { "Create and authorize" }}
                </button>
            </div>
            <div class="card">
                <h2>"Load an existing trading key"</h2>
                <p class="muted">"Paste a trading key's private key (base58, hex, or a JSON byte array). It is checked against the account's active keys before it is used. Never paste the owner wallet's key."</p>
                <input type="password" placeholder="Trading private key" prop:value=move || secret.get() on:input=move |ev| secret.set(event_target_value(&ev)) />
                <button on:click=load_key>"Load"</button>
            </div>
        </div>
        {move || shown.get().map(|message| view! {
            <div class="card">
                <h2>"Last message sent to the wallet"</h2>
                <pre>{message}</pre>
            </div>
        })}
    }
}
