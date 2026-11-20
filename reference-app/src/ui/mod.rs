//! Components shared by the pages: stat, line chart, notices, toasts and wallet connect.

use df_pascal_core::Dec;
use df_pascal_core::utils::Round;
use leptos::prelude::*;

use crate::state::use_app;
use crate::wallet;

mod shell;

pub use shell::{Logo, Rail, StatusBar};

#[component]
pub fn Stat(#[prop(into)] label: String, #[prop(into)] value: Signal<String>) -> impl IntoView {
    view! {
        <div class="stat">
            <span class="label">{label}</span>
            <span class="value">{move || value.get()}</span>
        </div>
    }
}

const FILL: &str = r##"<linearGradient id="chart-fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ec7a4f" stop-opacity="0.34"/><stop offset="1" stop-color="#ec7a4f" stop-opacity="0"/></linearGradient>"##;

/// Line chart of `values`, oldest first, scaled to their min and max, which are shown as labels.
#[component]
pub fn Chart(
    #[prop(into)] values: Signal<Vec<Dec>>,
    #[prop(optional)] label: Option<fn(Dec) -> String>,
) -> impl IntoView {
    const WIDTH: i128 = 600;
    const HEIGHT: i128 = 170;
    const TOP: i128 = 10;
    const BOTTOM: i128 = 150;
    let shape = move || {
        let values = values.get();
        if values.len() < 2 {
            return None;
        }
        let units: Vec<i128> = values.iter().map(|value| value.units_at(6, Round::Nearest).unwrap_or(0)).collect();
        let low = units.iter().copied().min().unwrap_or(0);
        let high = units.iter().copied().max().unwrap_or(0);
        let span = (high - low).max(1);
        let last = i128::try_from(units.len() - 1).unwrap_or(1).max(1);
        let points: Vec<String> = units
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let x = i128::try_from(index).unwrap_or(0) * WIDTH / last;
                let y = BOTTOM - (value - low) * (BOTTOM - TOP) / span;
                format!("{x},{y}")
            })
            .collect();
        let line = points.join(" ");
        let area = format!("0,{BOTTOM} {line} {WIDTH},{BOTTOM}");
        let show = label.unwrap_or(|value: Dec| value.to_string());
        let low = show(Dec::new(low, 6).unwrap_or(Dec::ZERO));
        let high = show(Dec::new(high, 6).unwrap_or(Dec::ZERO));
        Some((line, area, low, high))
    };
    view! {
        <div class="chart-wrap">
            <svg class="chart" viewBox=format!("0 0 {WIDTH} {HEIGHT}") preserveAspectRatio="none" role="img">
                <defs inner_html=FILL></defs>
                {[45, 80, 115].map(|y| view! {
                    <line class="grid-line" x1="0" x2=WIDTH y1=y y2=y vector-effect="non-scaling-stroke" />
                })}
                {move || shape().map(|(line, area, _, _)| view! {
                    <polygon class="area" points=area />
                    <polyline class="line" points=line vector-effect="non-scaling-stroke" />
                })}
            </svg>
            {move || shape().map(|(_, _, low, high)| view! {
                <span class="high">{high}</span>
                <span class="low">{low}</span>
            })}
        </div>
        <Show when=move || values.with(|values| values.len() < 2)>
            <p class="empty">"Not enough points to draw."</p>
        </Show>
    }
}

#[component]
pub fn Notice(#[prop(into)] kind: String, children: Children) -> impl IntoView {
    view! { <div class=format!("notice {kind}")>{children()}</div> }
}

#[component]
pub fn Toasts() -> impl IntoView {
    let app = use_app();
    view! {
        <div class="toasts" aria-live="polite">
            <For each=move || app.toasts.get() key=|(id, _, _)| *id let:toast>
                <div class=if toast.2 { "toast ok" } else { "toast error" }>{toast.1}</div>
            </For>
        </div>
    }
}

/// Connect wallet button, or the connected or watched account with a disconnect button.
#[component]
pub fn Connect() -> impl IntoView {
    let app = use_app();
    let busy = RwSignal::new(false);
    let connect = move |_| {
        busy.set(true);
        leptos::task::spawn_local(async move {
            match wallet::connect(wallet::list().into_iter().next()).await {
                Ok((owner, name)) => {
                    app.set_owner(Some(owner), Some(name.clone()));
                    app.toast(format!("Connected {name}"), true);
                }
                Err(error) => app.fail(error),
            }
            busy.set(false);
        });
    };
    let disconnect = move |_| {
        leptos::task::spawn_local(async move {
            wallet::disconnect().await;
            app.set_owner(None, None);
        });
    };
    view! {
        {move || match (app.owner.get(), app.wallet.get()) {
            (Some(owner), Some(name)) => view! {
                <span class="badge" title=owner.to_base58()>{format!("{name} {}", owner.short())}</span>
                <button class="small ghost" on:click=disconnect>"Disconnect"</button>
            }.into_any(),
            (Some(owner), None) => view! {
                <span class="badge" title=owner.to_base58()>{format!("Watching {}", owner.short())}</span>
                <button class="small" disabled=move || busy.get() on:click=connect>"Connect wallet"</button>
                <button class="small ghost" on:click=disconnect>"Stop"</button>
            }.into_any(),
            _ => view! {
                <button class="primary small" disabled=move || busy.get() on:click=connect>
                    {move || if busy.get() { "Connecting" } else { "Connect wallet" }}
                </button>
            }.into_any(),
        }}
    }
}

/// Shown when no account is set: connect a wallet or enter an address to watch.
#[component]
pub fn NeedAccount() -> impl IntoView {
    let app = use_app();
    let address = RwSignal::new(String::new());
    let watch = move |_| match df_pascal_core::Pubkey::parse(address.get().trim()) {
        Ok(owner) => app.set_owner(Some(owner), None),
        Err(error) => app.fail(error),
    };
    view! {
        <div class="card">
            <h2>"No account"</h2>
            <p class="muted">"Connect a Solana wallet to trade, or enter an owner address to watch it read-only."</p>
            <div class="row">
                <Connect />
            </div>
            <div class="row">
                <input placeholder="Owner address (base58)" style="flex:1" prop:value=move || address.get()
                    on:input=move |ev| address.set(event_target_value(&ev)) />
                <button on:click=watch>"Watch"</button>
            </div>
        </div>
    }
}
