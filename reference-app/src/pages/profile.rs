//! Profile page: username and bio (set with the trading key), referrals, leaderboard ranks, builder
//! totals, taker discount and trader search.

use df_pascal_core::Pubkey;
use df_pascal_core::account::{self, StateQuery, TraderSearch};
use leptos::prelude::*;

use crate::fmt;
use crate::state::{describe, use_app};
use crate::ui::{NeedAccount, Stat};

#[component]
pub fn Profile() -> impl IntoView {
    let app = use_app();
    view! {
        {move || match app.owner.get() {
            None => view! { <NeedAccount /> }.into_any(),
            Some(owner) => view! { <ProfileFor owner=owner /> }.into_any(),
        }}
        <Search />
    }
}

#[component]
fn ProfileFor(owner: Pubkey) -> impl IntoView {
    let app = use_app();
    let state = LocalResource::new(move || {
        let _ = app.revision.get();
        let client = app.client();
        async move {
            account::state(&*client, &owner, &StateQuery { exclude_orders: true, ..StateQuery::default() })
                .await
                .map(|r| r.data)
                .map_err(|e| describe(&e))
        }
    });
    let bio = LocalResource::new(move || {
        let _ = app.revision.get();
        let client = app.client();
        async move { account::bio(&*client, &owner).await.map(|r| r.data).map_err(|e| describe(&e)) }
    });
    let referrals = LocalResource::new(move || {
        let _ = app.revision.get();
        let client = app.client();
        async move { account::referrals(&*client, &owner).await.map(|r| r.data).map_err(|e| describe(&e)) }
    });
    let ranks = LocalResource::new(move || {
        let client = app.client();
        async move { account::ranks(&*client, &owner, None).await.map(|r| r.data).map_err(|e| describe(&e)) }
    });
    let builder = LocalResource::new(move || {
        let client = app.client();
        async move { account::builder_stats(&*client, &owner).await.map(|r| r.data).map_err(|e| describe(&e)) }
    });
    let discount = LocalResource::new(move || {
        let client = app.client();
        async move { account::taker_discount_terms(&*client, &owner).await.map(|r| r.data).map_err(|e| describe(&e)) }
    });

    let username = RwSignal::new(String::new());
    let bio_text = RwSignal::new(String::new());
    let custom = RwSignal::new(String::new());
    let apply = RwSignal::new(String::new());

    let run = move |what: &'static str| {
        let client = app.client();
        let (name, text, code, applied) = (username.get(), bio_text.get(), custom.get(), apply.get());
        leptos::task::spawn_local(async move {
            let result = match what {
                "username" => account::set_username(&*client, name.trim())
                    .await
                    .map(|r| format!("Username is now {}", r.data.username.unwrap_or_else(|| "cleared".to_owned()))),
                "bio" => account::set_bio(&*client, &text)
                    .await
                    .map(|r| format!("Bio is now {}", r.data.account_bio.unwrap_or_else(|| "cleared".to_owned()))),
                "generate" => account::generate_code(&*client)
                    .await
                    .map(|r| format!("Your referral code is {}", r.data.generated_referral_code)),
                "claim" => account::claim_code(&*client, code.trim())
                    .await
                    .map(|r| format!("Claimed {}", r.data.custom_referral_code)),
                _ => account::apply_code(&*client, applied.trim()).await.map(|_| "Referral code applied".to_owned()),
            };
            match result {
                Ok(text) => {
                    app.toast(text, true);
                    app.bump();
                }
                Err(error) => app.fail(describe(&error)),
            }
        });
    };
    let needs_key = move || !app.has_key();

    view! {
        <div class="grid halves">
            <div class="card">
                <h2>"Username and bio"</h2>
                <p class="muted">{move || match (state.get(), bio.get()) {
                    (Some(Ok(state)), Some(Ok(bio))) => format!("Username: {}. Bio: {}.", state.username.unwrap_or_else(|| "none".to_owned()), bio.unwrap_or_else(|| "none".to_owned())),
                    _ => "Loading".to_owned(),
                }}</p>
                <div class="row">
                    <input placeholder="3 to 20 letters, digits, _ or -" prop:value=move || username.get() on:input=move |ev| username.set(event_target_value(&ev)) />
                    <button disabled=needs_key on:click=move |_| run("username")>"Set username"</button>
                </div>
                <div class="row">
                    <input placeholder="Up to 80 characters" maxlength="80" prop:value=move || bio_text.get() on:input=move |ev| bio_text.set(event_target_value(&ev)) />
                    <button disabled=needs_key on:click=move |_| run("bio")>"Set bio"</button>
                </div>
                <p class="faint">"Leave a field empty to clear it. Both are signed by the trading key and need a funded account."</p>
            </div>
            <div class="card">
                <h2>"Ranks"</h2>
                {move || match ranks.get() {
                    Some(Ok(ranks)) => view! {
                        <div class="table-wrap"><table>
                            <thead><tr><th>"Window"</th><th class="r">"PnL"</th><th class="r">"PnL rank"</th><th class="r">"Volume"</th><th class="r">"Volume rank"</th></tr></thead>
                            <tbody>{ranks.windows.into_iter().map(|window| view! {
                                <tr>
                                    <td>{window.window.to_string()}</td>
                                    <td class="r num">{window.pnl.as_ref().map(|p| fmt::signed_usd(p.pnl_usd)).unwrap_or_default()}</td>
                                    <td class="r num">{window.pnl.as_ref().map(|p| p.rank.to_string()).unwrap_or_default()}</td>
                                    <td class="r num">{window.volume.as_ref().map(|v| fmt::count(v.volume_contracts)).unwrap_or_default()}</td>
                                    <td class="r num">{window.volume.as_ref().map(|v| v.rank.to_string()).unwrap_or_default()}</td>
                                </tr>
                            }).collect_view()}</tbody>
                        </table></div>
                    }.into_any(),
                    Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                    None => view! { <p class="empty">"Loading"</p> }.into_any(),
                }}
            </div>
        </div>
        <div class="card">
            <h2>"Referrals"</h2>
            {move || match referrals.get() {
                None => view! { <p class="empty">"Loading"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                Some(Ok(referrals)) => view! {
                    <div class="stats">
                        <Stat label="Your generated code" value=referrals.generated_referral_code.clone().unwrap_or_else(|| "-".to_owned()) />
                        <Stat label="Your custom code" value=referrals.custom_referral_code.clone().map(|code| if referrals.custom_referral_code_revoked { format!("{code} (revoked)") } else { code }).unwrap_or_else(|| "-".to_owned()) />
                        <Stat label="Referred traders" value=referrals.referral_count_total.to_string() />
                        <Stat label="Rewards paid" value=fmt::usd(referrals.reward_paid_total_usd) />
                        <Stat label="Your share" value=fmt::chance(referrals.referral_reward_terms.referral_reward_share) />
                        <Stat label="Referred by" value=referrals.referrer.as_ref().map(|r| r.username.clone().unwrap_or_else(|| r.owner.short())).unwrap_or_else(|| "-".to_owned()) />
                    </div>
                    <div class="row">
                        <button disabled=move || needs_key() || !referrals.can_generate_referral_code on:click=move |_| run("generate")>"Generate a code"</button>
                        <input placeholder="Custom code: 6 to 20 lowercase letters or digits" prop:value=move || custom.get() on:input=move |ev| custom.set(event_target_value(&ev)) />
                        <button disabled=move || needs_key() || !referrals.can_claim_custom_referral_code on:click=move |_| run("claim")>"Claim"</button>
                    </div>
                    <div class="row">
                        <input placeholder="Someone's referral code" prop:value=move || apply.get() on:input=move |ev| apply.set(event_target_value(&ev)) />
                        <button disabled=move || needs_key() || !referrals.can_apply_referral_code on:click=move |_| run("apply")>"Apply"</button>
                    </div>
                    {(!referrals.referrals.is_empty()).then(|| view! {
                        <div class="table-wrap"><table>
                            <thead><tr><th>"Trader"</th><th class="r">"Since"</th><th class="r">"Paid"</th><th class="r">"Limit"</th></tr></thead>
                            <tbody>{referrals.referrals.into_iter().map(|referral| view! {
                                <tr>
                                    <td class="mono">{referral.referred_owner.short()}</td>
                                    <td class="r">{fmt::date(referral.created_ts_ms)}</td>
                                    <td class="r num">{fmt::usd(referral.reward_paid_usd)}</td>
                                    <td class="r num">{fmt::usd(referral.reward_paid_limit_usd)}</td>
                                </tr>
                            }).collect_view()}</tbody>
                        </table></div>
                    })}
                }.into_any(),
            }}
        </div>
        <div class="grid halves">
            <div class="card">
                <h2>"As a builder"</h2>
                {move || match builder.get() {
                    Some(Ok(stats)) => view! {
                        <div class="stats">
                            <Stat label="Contracts routed" value=fmt::count(stats.builder_volume_contracts) />
                            <Stat label="Notional" value=fmt::usd(stats.builder_volume_notional_usd) />
                            <Stat label="Fees received" value=fmt::usd(stats.builder_fees_received_usd) />
                        </div>
                    }.into_any(),
                    Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                    None => view! { <p class="empty">"Loading"</p> }.into_any(),
                }}
            </div>
            <div class="card">
                <h2>"Taker discount"</h2>
                {move || match discount.get() {
                    Some(Ok(Some(terms))) => view! {
                        <div class="stats">
                            <Stat label="Discount" value=fmt::chance(terms.taker_fee_discount_share) />
                            <Stat label="Saved" value=fmt::usd(terms.fee_savings_usd) />
                            <Stat label="Up to" value=fmt::usd(terms.fee_savings_limit_usd) />
                        </div>
                    }.into_any(),
                    Some(Ok(None)) => view! { <p class="empty">"No taker discount."</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                    None => view! { <p class="empty">"Loading"</p> }.into_any(),
                }}
            </div>
        </div>
    }
}

#[component]
fn Search() -> impl IntoView {
    let app = use_app();
    let typed = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let found = LocalResource::new(move || {
        let text = query.get();
        let client = app.client();
        async move {
            if text.trim().is_empty() {
                return Ok(None);
            }
            let search = TraderSearch { query: text, limit: Some(10), ..TraderSearch::default() };
            account::search_traders(&*client, &search).await.map(|r| Some(r.data)).map_err(|e| describe(&e))
        }
    });
    view! {
        <div class="card">
            <h2>"Find a trader by X handle"</h2>
            <form class="row" on:submit=move |ev| { ev.prevent_default(); query.set(typed.get()); }>
                <input placeholder="@handle" prop:value=move || typed.get() on:input=move |ev| typed.set(event_target_value(&ev)) />
                <button type="submit">"Search"</button>
            </form>
            {move || match found.get() {
                Some(Ok(Some(results))) if results.results.is_empty() => view! { <p class="empty">"No trader has that handle."</p> }.into_any(),
                Some(Ok(Some(results))) => view! {
                    <div class="table-wrap"><table><tbody>
                        {results.results.into_iter().map(|trader| {
                            let owner = trader.owner;
                            let handle = trader.display_info.twitter_account_connection.as_ref().map(|x| format!("@{}", x.twitter_handle)).unwrap_or_default();
                            view! {
                                <tr>
                                    <td>{handle}{trader.typo_corrected.then(|| view! { <span class="badge">"close match"</span> })}</td>
                                    <td class="mono">{owner.short()}</td>
                                    <td class="r"><button class="small" on:click=move |_| app.set_owner(Some(owner), None)>"Watch"</button></td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody></table></div>
                }.into_any(),
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                _ => ().into_any(),
            }}
        </div>
    }
}
