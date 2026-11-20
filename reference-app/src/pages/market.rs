//! Market page: price chart, live book and trades, order ticket, holders, event markets and collateral return.

use df_pascal_core::client::web::Feed;
use df_pascal_core::live::store::Book;
use df_pascal_core::live::{Channel, Data, Event, Frame, Session};
use df_pascal_core::markets::{self, Market, MarketSpec, PublicTrade, Range, Search};
use df_pascal_core::trade::{self, OrderRequest};
use df_pascal_core::utils::price::{self, Outcome};
use df_pascal_core::utils::{Interval, Round};
use df_pascal_core::{Dec, trade::Placed};
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use send_wrapper::SendWrapper;

use crate::events;
use crate::fmt;
use crate::icons::{Glyph, Icon};
use crate::state::{MarketList, describe, use_app, use_market_list};
use crate::ui::Chart;

#[component]
pub fn MarketPage() -> impl IntoView {
    let params = use_params_map();
    let symbol = move || params.read().get("symbol").unwrap_or_default();
    // Rebuild the view when the symbol changes so it opens a new socket.
    move || view! { <MarketView symbol=symbol() /> }
}

#[component]
fn MarketView(symbol: String) -> impl IntoView {
    let app = use_app();
    let wanted = symbol.clone();
    let market = LocalResource::new(move || {
        let client = app.client();
        let symbol = wanted.clone();
        async move {
            let found = markets::search(&*client, &Search { symbols: vec![symbol.clone()], ..Search::default() })
                .await
                .map_err(|error| describe(&error))?;
            found
                .data
                .into_iter()
                .find(|market| market.symbol == symbol)
                .ok_or_else(|| format!("{symbol} is not listed"))
        }
    });
    let _ = symbol;
    move || match market.get() {
        None => view! { <p class="empty">"Loading market"</p> }.into_any(),
        Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
        Some(Ok(market)) => view! { <Loaded market=market /> }.into_any(),
    }
}

/// Book and latest trades from the socket, shared with the ticket.
#[derive(Clone, Copy)]
struct LiveData {
    book: RwSignal<Book>,
    trades: RwSignal<Vec<PublicTrade>>,
    status: RwSignal<String>,
    /// Price of the last clicked book level, and whether it was an ask.
    picked: RwSignal<Option<(Dec, bool)>>,
}

