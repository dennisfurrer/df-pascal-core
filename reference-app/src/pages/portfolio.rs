//! Portfolio page: balances, live positions and open orders, PnL chart and paged history.

use df_pascal_core::account::{
    self, ClosedPosition, ClosedQuery, MoversQuery, PositionResolution, StateQuery, Timeseries, Transfer,
};
use df_pascal_core::client::web::Feed;
use df_pascal_core::live::store::Account;
use df_pascal_core::live::{AccountUpdate, Channel, Data, Event, Frame, Kind, Session};
use df_pascal_core::trade::{self, CancelBy, CollateralReturn, Fill, OrderRequest};
use df_pascal_core::utils::{MoversWindow, Page, PageRequest, Side, Timeframe};
use df_pascal_core::{Dec, Pubkey};
use leptos::prelude::*;
use leptos_router::components::A;
use send_wrapper::SendWrapper;

use crate::fmt;
use crate::state::{describe, use_app};
use crate::ui::{Chart, NeedAccount, Stat};

#[component]
pub fn Portfolio() -> impl IntoView {
    let app = use_app();
    move || match app.owner.get() {
        None => view! { <NeedAccount /> }.into_any(),
        Some(owner) => {
            let _ = app.deployment.get();
            view! { <Holdings owner=owner /> }.into_any()
        }
    }
}

