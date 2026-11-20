//! Logo, market rail and status bar.

use std::time::Duration;

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_location;

use crate::events::{self, EventGroup};
use crate::fmt;
use crate::icons::{Glyph, Icon};
use crate::state::{MarketList, use_app, use_market_list};

const REPO: &str = "https://github.com/dennisfurrer/df-pascal-core";
const CRATE: &str = "https://crates.io/crates/df-pascal-core";

/// GitHub's mark, from Primer Octicons (mark-github-16, MIT License, Copyright (c) GitHub Inc.).
const GITHUB_MARK: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M6.766 11.328c-2.063-.25-3.516-1.734-3.516-3.656 0-.781.281-1.625.75-2.188-.203-.515-.172-1.609.063-2.062.625-.078 1.468.25 1.968.703.594-.187 1.219-.281 1.985-.281.765 0 1.39.094 1.953.265.484-.437 1.344-.765 1.969-.687.218.422.25 1.515.046 2.047.5.593.766 1.39.766 2.203 0 1.922-1.453 3.375-3.547 3.64.531.344.89 1.094.89 1.954v1.625c0 .468.391.734.86.547C13.781 14.359 16 11.53 16 8.03 16 3.61 12.406 0 7.984 0 3.563 0 0 3.61 0 8.031a7.88 7.88 0 0 0 5.172 7.422c.422.156.828-.125.828-.547v-1.25c-.219.094-.5.156-.75.156-1.031 0-1.64-.562-2.078-1.609-.172-.422-.36-.672-.719-.719-.187-.015-.25-.093-.25-.187 0-.188.313-.328.625-.328.453 0 .844.281 1.25.86.313.452.64.655 1.031.655s.641-.14 1-.5c.266-.265.47-.5.657-.656"/></svg>"#;

const MARK: &str = r##"<svg aria-hidden="true" fill="none" viewBox="0 0 200 200"><defs><radialGradient cx="0" cy="0" gradientTransform="translate(100 169) rotate(-90) scale(93 66.4445)" gradientUnits="userSpaceOnUse" id="pascal-leg" r="1"><stop stop-color="#DB9C67"/><stop offset="0.977379" stop-color="#D35340"/></radialGradient><radialGradient cx="0" cy="0" gradientTransform="translate(200 54) rotate(180) scale(100 179.988)" gradientUnits="userSpaceOnUse" id="pascal-top" r="1"><stop stop-color="#DB9C67"/><stop offset="0.977379" stop-color="#D35340"/></radialGradient><radialGradient cx="0" cy="0" gradientTransform="translate(100 169) rotate(-90) scale(90 189.622)" gradientUnits="userSpaceOnUse" id="pascal-right" r="1"><stop stop-color="#DB9C67"/><stop offset="0.977379" stop-color="#D35340"/></radialGradient></defs><path d="M140 54.0932L100.003 31L60 54.0932L100.003 77.1923L140 54.0932Z" fill="url(#pascal-top)"/><path d="M100.003 123.01L100.003 123.379L140 100.286L140 54.0933L100.003 77.1923V123.01Z" fill="url(#pascal-right)"/><path d="M100.003 123.01L60 99.9165V146.103L100.003 169.202V123.01Z" fill="url(#pascal-leg)"/></svg>"##;

/// Pascal logo: three faces of a cube with a copper gradient.
#[component]
pub fn Logo() -> impl IntoView {
    view! { <span class="logo" inner_html=MARK></span> }
}

const RAIL_EVENTS: usize = 60;
const RAIL_OUTCOMES: usize = 3;