#[component]
fn Loaded(market: Market) -> impl IntoView {
    let live = LiveData {
        book: RwSignal::new(Book::default()),
        trades: RwSignal::new(Vec::new()),
        status: RwSignal::new("Connecting".to_owned()),
        picked: RwSignal::new(None),
    };
    provide_context(live);
    start(use_app().deployment_config().data_ws_url, market.symbol.clone(), live);
    let (long, short) = market.outcome_names();
    let spec = market.spec();
    let attributes = market.display_attributes.clone();
    let now = use_app().client().now_ms();
    let resolves =
        attributes.expected_resolution_time_ms.map(|ms| fmt::moment(ms, now)).unwrap_or_else(|| "-".to_owned());
    let chance = market.resolution.as_ref().map(|resolution| resolution.price).or(market.mark_price);
    let change = market.change_24h();
    let book = live.book;
    let best = move |bids: bool, fallback: Option<Dec>| {
        move || {
            let book = book.get();
            let level = if bids { book.best_bid() } else { book.best_ask() };
            fmt::opt_price(level.map(|level| level.price).or(fallback))
        }
    };
    let (icon, category) = events::category(&market);
    let teams = events::teams(&market);
    let (short_mark, colour) = events::badge(&market, teams.as_ref());
    let when = match (market.resolution.as_ref(), attributes.expected_event_start_time_ms) {
        (Some(resolution), _) => format!("Resolved {}", fmt::moment(resolution.market_resolution_ts_ms, now)),
        (None, Some(starts)) if starts > now => format!("Starts {}", fmt::moment(starts, now)),
        (None, Some(starts)) => format!("Started {}", fmt::moment(starts, now)),
        (None, None) => format!("Resolves {resolves}"),
    };
    let (trend, trend_class) = match change {
        Some(change) if change.is_negative() => (Icon::TrendingDown, "pill short"),
        Some(change) if change.is_positive() => (Icon::TrendingUp, "pill long"),
        _ => (Icon::Activity, "pill"),
    };
    let event_title = match &attributes.game_page {
        Some(page) if !page.game_description.is_empty() => page.game_description.clone(),
        _ => attributes.event_description.clone(),
    };
    let line =
        attributes.game_page.as_ref().map(|page| page.section_description.clone()).filter(|line| !line.is_empty());
    view! {
        <div class="market-page">
            <section class="market-hero">
                <nav class="crumbs" aria-label="Breadcrumb">
                    <A href="/">"Markets"</A>
                    <Glyph icon=Icon::ChevronRight size=12 />
                    <span>{category}</span>
                    <Glyph icon=Icon::ChevronRight size=12 />
                    <span class="clip">{event_title.clone()}</span>
                </nav>
                <div class="hero-main">
                    <span class="badge-mark xl" style=format!("--c:{colour}")>{short_mark}</span>
                    <div class="hero-names">
                        <span class="hero-event"><Glyph icon=icon size=14 />{event_title}{line.map(|line| format!(", {line}"))}</span>
                        <h1>{attributes.market_description.clone()}</h1>
                        <span class="hero-when"><Glyph icon=Icon::Clock size=13 />{when}</span>
                    </div>
                    <div class="hero-price">
                        <span class="hero-chance num">{chance.map(fmt::chance).unwrap_or_else(|| "-".to_owned())}</span>
                        <span class="hero-price-foot">
                            <span class="faint">{format!("{long} chance")}</span>
                            <span class=trend_class><Glyph icon=trend size=12 />{change.map(|change| format!("{} 24h", fmt::price(change))).unwrap_or_else(|| "No change".to_owned())}</span>
                        </span>
                    </div>
                </div>
                <div class="hero-tiles">
                    <div class="mini"><span class="mini-head"><Glyph icon=Icon::TrendingUp size=13 />"Best bid"</span><span class="mini-value num long">{best(true, market.best_bid)}</span></div>
                    <div class="mini"><span class="mini-head"><Glyph icon=Icon::TrendingDown size=13 />"Best ask"</span><span class="mini-value num short">{best(false, market.best_ask)}</span></div>
                    <div class="mini"><span class="mini-head"><Glyph icon=Icon::ChartLine size=13 />"24h volume"</span><span class="mini-value num">{fmt::usd(market.notional_24h())}</span></div>
                    <div class="mini"><span class="mini-head"><Glyph icon=Icon::Users size=13 />"Long interest"</span><span class="mini-value num">{fmt::count(market.long_interest.unwrap_or(market.open_interest))}</span></div>
                    <div class="mini"><span class="mini-head"><Glyph icon=Icon::Percent size=13 />"Taker fee"</span><span class="mini-value num">{fmt::chance(market.taker_fee_rate)}</span></div>
                    <div class="mini"><span class="mini-head"><Glyph icon=Icon::CalendarClock size=13 />"Resolves"</span><span class="mini-value num">{resolves.clone()}</span></div>
                </div>
            </section>
            <Siblings market=market.clone() teams=teams />
            {market.resolution.clone().map(|resolution| view! {
                <p class="notice ok">{format!("Resolved at {} on {}", fmt::price(resolution.price), fmt::time(resolution.market_resolution_ts_ms))}</p>
            })}
            <div class="desk">
                <div class="desk-chart"><PriceChart symbol=market.symbol.clone() /></div>
                <div class="desk-book"><BookCard long=long.clone() short=short.clone() /></div>
                <div class="desk-ticket">
                    <Ticket spec=spec long=long short=short resolved=market.is_resolved() />
                    {market.is_mece.then(|| view! { <CollateralCard event=market.event_code().to_owned() /> })}
                </div>
                <div class="desk-lower"><Lower market=market.clone() /></div>
            </div>
        </div>
    }
}