#[component]
fn Holdings(owner: Pubkey) -> impl IntoView {
    let app = use_app();
    let held = RwSignal::new(Account::default());
    let events = RwSignal::new(Vec::<String>::new());
    let status = RwSignal::new("Connecting".to_owned());

    // Load the REST snapshot first so the page fills in before the socket opens.
    let client = app.client();
    leptos::task::spawn_local(async move {
        match account::state(&*client, &owner, &StateQuery::default()).await {
            Ok(reply) => {
                held.try_update(|held| {
                    if !held.is_ready() {
                        let update = AccountUpdate { state: reply.data, ..AccountUpdate::default() };
                        held.apply(Kind::Snapshot, &update, reply.context.seq);
                    }
                });
            }
            Err(error) => app.fail(describe(&error)),
        }
    });

    let mut feed = Feed::start(Session::new(app.deployment_config().data_ws_url));
    let stopper = SendWrapper::new(feed.stopper());
    on_cleanup(move || stopper.stop());
    leptos::task::spawn_local(async move {
        let _ = feed.subscribe(Channel::account(owner)).await;
        while let Some(event) = feed.next().await {
            match event {
                Event::Opened => {
                    let _ = status.try_set("Live".to_owned());
                }
                Event::Closed { reason, .. } => {
                    let _ = status.try_set(format!("Reconnecting ({reason})"));
                }
                Event::Frame(frame) => {
                    if let Frame::Message { kind, data: Data::Account(update), context, .. } = *frame {
                        held.try_update(|held| {
                            held.apply(kind, &update, context.seq);
                        });
                        let lines = describe_update(&update);
                        if !lines.is_empty() {
                            events.try_update(|events| {
                                for line in lines.into_iter().rev() {
                                    events.insert(0, line);
                                }
                                events.truncate(40);
                            });
                        }
                    }
                }
                Event::Unreadable(_) => {}
            }
        }
    });

    let value = move || held.with(|held| held.to_state().account_value());
    let unrealized = move || held.with(|held| held.to_state().unrealized_pnl());

    view! {
        <div class="strip">
            <div class="strip-title">
                <span class="label live-state">
                    <i class=move || if status.get() == "Live" { "dot" } else { "dot idle" }></i>
                    {move || format!("Portfolio, {}", status.get().to_lowercase())}
                </span>
                <span class="name mono" title=owner.to_base58()>{owner.short()}</span>
            </div>
            <Stat label="Account value" value=Signal::derive(move || fmt::usd(value())) />
            <Stat label="Free collateral" value=Signal::derive(move || fmt::usd(held.with(|held| held.collateral_usd))) />
            <Stat label="Unrealized PnL" value=Signal::derive(move || fmt::signed_usd(unrealized())) />
            <Stat label="Open orders" value=Signal::derive(move || held.with(|held| held.orders.len()).to_string()) />
            <Stat label="Lifetime volume" value=Signal::derive(move || held.with(|held| {
                held.lifetime_stats.as_ref().map(|stats| fmt::usd(stats.maker.volume_notional_usd.checked_add(stats.taker.volume_notional_usd).unwrap_or(Dec::ZERO))).unwrap_or_else(|| "-".to_owned())
            })) />
            <Stat label="Rewards received" value=Signal::derive(move || held.with(|held| {
                held.rewards.as_ref().map(|rewards| fmt::usd(Dec::checked_sum([rewards.referral_rewards_received_usd, rewards.builder_fees_received_usd, rewards.user_rewards_received_usd]).unwrap_or(Dec::ZERO))).unwrap_or_else(|| "-".to_owned())
            })) />
        </div>
        <div class="desk-split">
            <div class="column">
                <Positions held=held />
                <Orders held=held />
                <History owner=owner />
            </div>
            <div class="column">
                <PnlChart owner=owner />
                <Movers owner=owner />
                <div class="card">
                    <h2>"Live events"</h2>
                    {move || {
                        let events = events.get();
                        if events.is_empty() {
                            return view! { <p class="empty">"Fills, order changes and transfers show here as they happen."</p> }.into_any();
                        }
                        view! { <div class="events">{events.into_iter().map(|line| view! { <span class="mono">{line}</span> }).collect_view()}</div> }.into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

fn describe_update(update: &AccountUpdate) -> Vec<String> {
    let mut lines = Vec::new();
    for change in &update.order_updates {
        let order = &change.order;
        let reason = change.update_reason.as_ref().map(|reason| format!(" {reason}")).unwrap_or_default();
        lines.push(format!(
            "order {} {} {} {} @ {}: {} filled, {} open{reason}",
            order.client_order_id,
            order.symbol,
            order.side,
            order.size_original,
            fmt::price(order.price),
            order.size_filled,
            order.size_remaining
        ));
    }
    for fill in &update.fills {
        lines.push(format!(
            "fill {} {} {} @ {} ({}), fee {}",
            fill.symbol,
            fill.side,
            fill.fill_size,
            fmt::price(fill.fill_price),
            fill.liquidity,
            fmt::usd(fill.fee_usd)
        ));
    }
    for transfer in &update.transfers {
        lines.push(format!(
            "{} {} {}",
            transfer.kind,
            transfer.status,
            fmt::signed_usd(transfer.collateral_change_usd)
        ));
    }
    for resolution in &update.resolutions {
        lines.push(format!(
            "resolved {} at {}: {}",
            resolution.symbol,
            fmt::price(resolution.resolution),
            fmt::signed_usd(resolution.collateral_change_usd)
        ));
    }
    for returned in &update.collateral_returns {
        lines.push(format!("collateral returned {}", fmt::usd(returned.collateral_change_usd)));
    }
    lines
}

#[component]
fn Positions(held: RwSignal<Account>) -> impl IntoView {
    let app = use_app();
    let close = move |symbol: String, size: i64| {
        let client = app.client();
        leptos::task::spawn_local(async move {
            // Reduce-only immediate-or-cancel market order on the opposite side at the worst price.
            let (side, worst) = if size > 0 { (Side::Ask, Dec::lit::<2>(1)) } else { (Side::Bid, Dec::ONE) };
            let id = trade::client_order_id(client.now_ms(), 1);
            let order = OrderRequest::market(symbol, side, size.unsigned_abs(), worst, id).reduce_only();
            match trade::place(&*client, &order).await {
                Ok(reply) => app.toast(format!("Closed {} contracts", reply.data.order.size_filled), true),
                Err(error) => app.fail(describe(&error)),
            }
        });
    };
    view! {
        <div class="card">
            <h2>"Positions"</h2>
            {move || {
                let positions: Vec<_> = held.with(|held| held.positions.values().cloned().collect());
                if positions.is_empty() {
                    return view! { <p class="empty">"No open positions."</p> }.into_any();
                }
                view! {
                    <div class="table-wrap"><table>
                        <thead><tr>
                            <th>"Market"</th><th class="r">"Size"</th><th class="r">"Entry"</th><th class="r">"Mark"</th>
                            <th class="r">"Value"</th><th class="r">"Unrealized"</th><th class="r hide-narrow">"Realized"</th><th></th>
                        </tr></thead>
                        <tbody>
                            {positions.into_iter().map(|position| {
                                let symbol = position.symbol.clone();
                                let size = position.size;
                                let href = format!("/market/{}", position.symbol);
                                let name = position.symbol.clone();
                                view! {
                                    <tr>
                                        <td><A href=href>{name}</A></td>
                                        <td class=if position.is_long() { "r num long" } else { "r num short" }>{fmt::count(position.size)}</td>
                                        <td class="r num">{fmt::opt_price(position.average_entry_price)}</td>
                                        <td class="r num">{fmt::price(position.mark_price)}</td>
                                        <td class="r num">{fmt::usd(position.value())}</td>
                                        <td class="r num">{fmt::signed_usd(position.unrealized_pnl_usd)}</td>
                                        <td class="r num hide-narrow">{fmt::signed_usd(position.realized_pnl_usd)}</td>
                                        <td class="r"><button class="small" disabled=move || !app.has_key() on:click=move |_| close(symbol.clone(), size)>"Close"</button></td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table></div>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn Orders(held: RwSignal<Account>) -> impl IntoView {
    let app = use_app();
    let cancel = move |client_order_id: u64| {
        let client = app.client();
        leptos::task::spawn_local(async move {
            match trade::cancel(&*client, CancelBy::ClientOrderId(client_order_id)).await {
                Ok(_) => app.toast(format!("Canceled {client_order_id}"), true),
                Err(error) => app.fail(describe(&error)),
            }
        });
    };
    let cancel_all = move |symbol: String| {
        let client = app.client();
        leptos::task::spawn_local(async move {
            match trade::cancel_all(&*client, &symbol).await {
                Ok(reply) => app.toast(format!("Canceled {} orders in {symbol}", reply.data.orders.len()), true),
                Err(error) => app.fail(describe(&error)),
            }
        });
    };
    view! {
        <div class="card">
            <h2>"Open orders"</h2>
            {move || {
                let orders: Vec<_> = held.with(|held| held.orders.values().cloned().collect());
                if orders.is_empty() {
                    return view! { <p class="empty">"No open orders."</p> }.into_any();
                }
                let mut symbols: Vec<String> = orders.iter().map(|order| order.symbol.clone()).collect();
                symbols.dedup();
                view! {
                    <div class="row">
                        {symbols.into_iter().map(|symbol| {
                            let label = format!("Cancel all in {symbol}");
                            view! { <button class="small" disabled=move || !app.has_key() on:click=move |_| cancel_all(symbol.clone())>{label}</button> }
                        }).collect_view()}
                    </div>
                    <div class="table-wrap"><table>
                        <thead><tr>
                            <th>"Market"</th><th>"Side"</th><th class="r">"Price"</th><th class="r">"Open"</th>
                            <th class="r hide-narrow">"Filled"</th><th class="hide-narrow">"Kind"</th><th class="r hide-narrow">"Placed"</th><th></th>
                        </tr></thead>
                        <tbody>
                            {orders.into_iter().map(|order| {
                                let id = order.client_order_id;
                                let kind = format!("{} {}{}{}", order.order_type, order.tif, if order.post_only { " post-only" } else { "" }, if order.reduce_only { " reduce-only" } else { "" });
                                view! {
                                    <tr>
                                        <td><A href=format!("/market/{}", order.symbol)>{order.symbol.clone()}</A></td>
                                        <td class=if order.side == Side::Bid { "long" } else { "short" }>{order.side.to_string()}</td>
                                        <td class="r num">{fmt::price(order.price)}</td>
                                        <td class="r num">{fmt::count(order.size_remaining)}</td>
                                        <td class="r num hide-narrow">{fmt::count(order.size_filled)}</td>
                                        <td class="hide-narrow faint">{kind}</td>
                                        <td class="r hide-narrow faint">{fmt::time(order.place_ts_ms)}</td>
                                        <td class="r"><button class="small" disabled=move || !app.has_key() on:click=move |_| cancel(id)>"Cancel"</button></td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table></div>
                }.into_any()
            }}
        </div>
    }
}

#[component]
fn PnlChart(owner: Pubkey) -> impl IntoView {
    let app = use_app();
    let timeframe = RwSignal::new(Timeframe::Week);
    let line = RwSignal::new("total");
    let history = LocalResource::new(move || {
        let _ = app.revision.get();
        let client = app.client();
        async move { account::pnl_history(&*client, &owner).await.map(|reply| reply.data).map_err(|error| describe(&error)) }
    });
    let values = Signal::derive(move || {
        let Some(Ok(series)) = history.get() else { return Vec::new() };
        let Some(Timeseries { data, .. }) = series.into_iter().find(|series| series.timeframe == timeframe.get())
        else {
            return Vec::new();
        };
        let points = match line.get() {
            "value" => data.account_value_history,
            "trading" => data.trading_pnl_history,
            _ => data.total_pnl_history,
        };
        points.into_iter().map(|point| point.value).collect()
    });
    let frames =
        [("Day", Timeframe::Day), ("Week", Timeframe::Week), ("Month", Timeframe::Month), ("All", Timeframe::AllTime)];
    let lines = [("Total PnL", "total"), ("Trading PnL", "trading"), ("Account value", "value")];
    view! {
        <div class="card">
            <div class="card-head">
                <h2>"PnL"</h2>
                <div class="tabs">
                    {frames.into_iter().map(|(name, frame)| {
                        let pressed = frame.clone();
                        view! { <button class="small" aria-pressed=move || (timeframe.get() == pressed).to_string() on:click=move |_| timeframe.set(frame.clone())>{name}</button> }
                    }).collect_view()}
                </div>
            </div>
            <div class="tabs">
                {lines.into_iter().map(|(name, which)| view! {
                    <button class="small" aria-pressed=move || (line.get() == which).to_string() on:click=move |_| line.set(which)>{name}</button>
                }).collect_view()}
            </div>
            {move || match history.get() {
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                _ => view! { <Chart values=values label=fmt::usd /> }.into_any(),
            }}
            <p class="faint">"From stored history, up to the last closed half hour."</p>
        </div>
    }
}

#[component]
fn Movers(owner: Pubkey) -> impl IntoView {
    let app = use_app();
    let window = RwSignal::new(MoversWindow::Week);
    let movers = LocalResource::new(move || {
        let window = window.get();
        let client = app.client();
        async move {
            let query = MoversQuery { limit: Some(8), ..MoversQuery::new(window) };
            account::pnl_movers(&*client, &owner, &query)
                .await
                .map(|reply| reply.data)
                .map_err(|error| describe(&error))
        }
    });
    let windows = [
        ("6h", MoversWindow::SixHours),
        ("12h", MoversWindow::HalfDay),
        ("24h", MoversWindow::Day),
        ("7d", MoversWindow::Week),
    ];
    view! {
        <div class="card">
            <div class="card-head">
                <h2>"What moved PnL"</h2>
                <div class="tabs">
                    {windows.into_iter().map(|(name, choice)| {
                        let pressed = choice.clone();
                        view! { <button class="small" aria-pressed=move || (window.get() == pressed).to_string() on:click=move |_| window.set(choice.clone())>{name}</button> }
                    }).collect_view()}
                </div>
            </div>
            {move || match movers.get() {
                None => view! { <p class="empty">"Loading"</p> }.into_any(),
                Some(Err(error)) => view! { <p class="notice error">{error}</p> }.into_any(),
                Some(Ok(movers)) => view! {
                    <p class="muted">{format!("{} across {} markets", fmt::signed_usd(movers.total_pnl_change_usd), movers.total_markets)}</p>
                    <div class="table-wrap"><table><tbody>
                        {movers.entries.into_iter().map(|entry| view! {
                            <tr>
                                <td><A href=format!("/market/{}", entry.symbol)>{entry.symbol.clone()}</A></td>
                                <td class="r num">{fmt::signed_usd(entry.pnl_change_usd)}</td>
                                <td class="r faint">{if entry.resolved { "resolved" } else { "" }}</td>
                            </tr>
                        }).collect_view()}
                    </tbody></table></div>
                }.into_any(),
            }}
        </div>
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind_ {
    Fills,
    Transfers,
    Resolutions,
    Closed,
    Returns,
}

#[component]
fn History(owner: Pubkey) -> impl IntoView {
    let tab = RwSignal::new(Kind_::Fills);
    let tabs = [
        ("Fills", Kind_::Fills),
        ("Transfers", Kind_::Transfers),
        ("Resolutions", Kind_::Resolutions),
        ("Closed positions", Kind_::Closed),
        ("Collateral returns", Kind_::Returns),
    ];
    view! {
        <div class="card">
            <div class="card-head">
                <h2>"History"</h2>
                <div class="tabs">
                    {tabs.into_iter().map(|(name, kind)| view! {
                        <button class="small" aria-pressed=move || (tab.get() == kind).to_string() on:click=move |_| tab.set(kind)>{name}</button>
                    }).collect_view()}
                </div>
            </div>
            {move || match tab.get() {
                Kind_::Fills => view! { <Paged owner=owner kind=Kind_::Fills /> }.into_any(),
                Kind_::Transfers => view! { <Paged owner=owner kind=Kind_::Transfers /> }.into_any(),
                Kind_::Resolutions => view! { <Paged owner=owner kind=Kind_::Resolutions /> }.into_any(),
                Kind_::Closed => view! { <Paged owner=owner kind=Kind_::Closed /> }.into_any(),
                Kind_::Returns => view! { <Paged owner=owner kind=Kind_::Returns /> }.into_any(),
            }}
        </div>
    }
}

/// One history list, newest first; further pages are fetched by cursor.
#[derive(Clone)]
enum Rows {
    Fills(Vec<Fill>),
    Transfers(Vec<Transfer>),
    Resolutions(Vec<PositionResolution>),
    Closed(Vec<ClosedPosition>),
    Returns(Vec<CollateralReturn>),
}

#[component]
fn Paged(owner: Pubkey, kind: Kind_) -> impl IntoView {
    let app = use_app();
    let rows = RwSignal::new(None::<Rows>);
    let cursor = RwSignal::new(None::<String>);
    let done = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let load = move || {
        if busy.get_untracked() || done.get_untracked() {
            return;
        }
        busy.set(true);
        let client = app.client();
        let request = PageRequest { limit: Some(50), before_cursor: cursor.get_untracked(), at_or_before_seq: None };
        leptos::task::spawn_local(async move {
            fn take<T>(page: Page<T>, cursor: RwSignal<Option<String>>, done: RwSignal<bool>) -> Vec<T> {
                done.set(page.next_cursor.is_none());
                cursor.set(page.next_cursor);
                page.items
            }
            let result = match kind {
                Kind_::Fills => account::fills(&*client, &owner, &[], &request)
                    .await
                    .map(|r| Rows::Fills(take(r.data, cursor, done))),
                Kind_::Transfers => account::transfers(&*client, &owner, &request)
                    .await
                    .map(|r| Rows::Transfers(take(r.data, cursor, done))),
                Kind_::Resolutions => account::position_resolutions(&*client, &owner, &[], &request)
                    .await
                    .map(|r| Rows::Resolutions(take(r.data, cursor, done))),
                Kind_::Returns => account::collateral_returns(&*client, &owner, &[], &request)
                    .await
                    .map(|r| Rows::Returns(take(r.data, cursor, done))),
                Kind_::Closed => {
                    let query = ClosedQuery {
                        limit: Some(50),
                        before_cursor: request.before_cursor.clone(),
                        ..ClosedQuery::default()
                    };
                    account::closed_positions(&*client, &owner, &query)
                        .await
                        .map(|r| Rows::Closed(take(r.data, cursor, done)))
                }
            };
            match result {
                Ok(more) => rows
                    .try_update(|rows| {
                        *rows = Some(match (rows.take(), more) {
                            (Some(Rows::Fills(mut a)), Rows::Fills(b)) => {
                                a.extend(b);
                                Rows::Fills(a)
                            }
                            (Some(Rows::Transfers(mut a)), Rows::Transfers(b)) => {
                                a.extend(b);
                                Rows::Transfers(a)
                            }
                            (Some(Rows::Resolutions(mut a)), Rows::Resolutions(b)) => {
                                a.extend(b);
                                Rows::Resolutions(a)
                            }
                            (Some(Rows::Closed(mut a)), Rows::Closed(b)) => {
                                a.extend(b);
                                Rows::Closed(a)
                            }
                            (Some(Rows::Returns(mut a)), Rows::Returns(b)) => {
                                a.extend(b);
                                Rows::Returns(a)
                            }
                            (_, more) => more,
                        });
                    })
                    .unwrap_or(()),
                Err(error) => app.fail(describe(&error)),
            }
            let _ = busy.try_set(false);
        });
    };
    load();
    view! {
        {move || match rows.get() {
            None => view! { <p class="empty">"Loading"</p> }.into_any(),
            Some(rows) => table(rows),
        }}
        <Show when=move || !done.get()>
            <button disabled=move || busy.get() on:click=move |_| load()>{move || if busy.get() { "Loading" } else { "Load older" }}</button>
        </Show>
    }
}

fn table(rows: Rows) -> AnyView {
    let empty = || view! { <p class="empty">"Nothing yet."</p> }.into_any();
    match rows {
        Rows::Fills(rows) if !rows.is_empty() => view! {
            <div class="table-wrap"><table>
                <thead><tr><th>"Time"</th><th>"Market"</th><th>"Side"</th><th class="r">"Price"</th><th class="r">"Size"</th><th class="hide-narrow">"Liquidity"</th><th class="r">"Fee"</th><th class="r hide-narrow">"Realized"</th></tr></thead>
                <tbody>{rows.into_iter().map(|fill| view! {
                    <tr>
                        <td class="faint">{fmt::time(fill.trade_ts_ms)}</td>
                        <td>{fill.symbol.clone()}</td>
                        <td class=if fill.side == Side::Bid { "long" } else { "short" }>{fill.side.to_string()}</td>
                        <td class="r num">{fmt::price(fill.fill_price)}</td>
                        <td class="r num">{fmt::count(fill.fill_size)}</td>
                        <td class="hide-narrow faint">{fill.liquidity.to_string()}</td>
                        <td class="r num">{fmt::usd(fill.fee_usd)}</td>
                        <td class="r num hide-narrow">{fmt::signed_usd(fill.realized_pnl_usd)}</td>
                    </tr>
                }).collect_view()}</tbody>
            </table></div>
        }.into_any(),
        Rows::Transfers(rows) if !rows.is_empty() => view! {
            <div class="table-wrap"><table>
                <thead><tr><th>"Time"</th><th>"Kind"</th><th>"Status"</th><th class="r">"Amount"</th><th class="r">"Fee"</th><th class="hide-narrow">"Reason"</th></tr></thead>
                <tbody>{rows.into_iter().map(|transfer| view! {
                    <tr>
                        <td class="faint">{fmt::time(transfer.transfer_ts_ms)}</td>
                        <td>{transfer.kind.to_string()}</td>
                        <td>{transfer.status.to_string()}</td>
                        <td class="r num">{fmt::signed_usd(transfer.collateral_change_usd)}</td>
                        <td class="r num">{fmt::usd(transfer.fee_usd)}</td>
                        <td class="hide-narrow faint">{transfer.user_reward_reason.clone().unwrap_or_default()}</td>
                    </tr>
                }).collect_view()}</tbody>
            </table></div>
        }.into_any(),
        Rows::Resolutions(rows) if !rows.is_empty() => view! {
            <div class="table-wrap"><table>
                <thead><tr><th>"Time"</th><th>"Market"</th><th class="r">"Settled at"</th><th class="r">"Size"</th><th class="r">"Paid"</th><th class="r">"Realized"</th></tr></thead>
                <tbody>{rows.into_iter().map(|row| view! {
                    <tr>
                        <td class="faint">{fmt::time(row.position_resolution_ts_ms)}</td>
                        <td>{row.symbol.clone()}</td>
                        <td class="r num">{fmt::price(row.resolution)}</td>
                        <td class="r num">{fmt::count(row.size)}</td>
                        <td class="r num">{fmt::usd(row.collateral_change_usd)}</td>
                        <td class="r num">{fmt::signed_usd(row.realized_pnl_usd)}</td>
                    </tr>
                }).collect_view()}</tbody>
            </table></div>
        }.into_any(),
        Rows::Closed(rows) if !rows.is_empty() => view! {
            <div class="table-wrap"><table>
                <thead><tr><th>"Closed"</th><th>"Market"</th><th>"Side"</th><th class="r">"In"</th><th class="r">"Out"</th><th class="r">"Contracts"</th><th class="r">"Realized"</th></tr></thead>
                <tbody>{rows.into_iter().map(|row| view! {
                    <tr>
                        <td class="faint">{fmt::time(row.close_ts_ms)}</td>
                        <td>{row.symbol.clone()}{row.held_until_resolution.then(|| view! { <span class="badge">"held to resolution"</span> })}</td>
                        <td>{row.position_side.to_string()}</td>
                        <td class="r num">{fmt::price(row.average_increasing_price)}</td>
                        <td class="r num">{fmt::price(row.average_reducing_price)}</td>
                        <td class="r num">{fmt::count(row.cumulative_increasing_contracts)}</td>
                        <td class="r num">{fmt::signed_usd(row.realized_pnl_usd)}</td>
                    </tr>
                }).collect_view()}</tbody>
            </table></div>
        }.into_any(),
        Rows::Returns(rows) if !rows.is_empty() => view! {
            <div class="table-wrap"><table>
                <thead><tr><th>"Time"</th><th class="r">"Freed"</th><th class="r">"Size change"</th><th>"Markets"</th></tr></thead>
                <tbody>{rows.into_iter().map(|row| view! {
                    <tr>
                        <td class="faint">{fmt::time(row.collateral_return_ts_ms)}</td>
                        <td class="r num">{fmt::usd(row.collateral_change_usd)}</td>
                        <td class="r num">{fmt::count(row.position_size_change)}</td>
                        <td class="faint">{row.positions.iter().map(|p| p.symbol.clone()).collect::<Vec<_>>().join(", ")}</td>
                    </tr>
                }).collect_view()}</tbody>
            </table></div>
        }.into_any(),
        _ => empty(),
    }
}
