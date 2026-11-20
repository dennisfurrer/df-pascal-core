//! Markets page: live markets grouped by event or as a table, with tag filter and search, and markets
//! resolved in the last day.

use std::collections::BTreeMap;

use df_pascal_core::Dec;
use df_pascal_core::markets::{self, Market, ResolvedQuery, TextSearch};
use df_pascal_core::utils::{Lookback, MarketStatus};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;

use crate::events::{self, EventGroup};
use crate::fmt;
use crate::icons::{Glyph, Icon};
use crate::state::{MarketList, describe, use_app, use_market_list};

const SHOWN: usize = 200;

#[component]
pub fn Markets() -> impl IntoView {
    let app = use_app();
    let tab = RwSignal::new("live");
    let tag = RwSignal::new(None::<String>);
    let typed = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());

    let Some(MarketList(live)) = use_market_list() else { return ().into_any() };
    let resolved = LocalResource::new(move || {
        let wanted = tab.get() == "resolved";
        let client = app.client();
        async move {
            if !wanted {
                return Ok(Vec::new());
            }
            let query = ResolvedQuery { lookback: Some(Lookback::H24), limit: Some(100), ..ResolvedQuery::default() };
            markets::resolved(&*client, &query).await.map(|reply| reply.data).map_err(|error| describe(&error))
        }
    });
    let searched = LocalResource::new(move || {
        let text = query.get();
        let client = app.client();
        async move {
            if text.trim().is_empty() {
                return Ok(None);
            }
            let search =
                TextSearch { status: Some(MarketStatus::All), limit: Some(100), ..TextSearch::new(text.trim()) };
            markets::text_search(&*client, &search)
                .await
                .map(|reply| Some(reply.data))
                .map_err(|error| describe(&error))
        }
    });

    let tags = move || {
        let Some(Ok(markets)) = live.get() else { return Vec::new() };
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for market in &markets {
            for tag in &market.display_attributes.tags {
                *counts.entry(tag.clone()).or_default() += 1;
            }
        }
        let mut tags: Vec<(String, usize)> = counts.into_iter().collect();
        tags.sort_by(|a, b| b.1.cmp(&a.1));
        tags.truncate(16);
        tags
    };
    let totals = move || {
        let Some(Ok(markets)) = live.get() else { return None };
        let volume =
            markets.iter().fold(Dec::ZERO, |sum, market| sum.checked_add(market.notional_24h()).unwrap_or(sum));
        let interest: u64 = markets.iter().map(|market| market.long_interest.unwrap_or(market.open_interest)).sum();
        let groups = events::group(markets.clone());
        let busiest = groups.first().map(|group| group.title.clone()).unwrap_or_default();
        Some((volume, markets.len(), groups.len(), interest, busiest))
    };

    let layout = RwSignal::new("events");
    let sort = RwSignal::new("volume");
    let rows = move || -> Result<Vec<Market>, String> {
        if let Some(found) = searched.get().transpose()?.flatten() {
            return Ok(found);
        }
        let mut markets = match tab.get() {
            "resolved" => resolved.get().unwrap_or(Ok(Vec::new()))?,
            _ => live.get().unwrap_or(Ok(Vec::new()))?,
        };
        if let Some(tag) = tag.get() {
            markets.retain(|market| market.display_attributes.tags.contains(&tag));
        }
        markets.sort_by_key(|market| std::cmp::Reverse(market.notional_24h()));
        Ok(markets)
    };
    let groups = move |markets: Vec<Market>| {
        let now = app.client().now_ms();
        let mut groups = events::group(markets);
        match sort.get() {
            "starting" => {
                groups.retain(|group| group.starts_ms.is_some_and(|starts| starts > now));
                groups.sort_by_key(|group| group.starts_ms);
            }
            "closing" => groups.sort_by_key(|group| group.resolves_ms.unwrap_or(u64::MAX)),
            _ => {}
        }
        groups.truncate(EVENTS_SHOWN);
        groups
    };

    view! {
        <div class="markets-page">
            <header class="hero">
                <div class="hero-text">
                    <h1>"Markets"</h1>
                    <p class="muted">"Prediction markets on Pascal: every outcome trades from $0 to $1 and pays $1 if it happens."</p>
                </div>
                <div class="hero-stats">
                    {move || totals().map(|(volume, count, events, interest, busiest)| view! {
                        <div class="tile-stat">
                            <span class="tile"><Glyph icon=Icon::ChartLine /></span>
                            <span class="tile-text"><span class="label">"24h volume"</span><span class="value">{fmt::usd_short(volume)}</span></span>
                        </div>
                        <div class="tile-stat">
                            <span class="tile"><Glyph icon=Icon::Layers /></span>
                            <span class="tile-text"><span class="label">"Live markets"</span><span class="value">{format!("{} in {} events", fmt::count(count), fmt::count(events))}</span></span>
                        </div>
                        <div class="tile-stat">
                            <span class="tile"><Glyph icon=Icon::Users /></span>
                            <span class="tile-text"><span class="label">"Contracts held"</span><span class="value">{fmt::count(interest)}</span></span>
                        </div>
                        <div class="tile-stat wide">
                            <span class="tile"><Glyph icon=Icon::Flame /></span>
                            <span class="tile-text"><span class="label">"Busiest event"</span><span class="value clip">{busiest}</span></span>
                        </div>
                    })}
                </div>
            </header>
            <div class="toolbar">
                <form class="search-field" on:submit=move |ev| { ev.prevent_default(); query.set(typed.get()); }>
                    <Glyph icon=Icon::Search />
                    <input placeholder="Search markets, teams, people" prop:value=move || typed.get()
                        on:input=move |ev| typed.set(event_target_value(&ev)) />
                    <Show when=move || !query.get().is_empty()>
                        <button type="button" class="ghost small" on:click=move |_| { typed.set(String::new()); query.set(String::new()); }>"Clear"</button>
                    </Show>
                </form>
                <div class="tabs">
                    <button aria-pressed=move || (tab.get() == "live").to_string() on:click=move |_| tab.set("live")>"Live"</button>
                    <button aria-pressed=move || (tab.get() == "resolved").to_string() on:click=move |_| tab.set("resolved")>"Resolved today"</button>
                </div>
                <div class="tabs">
                    <button aria-pressed=move || (sort.get() == "volume").to_string() on:click=move |_| sort.set("volume")>"Most traded"</button>
                    <button aria-pressed=move || (sort.get() == "starting").to_string() on:click=move |_| sort.set("starting")>"Starting soon"</button>
                    <button aria-pressed=move || (sort.get() == "closing").to_string() on:click=move |_| sort.set("closing")>"Resolving soon"</button>
                </div>
                <div class="tabs">
                    <button aria-pressed=move || (layout.get() == "events").to_string() on:click=move |_| layout.set("events")>"Cards"</button>
                    <button aria-pressed=move || (layout.get() == "table").to_string() on:click=move |_| layout.set("table")>"Table"</button>
                </div>
            </div>
            <div class="chips">
                <button class="chip" aria-pressed=move || tag.get().is_none().to_string() on:click=move |_| tag.set(None)>
                    <Glyph icon=Icon::Sparkles size=14 />"All"
                </button>
                <For each=tags key=|(tag, _)| tag.clone() let:item>
                    {
                        let (name, count) = item;
                        let value = name.clone();
                        let pressed = name.clone();
                        view! {
                            <button class="chip" aria-pressed=move || (tag.get().as_deref() == Some(pressed.as_str())).to_string()
                                on:click=move |_| tag.set(Some(value.clone()))>
                                <Glyph icon=events::tag_icon(&name) size=14 />
                                {name.clone()}
                                <span class="chip-count">{fmt::count(count)}</span>
                            </button>
                        }
                    }
                </For>
            </div>
            {move || match (live.get(), rows()) {
                (None, _) => view! { <p class="empty">"Loading markets"</p> }.into_any(),
                (_, Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                (_, Ok(rows)) if rows.is_empty() => view! { <p class="empty">"No markets."</p> }.into_any(),
                (_, Ok(rows)) if layout.get() == "table" => {
                    let mut rows = rows;
                    rows.truncate(SHOWN);
                    view! { <div class="card flush"><MarketTable rows=rows /></div> }.into_any()
                }
                (_, Ok(rows)) => {
                    let groups = groups(rows);
                    if groups.is_empty() {
                        return view! { <p class="empty">"No events match."</p> }.into_any();
                    }
                    let feature = sort.get() == "volume" && tag.get().is_none() && query.get().is_empty();
                    let now = app.client().now_ms();
                    view! {
                        <div class="event-grid">
                            {groups.into_iter().enumerate().map(|(index, group)| view! {
                                <EventCard group=group featured=feature && index < 2 now=now />
                            }).collect_view()}
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
    .into_any()
}

const EVENTS_SHOWN: usize = 120;

/// One event or game: its category, time, outcomes with chances, volume and other lines.
#[component]
pub fn EventCard(group: EventGroup, featured: bool, now: u64) -> impl IntoView {
    let shown = if featured { 6 } else { 4 };
    let first = group.markets.first().map(|market| format!("/market/{}", market.symbol)).unwrap_or_default();
    let resolved = group.markets.iter().all(Market::is_resolved);
    let when = match (resolved, group.starts_ms, group.resolves_ms) {
        (true, _, _) => "Resolved".to_owned(),
        (_, Some(starts), _) if starts > now => format!("Starts {}", fmt::moment(starts, now)),
        (_, Some(starts), _) => format!("Started {}", fmt::moment(starts, now)),
        (_, None, Some(ends)) => format!("Resolves {}", fmt::moment(ends, now)),
        _ => String::new(),
    };
    let more = group.markets.len().saturating_sub(shown);
    let lines = match (more, group.other_lines) {
        (0, 0) if group.all == 1 => "1 market".to_owned(),
        (0, 0) => format!("{} markets", group.all),
        (0, other) => format!("{other} more in other lines"),
        (more, 0) => format!("{more} more outcomes"),
        (more, other) => format!("{more} more outcomes, {other} in other lines"),
    };
    let matchup = group.teams.is_some() && group.markets.len() <= 3;
    let teams = group.teams.clone();
    let outcomes = group.markets.into_iter().take(shown).map(|market| {
        let chance = events::chance(&market);
        let (short, colour) = events::badge(&market, teams.as_ref());
        let href = format!("/market/{}", market.symbol);
        let name = events::outcome_name(&market);
        let shown = chance.map(fmt::chance).unwrap_or_else(|| "-".to_owned());
        let style = format!("--c:{colour}");
        let width = format!("width:{}%", events::percent(chance));
        if matchup {
            view! {
                <A href=href attr:class="side" attr:style=style>
                    <span class="badge-mark large">{short}</span>
                    <span class="side-name">{name}</span>
                    <span class="side-chance num">{shown}</span>
                    <span class="meter"><i style=width></i></span>
                </A>
            }
            .into_any()
        } else {
            view! {
                <A href=href attr:class="outcome" attr:style=style>
                    <span class="badge-mark">{short}</span>
                    <span class="outcome-name">{name}</span>
                    <span class="outcome-chance num">{shown}</span>
                    <span class="meter"><i style=width></i></span>
                </A>
            }
            .into_any()
        }
    });
    view! {
        <article class="event" class:featured=featured>
            <div class="event-head">
                <span class="tile"><Glyph icon=group.icon /></span>
                <span class="event-kicker">
                    <span class="event-category">{group.category.clone()}</span>
                    <span class="event-when"><Glyph icon=Icon::Clock size=12 />{when}</span>
                </span>
            </div>
            <A href=first.clone() attr:class="event-title">{group.title.clone()}</A>
            <div class=if matchup { "matchup" } else { "outcomes" }>{outcomes.collect_view()}</div>
            <div class="event-foot">
                <span class="foot-item"><Glyph icon=Icon::ChartLine size=13 />{format!("{} vol", fmt::usd_short(group.volume))}</span>
                <span class="foot-item"><Glyph icon=Icon::Layers size=13 />{lines}</span>
                <A href=first attr:class="go">"Trade"<Glyph icon=Icon::ChevronRight size=14 /></A>
            </div>
        </article>
    }
}

#[component]
fn MarketTable(rows: Vec<Market>) -> impl IntoView {
    let navigate = use_navigate();
    view! {
        <div class="table-wrap">
            <table>
                <thead>
                    <tr>
                        <th>"Market"</th>
                        <th class="r">"Chance"</th>
                        <th class="r hide-narrow">"Bid"</th>
                        <th class="r hide-narrow">"Ask"</th>
                        <th class="r">"24h volume"</th>
                        <th class="r hide-narrow">"Resolves"</th>
                    </tr>
                </thead>
                <tbody>
                    {rows.into_iter().map(|market| {
                        let navigate = navigate.clone();
                        let href = format!("/market/{}", market.symbol);
                        let chance = market.resolution.as_ref().map(|resolution| resolution.price).or(market.mark_price);
                        let resolves = market.display_attributes.expected_resolution_time_ms.map(fmt::date).unwrap_or_default();
                        view! {
                            <tr class="link" on:click=move |_| navigate(&href, Default::default())>
                                <td>
                                    <div>{market.title()}</div>
                                    <div class="faint mono">{market.symbol.clone()}</div>
                                </td>
                                <td class="r num">{chance.map(fmt::chance).unwrap_or_else(|| "-".to_owned())}</td>
                                <td class="r num hide-narrow">{fmt::opt_price(market.best_bid)}</td>
                                <td class="r num hide-narrow">{fmt::opt_price(market.best_ask)}</td>
                                <td class="r num">{fmt::usd(market.notional_24h())}</td>
                                <td class="r hide-narrow muted">{resolves}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}