/// The event's outcomes as a row of links, the current one marked.
#[component]
fn Siblings(market: Market, teams: Option<events::Teams>) -> impl IntoView {
    let Some(MarketList(list)) = use_market_list() else { return ().into_any() };
    let symbol = market.symbol.clone();
    let row = move || {
        let Some(Ok(all)) = list.get() else { return ().into_any() };
        let siblings = events::siblings(&market, &all);
        if siblings.len() < 2 {
            return ().into_any();
        }
        view! {
            <div class="siblings">
                {siblings.into_iter().map(|other| {
                    let (mark, colour) = events::badge(&other, teams.as_ref());
                    let current = (other.symbol == symbol).then_some("page");
                    view! {
                        <A href=format!("/market/{}", other.symbol) attr:class="sib" attr:aria-current=current attr:style=format!("--c:{colour}")>
                            <span class="badge-mark">{mark}</span>
                            <span class="sib-name">{events::outcome_name(&other)}</span>
                            <span class="num sib-chance">{events::chance(&other).map(fmt::chance).unwrap_or_else(|| "-".to_owned())}</span>
                        </A>
                    }
                }).collect_view()}
            </div>
        }
        .into_any()
    };
    view! { {row} }.into_any()
}

/// Tabs below the book: trades, largest holders, event markets and terms.
#[component]
fn Lower(market: Market) -> impl IntoView {
    let tab = RwSignal::new("trades");
    let tabs = [
        ("trades", "Trades", Icon::Activity),
        ("holders", "Largest holders", Icon::Users),
        ("event", "Event markets", Icon::Layers),
        ("about", "About", Icon::BookOpen),
    ];
    let attributes = market.display_attributes.clone();
    view! {
        <div class="card flush">
            <div class="card-head tabbar">
                {tabs.into_iter().map(|(id, name, icon)| view! {
                    <button class="tab" aria-pressed=move || (tab.get() == id).to_string() on:click=move |_| tab.set(id)><Glyph icon=icon size=14 />{name}</button>
                }).collect_view()}
            </div>
            <div class="pane" hidden=move || tab.get() != "trades"><TradesCard /></div>
            <div class="pane" hidden=move || tab.get() != "holders"><HoldersCard symbol=market.symbol.clone() /></div>
            <div class="pane" hidden=move || tab.get() != "event"><EventCard market=market.clone() /></div>
            <div class="pane pad" hidden=move || tab.get() != "about">
                <dl class="facts">
                    <dt>"Symbol"</dt><dd class="mono">{market.symbol.clone()}</dd>
                    <dt>"Event"</dt><dd>{attributes.event_description.clone()}</dd>
                    <dt>"Event code"</dt><dd class="mono">{market.event_code().to_owned()}</dd>
                    <dt>"Mutually exclusive"</dt><dd>{if market.is_mece { "Yes" } else { "No" }}</dd>
                    <dt>"Tags"</dt><dd><span class="row">{attributes.tags.iter().map(|tag| view! { <span class="badge">{tag.clone()}</span> }).collect_view()}</span></dd>
                    <dt>"Rules"</dt><dd>{market.rules_url.clone().map(|url| view! { <a href=url target="_blank" rel="noopener">"Open the rules"</a> })}</dd>
                </dl>
            </div>
        </div>
    }
}

/// Opens the market data socket and updates the book and trades until the page is unmounted.
fn start(url: String, symbol: String, live: LiveData) {
    let app = use_app();
    let mut feed = Feed::start(Session::new(url));
    let stopper = SendWrapper::new(feed.stopper());
    on_cleanup(move || stopper.stop());
    let client = app.client();
    leptos::task::spawn_local(async move {
        if let Ok(recent) = markets::recent(&*client, &[&symbol], Some(30)).await
            && let Some(first) = recent.data.into_iter().next()
        {
            let _ = live.trades.try_set(first.trades);
        }
        let _ = feed.subscribe(Channel::book(&symbol)).await;
        let _ = feed.subscribe(Channel::trades(&symbol)).await;
        while let Some(event) = feed.next().await {
            match event {
                Event::Opened => {
                    let _ = live.status.try_set("Live".to_owned());
                }
                Event::Closed { reason, retry } => {
                    let _ = live.status.try_set(format!("Reconnecting in {} s ({reason})", retry.after_ms / 1_000));
                }
                Event::Frame(frame) => match *frame {
                    Frame::Message { kind, data: Data::Book(update), context, .. } => {
                        live.book.try_update(|book| {
                            book.apply(kind, &update, context.seq);
                        });
                    }
                    Frame::Message { data: Data::Trades(new), .. } => {
                        live.trades.try_update(|trades| {
                            for trade in new {
                                if !trades.iter().any(|known| known.trade_id == trade.trade_id) {
                                    trades.insert(0, trade);
                                }
                            }
                            trades.truncate(30);
                        });
                    }
                    Frame::Error { message, .. } => {
                        let _ = live.status.try_set(message);
                    }
                    _ => {}
                },
                Event::Unreadable(_) => {}
            }
        }
    });
}

