//! Settings page: deployment, Solana RPC for deposits, client metadata byte and clock offset.

use leptos::prelude::*;

use crate::state::use_app;

#[component]
pub fn Settings() -> impl IntoView {
    let app = use_app();
    let rpc = RwSignal::new(app.rpc_url.get_untracked());
    let metadata = RwSignal::new(app.metadata.get_untracked().to_string());
    let offset = RwSignal::new(app.client().clock_offset_ms());
    let sync = move |_| {
        let client = app.client();
        leptos::task::spawn_local(async move {
            match client.sync_clock().await {
                Ok(ms) => offset.set(ms),
                Err(error) => app.fail(error),
            }
        });
    };
    let save_metadata = move |_| match metadata.get().trim().parse::<u8>() {
        Ok(value) => {
            app.set_metadata(value);
            app.toast("Saved", true);
        }
        Err(_) => app.fail("A byte is 0 to 255"),
    };
    view! {
        <div class="card">
            <h1>"Settings"</h1>
            <label class="field">"Deployment"
                <select on:change=move |ev| app.set_deployment(&event_target_value(&ev))>
                    <option value="prod" selected=move || app.deployment.get() == "prod">"Production (deployment 3)"</option>
                    <option value="canary" selected=move || app.deployment.get() == "canary">"Canary (deployment 2)"</option>
                </select>
            </label>
            <label class="field">"Solana RPC for deposits and balances"
                <div class="row">
                    <input style="flex:1" prop:value=move || rpc.get() on:input=move |ev| rpc.set(event_target_value(&ev)) />
                    <button on:click=move |_| { app.set_rpc(rpc.get().trim()); app.toast("RPC saved", true); }>"Save"</button>
                </div>
            </label>
            <p class="faint">"The default is PublicNode, which needs no key and answers browsers. Solana's own public endpoint refuses browser requests."</p>
            <label class="field">"Client byte on orders and cancels (0 sends none)"
                <div class="row">
                    <input inputmode="numeric" prop:value=move || metadata.get() on:input=move |ev| metadata.set(event_target_value(&ev)) />
                    <button on:click=save_metadata>"Save"</button>
                </div>
            </label>
            <div class="row">
                <span>{move || format!("Pascal's clock is {} ms ahead of this one.", offset.get())}</span>
                <button class="small" on:click=sync>"Check again"</button>
            </div>
        </div>
        <div class="card">
            <h2>"About"</h2>
            <p class="muted">"This app is the reference for df-pascal-core, the Rust SDK for Pascal. Everything here goes through the SDK: reads over fetch, the market and account channels over the browser websocket, trading requests signed in the page by the trading key, and wallet requests prepared by the SDK, signed by the wallet and checked before sending."</p>
            <p><a href="https://docs.pascal.trade" target="_blank" rel="noopener">"Pascal API docs"</a></p>
        </div>
    }
}