/// Live events by 24-hour volume with their leading outcomes, searchable. Shown on wide screens,
/// except on the markets page.
#[component]
pub fn Rail() -> impl IntoView {
    let Some(MarketList(list)) = use_market_list() else { return ().into_any() };
    let location = use_location();
    let query = RwSignal::new(String::new());
    let groups = move || -> Option<Result<Vec<EventGroup>, String>> {
        let found = list.get()?;
        Some(found.map(|mut markets| {
            let wanted = query.get().trim().to_lowercase();
            if !wanted.is_empty() {
                markets.retain(|market| {
                    market.title().to_lowercase().contains(&wanted) || market.symbol.to_lowercase().contains(&wanted)
                });
            }
            let mut groups = events::group(markets);
            groups.truncate(RAIL_EVENTS);
            groups
        }))
    };
    let count = move || list.get().and_then(Result::ok).map(|markets| markets.len());
    view! {
        <aside class="rail" aria-label="Markets" hidden=move || location.pathname.get() == "/">
            <div class="rail-head">
                <span class="rail-title">"Markets"</span>
                <span class="faint num">{move || count().map(|count| count.to_string()).unwrap_or_default()}</span>
                <A href="/" attr:class="rail-all">"All markets"</A>
            </div>
            <div class="rail-search">
                <input placeholder="Search markets" prop:value=move || query.get() on:input=move |ev| query.set(event_target_value(&ev)) />
            </div>
            <div class="rail-list">
                {move || match groups() {
                    None => view! { <p class="empty rail-note">"Loading markets"</p> }.into_any(),
                    Some(Err(error)) => view! { <p class="notice error rail-note">{error}</p> }.into_any(),
                    Some(Ok(groups)) if groups.is_empty() => view! { <p class="empty rail-note">"No market matches."</p> }.into_any(),
                    Some(Ok(groups)) => groups.into_iter().map(|group| {
                        let more = group.markets.len().saturating_sub(RAIL_OUTCOMES);
                        let first = group.markets.first().map(|market| format!("/market/{}", market.symbol)).unwrap_or_default();
                        let codes: Vec<String> = group.markets.iter().map(|market| format!("/market/{}", market.symbol)).collect();
                        let inside = move || codes.contains(&location.pathname.get()).then_some("true");
                        let (title, hover) = (group.title.clone(), group.title.clone());
                        view! {
                            <div class="rail-event" data-current=inside>
                                <A href=first attr:class="rail-event-title" attr:title=hover>{title}</A>
                                {group.markets.into_iter().take(RAIL_OUTCOMES).map(|market| {
                                    let href = format!("/market/{}", market.symbol);
                                    let path = href.clone();
                                    let current = move || (location.pathname.get() == path).then_some("true");
                                    let chance = events::chance(&market);
                                    let width = format!("width:{}%", events::percent(chance));
                                    view! {
                                        <A href=href attr:class="rail-row" attr:aria-current=current>
                                            <span class="rail-name">{events::outcome_name(&market)}</span>
                                            <span class="num rail-chance">{chance.map(fmt::chance).unwrap_or_else(|| "-".to_owned())}</span>
                                            <span class="rail-bar"><i style=width></i></span>
                                        </A>
                                    }
                                }).collect_view()}
                                <span class="rail-event-foot faint num">
                                    {format!("{} 24h", fmt::usd(group.volume))}
                                    {(more > 0).then(|| view! { <span>{format!("{more} more")}</span> })}
                                </span>
                            </div>
                        }
                    }).collect_view().into_any(),
                }}
            </div>
        </aside>
    }
    .into_any()
}

/// Bottom bar: deployment, clock offset, account, trading key and market count.
#[component]
pub fn StatusBar() -> impl IntoView {
    let app = use_app();
    let list = use_market_list();
    let offset = RwSignal::new(0_i64);
    let read = move || offset.set(app.client().clock_offset_ms());
    read();
    if let Ok(handle) = set_interval_with_handle(read, Duration::from_secs(5)) {
        on_cleanup(move || handle.clear());
    }
    let markets = move || {
        list.and_then(|MarketList(list)| list.get()).map(|found| match found {
            Ok(markets) => format!("{} live markets", markets.len()),
            Err(_) => "Market list unavailable".to_owned(),
        })
    };
    view! {
        <footer class="status">
            <span class="status-live">
                <i class=move || if markets().is_some_and(|text| text.ends_with("markets")) { "dot" } else { "dot idle" }></i>
                {move || {
                    let deployment = app.deployment_config();
                    format!("{}, {}", deployment.name, deployment.read_url.trim_start_matches("https://"))
                }}
            </span>
            <span class="num">{move || markets().unwrap_or_else(|| "Loading markets".to_owned())}</span>
            <span class="num hide-narrow-flex">{move || format!("Clock offset {} ms", offset.get())}</span>
            <span class="push">{move || match app.owner.get() {
                Some(owner) => format!("Account {}", owner.short()),
                None => "No account".to_owned(),
            }}</span>
            <span class="hide-narrow-flex">{move || if app.trading.get().is_some() { "Trading key held" } else { "No trading key" }}</span>
            <span class="hide-narrow-flex">"df-pascal-core"</span>
            <span class="status-links">
                <a href=REPO target="_blank" rel="noopener" title="Source on GitHub" aria-label="Source on GitHub">
                    <span class="glyph" inner_html=GITHUB_MARK></span>
                </a>
                <a href=CRATE target="_blank" rel="noopener" title="df-pascal-core on crates.io" aria-label="df-pascal-core on crates.io">
                    <Glyph icon=Icon::Package size=12 />
                </a>
            </span>
        </footer>
    }
}