#[component]
fn BookCard(long: String, short: String) -> impl IntoView {
    let live = use_context::<LiveData>();
    let Some(live) = live else { return ().into_any() };
    let depth = 8;
    let ladder = move || {
        let book = live.book.get();
        let asks: Vec<_> = book.asks().take(depth).map(|level| (level.price, level.size)).collect();
        let bids: Vec<_> = book.bids().take(depth).map(|level| (level.price, level.size)).collect();
        let total = |levels: &[(Dec, u64)]| levels.iter().map(|level| level.1).sum::<u64>();
        let deepest = total(&asks).max(total(&bids)).max(1);
        let rows = move |levels: Vec<(Dec, u64)>, ask: bool| {
            let mut running = 0_u64;
            let mut rows: Vec<_> = levels
                .into_iter()
                .map(|(price, size)| {
                    running += size;
                    let share = running.saturating_mul(1_000) / deepest;
                    let style = format!("--d:{}.{:03}", share / 1_000, share % 1_000);
                    view! {
                        <div class="lvl" class:ask=ask class:bid=!ask role="button" title="Fill the ticket at this price"
                            on:click=move |_| live.picked.set(Some((price, ask)))>
                            <i class="depth" style=style></i>
                            <span class="px">{fmt::price(price)}</span>
                            <span>{fmt::count(size)}</span>
                            <span class="faint">{fmt::count(running)}</span>
                        </div>
                    }
                })
                .collect();
            if ask {
                rows.reverse();
            }
            rows.collect_view()
        };
        let last = live.trades.with(|trades| trades.first().map(|trade| trade.price));
        let spread = match (book.best_bid(), book.best_ask()) {
            (Some(bid), Some(ask)) => {
                format!("Spread {}", fmt::price(ask.price.checked_sub(bid.price).unwrap_or(Dec::ZERO)))
            }
            _ => String::new(),
        };
        view! {
            <div class="ladder-side asks">{rows(asks, true)}</div>
            <div class="mid">
                <span class="mid-px">{fmt::opt_price(last)}</span>
                <span class="faint">"last"</span>
                <span class="faint num push">{spread}</span>
            </div>
            <div class="ladder-side bids">{rows(bids, false)}</div>
        }
    };
    view! {
        <div class="card flush book-card">
            <div class="card-head">
                <h2><Glyph icon=Icon::Layers size=14 />"Book"</h2>
                <span class="live-state">
                    <i class=move || if live.status.get() == "Live" { "dot" } else { "dot idle" }></i>
                    {move || live.status.get()}
                </span>
            </div>
            <div class="ladder-cols"><span>"Price"</span><span>"Size"</span><span>"Total"</span></div>
            <div class="ladder">{ladder}</div>
            <p class="foot">{format!("Bids buy {long}; asks buy {short} at one less the price. Click a level to fill the ticket.")}</p>
        </div>
    }
    .into_any()
}

#[component]
fn TradesCard() -> impl IntoView {
    let Some(live) = use_context::<LiveData>() else { return ().into_any() };
    let app = use_app();
    view! {
        <div>
            <div class="table-wrap">
                <table>
                    <thead><tr><th>"Taker"</th><th class="r">"Price"</th><th class="r">"Size"</th><th class="r">"Age"</th></tr></thead>
                    <tbody>
                        {move || {
                            let now = app.client().now_ms();
                            live.trades.get().into_iter().map(|trade| view! {
                                <tr>
                                    <td class=if trade.taker_side == df_pascal_core::utils::Side::Bid { "long" } else { "short" }>
                                        {if trade.taker_side == df_pascal_core::utils::Side::Bid { "Buy" } else { "Sell" }}
                                    </td>
                                    <td class="r num">{fmt::price(trade.price)}</td>
                                    <td class="r num">{fmt::count(trade.size)}</td>
                                    <td class="r faint">{fmt::ago(trade.trade_ts_ms, now)}</td>
                                </tr>
                            }).collect_view()
                        }}
                    </tbody>
                </table>
            </div>
        </div>
    }
    .into_any()
}

#[component]
fn PriceChart(symbol: String) -> impl IntoView {
    let app = use_app();
    let interval = RwSignal::new(Interval::H1);
    let symbol = StoredValue::new(symbol);
    let candles = LocalResource::new(move || {
        let interval = interval.get();
        let client = app.client();
        let symbol = symbol.get_value();
        async move {
            let range = Range::last(interval, client.now_ms(), 200);
            markets::mark_price_candles(&*client, &symbol, &range)
                .await
                .map(|reply| reply.data)
                .map_err(|error| describe(&error))
        }
    });
    let values = Signal::derive(move || match candles.get() {
        Some(Ok(mut candles)) => {
            candles.sort_by_key(|candle| candle.time);
            candles.into_iter().map(|candle| candle.close).collect()
        }
        _ => Vec::new(),
    });
    let choices = [
        ("Minutes", Interval::M1),
        ("5 min", Interval::M5),
        ("Hours", Interval::H1),
        ("4 hours", Interval::H4),
        ("Days", Interval::D1),
    ];
    view! {
        <div class="card">
            <div class="card-head">
                <h2><Glyph icon=Icon::ChartLine size=14 />"Mark price"</h2>
                <div class="tabs">
                    {choices.into_iter().map(|(name, choice)| {
                        let pressed = choice.clone();
                        view! {
                            <button class="small" aria-pressed=move || (interval.get() == pressed).to_string()
                                on:click=move |_| interval.set(choice.clone())>{name}</button>
                        }
                    }).collect_view()}
                </div>
            </div>
            {move || match candles.get() {
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                _ => view! { <Chart values=values label=fmt::price /> }.into_any(),
            }}
        </div>
    }
}

#[component]
fn EventCard(market: Market) -> impl IntoView {
    let app = use_app();
    let event = market.event_code().to_owned();
    let symbol = market.symbol.clone();
    let siblings = LocalResource::new(move || {
        let client = app.client();
        let event = event.clone();
        async move {
            markets::search(&*client, &Search { events: vec![event], ..Search::default() })
                .await
                .map(|reply| reply.data)
                .map_err(|error| describe(&error))
        }
    });
    view! {
        <div>
            {move || match siblings.get() {
                None => view! { <p class="empty">"Loading"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                Some(Ok(markets)) => {
                    let mut markets: Vec<Market> = markets.into_iter().filter(|other| other.symbol != symbol).collect();
                    markets.sort_by(|a, b| b.mark_price.cmp(&a.mark_price));
                    if markets.is_empty() {
                        return view! { <p class="empty">"This event has one market."</p> }.into_any();
                    }
                    view! {
                        <div class="table-wrap"><table>
                            <thead><tr><th>"Market"</th><th class="r">"Chance"</th></tr></thead>
                            <tbody>
                            {markets.into_iter().map(|other| view! {
                                <tr>
                                    <td><A href=format!("/market/{}", other.symbol)>{other.display_attributes.market_description.clone()}</A></td>
                                    <td class="r num">{other.resolution.as_ref().map(|r| r.price).or(other.mark_price).map(fmt::chance).unwrap_or_default()}</td>
                                </tr>
                            }).collect_view()}
                        </tbody></table></div>
                    }.into_any()
                }
            }}
        </div>
    }
}

#[component]
fn HoldersCard(symbol: String) -> impl IntoView {
    let app = use_app();
    let wanted = symbol.clone();
    let holders = LocalResource::new(move || {
        let client = app.client();
        let symbol = wanted.clone();
        async move {
            markets::top_holders(&*client, &[&symbol], Some(10))
                .await
                .map(|reply| reply.data.into_values().next().unwrap_or_default())
                .map_err(|error| describe(&error))
        }
    });
    let list = |rows: Vec<df_pascal_core::markets::Holder>| {
        rows.into_iter()
            .map(|holder| {
                view! {
                    <tr>
                        <td class="mono">{holder.owner.short()}</td>
                        <td class="r num">{fmt::count(holder.position.size)}</td>
                        <td class="r num">{fmt::opt_price(holder.position.average_entry_price)}</td>
                        <td class="r num">{fmt::signed_usd(holder.position.unrealized_pnl_usd)}</td>
                    </tr>
                }
            })
            .collect_view()
    };
    view! {
        <div>
            {move || match holders.get() {
                None => view! { <p class="empty">"Loading"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                Some(Ok(holders)) => view! {
                    <div class="table-wrap"><table>
                        <thead><tr><th>"Owner"</th><th class="r">"Contracts"</th><th class="r">"Entry"</th><th class="r">"Unrealized"</th></tr></thead>
                        <tbody>{list(holders.longs)}{list(holders.shorts)}</tbody>
                    </table></div>
                }.into_any(),
            }}
        </div>
    }
}

/// Order ticket in Yes and No terms; the SDK maps them to bids and asks.
#[component]
fn Ticket(spec: MarketSpec, long: String, short: String, resolved: bool) -> impl IntoView {
    let app = use_app();
    let live = use_context::<LiveData>();
    let outcome = RwSignal::new(Outcome::Yes);
    let buy = RwSignal::new(true);
    let market_order = RwSignal::new(false);
    let price_text = RwSignal::new(String::new());
    let size_text = RwSignal::new("1".to_owned());
    let post_only = RwSignal::new(false);
    let expiry_minutes = RwSignal::new(String::new());
    let busy = RwSignal::new(false);
    let last = RwSignal::new(None::<String>);
    let spec = StoredValue::new(spec);

    // Sets the price field from the best price on the opposite side of the book.
    let suggest = move || {
        let Some(live) = live else { return };
        let book = live.book.get_untracked();
        let (yes_bid, yes_ask) = (book.best_bid().map(|level| level.price), book.best_ask().map(|level| level.price));
        let quote = match (outcome.get_untracked(), buy.get_untracked()) {
            (Outcome::Yes, true) => yes_ask,
            (Outcome::Yes, false) => yes_bid,
            (Outcome::No, true) => yes_bid.map(price::complement),
            (Outcome::No, false) => yes_ask.map(price::complement),
        };
        if let Some(quote) = quote {
            price_text.set(fmt::price(quote));
        }
    };

    // Prefill the price once the book arrives, if the field is empty.
    Effect::new(move |_| {
        if let Some(live) = live {
            live.book.track();
            if price_text.get_untracked().is_empty() {
                suggest();
            }
        }
    });

    // Clicking a book level sets a limit order: buy Yes at an ask, sell Yes at a bid.
    Effect::new(move |_| {
        if let Some((price, ask)) = live.and_then(|live| live.picked.get()) {
            outcome.set(Outcome::Yes);
            buy.set(ask);
            market_order.set(false);
            price_text.set(fmt::price(price));
        }
    });

    let order = move || -> Result<OrderRequest, String> {
        let spec = spec.get_value();
        let size: u64 = size_text.get().trim().parse().map_err(|_| "Size is a whole number of contracts".to_owned())?;
        let id = trade::client_order_id(app.client().now_ms(), 0);
        if market_order.get() {
            let (side, _, reduce_only) = price::yes_no(outcome.get(), buy.get(), Dec::ONE);
            let worst = match side {
                df_pascal_core::utils::Side::Bid => Dec::ONE,
                _ => spec.tick_size_min,
            };
            let mut order = OrderRequest::market(spec.symbol.clone(), side, size, worst, id);
            if reduce_only {
                order = order.reduce_only();
            }
            return Ok(order);
        }
        if price_text.get().trim().is_empty() {
            return Err("Enter a price, or press Best price".to_owned());
        }
        let shown = Dec::parse(price_text.get().trim()).map_err(|_| "Price is a decimal from 0 to 1".to_owned())?;
        let mut order = OrderRequest::outcome(spec.symbol.clone(), outcome.get(), buy.get(), shown, size, id);
        if !spec.is_on_tick(order.0.price) {
            return Err(format!(
                "{} is off the tick grid ({} here)",
                fmt::price(order.0.price),
                fmt::price(spec.tick_size(order.0.price))
            ));
        }
        if post_only.get() {
            order = order.post_only();
        }
        if let Ok(minutes) = expiry_minutes.get().trim().parse::<u64>() {
            order = order.good_til(app.client().now_ms() + minutes * 60_000);
        }
        Ok(order)
    };

    let summary = move || match order() {
        Err(error) => view! { <p class="notice warn">{error}</p> }.into_any(),
        Ok(order) => {
            let spec = spec.get_value();
            let order = order.0;
            let collateral = price::collateral(&order.side, order.size, order.price);
            let fee = spec.taker_fee(order.size, order.price);
            view! {
                <dl class="slip">
                    <dt>"Sends"</dt>
                    <dd>{format!("{} {} {} at {}{}{}", order.order_type, order.side, order.size, fmt::price(order.price),
                        if order.reduce_only { ", reduce-only" } else { "" }, if order.post_only { ", post-only" } else { "" })}</dd>
                    <dt>"Collateral, at most"</dt>
                    <dd>{fmt::usd(collateral)}</dd>
                    <dt>"Taker fee, about"</dt>
                    <dd>{format!("${}", fee.to_fixed(4))}</dd>
                </dl>
            }.into_any()
        }
    };

    let round = move |_| {
        if let Ok(shown) = Dec::parse(price_text.get().trim()) {
            let spec = spec.get_value();
            let exchange = if outcome.get() == Outcome::Yes { shown } else { price::complement(shown) };
            let snapped = spec.round(exchange, Round::Nearest);
            let back = if outcome.get() == Outcome::Yes { snapped } else { price::complement(snapped) };
            price_text.set(fmt::price(back));
        }
    };

    let submit = move |_| {
        let order = match order() {
            Ok(order) => order,
            Err(error) => return app.fail(error),
        };
        if !app.has_key() {
            return app.fail("Trading needs a trading key: create or load one on Trading keys.");
        }
        busy.set(true);
        let client = app.client();
        leptos::task::spawn_local(async move {
            match trade::place(&*client, &order).await {
                Ok(reply) => {
                    last.set(Some(placed(&reply.data)));
                    app.toast(placed(&reply.data), true);
                    app.bump();
                }
                Err(error) => app.fail(describe(&error)),
            }
            busy.set(false);
        });
    };

    let (long_label, short_label) = (long.clone(), short.clone());
    let quote = move |yes: bool| {
        move || {
            let Some(live) = live else { return "-".to_owned() };
            let book = live.book.get();
            let (bid, ask) = (book.best_bid().map(|level| level.price), book.best_ask().map(|level| level.price));
            let price = match (yes, buy.get()) {
                (true, true) => ask,
                (true, false) => bid,
                (false, true) => bid.map(price::complement),
                (false, false) => ask.map(price::complement),
            };
            fmt::opt_price(price).replace("-", "No quote")
        }
    };
    view! {
        <div class="card console" class:selling=move || !buy.get()>
            <h2><Glyph icon=Icon::Zap size=14 />"Order"</h2>
            {resolved.then(|| view! { <p class="notice">"This market has resolved and takes no orders."</p> })}
            <div class="tabs picks">
                <button data-tone="long" aria-pressed=move || (outcome.get() == Outcome::Yes).to_string() on:click=move |_| { outcome.set(Outcome::Yes); suggest(); }>
                    <span class="pick-name">{long_label}</span>
                    <span class="pick-px num">{quote(true)}</span>
                </button>
                <button data-tone="short" aria-pressed=move || (outcome.get() == Outcome::No).to_string() on:click=move |_| { outcome.set(Outcome::No); suggest(); }>
                    <span class="pick-name">{short_label}</span>
                    <span class="pick-px num">{quote(false)}</span>
                </button>
            </div>
            <div class="field-row">
                <div class="tabs">
                    <button aria-pressed=move || buy.get().to_string() on:click=move |_| { buy.set(true); suggest(); }>"Buy"</button>
                    <button aria-pressed=move || (!buy.get()).to_string() on:click=move |_| { buy.set(false); suggest(); }>"Sell"</button>
                </div>
                <div class="tabs">
                    <button aria-pressed=move || (!market_order.get()).to_string() on:click=move |_| market_order.set(false)>"Limit"</button>
                    <button aria-pressed=move || market_order.get().to_string() on:click=move |_| market_order.set(true)>"Market"</button>
                </div>
            </div>
            <div class="field-row">
                <Show when=move || !market_order.get()>
                    <label class="field">"Price ($0 to $1)"
                        <input inputmode="decimal" prop:value=move || price_text.get() on:input=move |ev| price_text.set(event_target_value(&ev)) />
                    </label>
                </Show>
                <label class="field">"Contracts"
                    <input inputmode="numeric" prop:value=move || size_text.get() on:input=move |ev| size_text.set(event_target_value(&ev)) />
                </label>
            </div>
            <Show when=move || !market_order.get()>
                <div class="row">
                    <button class="small" on:click=move |_| suggest()>"Best price"</button>
                    <button class="small" on:click=round>"Round to tick"</button>
                    <label class="check"><input type="checkbox" prop:checked=move || post_only.get() on:change=move |ev| post_only.set(event_target_checked(&ev)) />"Post only"</label>
                </div>
                <label class="field">"Cancel after minutes (empty: until canceled)"
                    <input inputmode="numeric" prop:value=move || expiry_minutes.get() on:input=move |ev| expiry_minutes.set(event_target_value(&ev)) />
                </label>
            </Show>
            {summary}
            <button class=move || if buy.get() { "send buy" } else { "send sell" } disabled=move || busy.get() || resolved on:click=submit>
                {move || match (busy.get(), buy.get()) {
                    (true, _) => "Sending".to_owned(),
                    (false, true) => format!("Buy {}", if outcome.get() == Outcome::Yes { long.clone() } else { short.clone() }),
                    (false, false) => format!("Sell {}", if outcome.get() == Outcome::Yes { long.clone() } else { short.clone() }),
                }}
            </button>
            {move || last.get().map(|text| view! { <p class="faint">{text}</p> })}
        </div>
    }
}

fn placed(placed: &Placed) -> String {
    let order = &placed.order;
    let reason = placed.update_reason.as_ref().map(|reason| format!(" ({reason})")).unwrap_or_default();
    format!(
        "Order {}: {} filled, {} open of {}{}",
        order.client_order_id, order.size_filled, order.size_remaining, order.size_original, reason
    )
}

/// Releases collateral the event's positions lock beyond their maximum payout, in one request signed
/// by the trading key.
#[component]
fn CollateralCard(event: String) -> impl IntoView {
    let app = use_app();
    let busy = RwSignal::new(false);
    let event = StoredValue::new(event);
    let run = move |_| {
        busy.set(true);
        let client = app.client();
        leptos::task::spawn_local(async move {
            match trade::return_collateral(&*client, &event.get_value()).await {
                Ok(reply) => {
                    app.toast(format!("Returned {} of collateral", fmt::usd(reply.data.collateral_change_usd)), true);
                    app.bump();
                }
                Err(error) => app.fail(describe(&error)),
            }
            busy.set(false);
        });
    };
    view! {
        <div class="card">
            <h2><Glyph icon=Icon::Scale size=14 />"Return collateral"</h2>
            <p class="muted">"Positions across every market of this event can lock more collateral than they can pay out. Returning collateral converts them as far as they allow and frees the rest."</p>
            <button disabled=move || busy.get() on:click=run>{move || format!("Return collateral for {}", event.get_value())}</button>
        </div>
    }
}
