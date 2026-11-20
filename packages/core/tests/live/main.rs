//! Live tests against Pascal's production read API and market data websocket. Each read and
//! channel is decoded into its type; nothing is signed or sent. Run manually:
//! `cargo test -p df-pascal-core --test live -- --ignored --test-threads 1`.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use df_pascal_core::account::{self, ClosedQuery, MoversQuery, StateQuery, TraderSearch};
use df_pascal_core::client::native::{Feed, Native, clock};
use df_pascal_core::live::{Channel, Data, Event, Frame, Session};
use df_pascal_core::markets::{self, Range, ResolvedQuery, Search, TextSearch};
use df_pascal_core::utils::{Interval, MoversWindow, PageRequest, Pubkey};
use df_pascal_core::{Client, Deployment, transfer};

fn client() -> Client<Native> {
    Client::reader(Native::new(), clock)
}

/// The busiest live market and the maker and taker of its latest trade.
async fn subjects(client: &Client<Native>) -> (String, Pubkey, Pubkey) {
    let markets = markets::list(client).await.expect("markets").data;
    assert!(!markets.is_empty());
    let busiest = markets.iter().max_by_key(|market| market.volume_24h()).expect("a market");
    let trades = markets::trades(client, &[&busiest.symbol], &PageRequest::limit(5)).await.expect("trades").data;
    let trade = trades.items.first().expect("a trade");
    (busiest.symbol.clone(), trade.maker, trade.taker)
}

#[tokio::test]
#[ignore = "live"]
async fn every_market_read_decodes() {
    let client = client();
    let offset = client.sync_clock().await.expect("time");
    assert!(offset.abs() < 60_000, "clock offset {offset}");
    let (symbol, maker, _) = subjects(&client).await;
    let book = markets::book(&client, &symbol).await.expect("book").data;
    assert!(book.best_bid().is_some() || book.best_ask().is_some());
    let now = client.now_ms();
    let candles = markets::trade_candles(&client, &symbol, &Range::last(Interval::H1, now, 24)).await.expect("candles");
    assert!(candles.data.len() <= 24);
    markets::mark_price_candles(&client, &symbol, &Range::last(Interval::H1, now, 24)).await.expect("mark candles");
    markets::mark_price_candles_batch(&client, &[&symbol], &Range::last(Interval::D1, now, 5))
        .await
        .expect("batch candles");
    markets::recent(&client, &[&symbol], Some(5)).await.expect("recent trades");
    markets::top_holders(&client, &[&symbol], Some(3)).await.expect("holders");
    let event = markets::event_code(&symbol).to_owned();
    let found = markets::search(&client, &Search { events: vec![event], ..Search::default() }).await.expect("search");
    assert!(found.data.iter().any(|market| market.symbol == symbol));
    markets::text_search(&client, &TextSearch::new("election")).await.expect("text search");
    let resolved = markets::resolved(&client, &ResolvedQuery { limit: Some(50), ..ResolvedQuery::default() })
        .await
        .expect("resolved");
    assert!(resolved.data.iter().all(|market| market.is_resolved()));
    markets::referral_code(&client, "abcdef").await.expect("referral code");
    markets::usernames(&client, &[maker]).await.expect("usernames");
}

#[tokio::test]
#[ignore = "live"]
async fn every_account_read_decodes() {
    let client = client();
    let (symbol, owner, _) = subjects(&client).await;
    account::state(&client, &owner, &StateQuery::default()).await.expect("state");
    account::state(&client, &owner, &StateQuery { symbol: Some(symbol.clone()), exclude_orders: true })
        .await
        .expect("filtered state");
    let keys = account::trading_keys(&client, &owner).await.expect("keys").data;
    if let Some(key) = keys.first() {
        account::trading_key_accounts(&client, &key.trading_key).await.expect("key accounts");
    }
    account::builder_stats(&client, &owner).await.expect("builder stats");
    account::bio(&client, &owner).await.expect("bio");
    account::taker_discount_terms(&client, &owner).await.expect("terms");
    account::referrals(&client, &owner).await.expect("referrals");
    account::ranks(&client, &owner, None).await.expect("ranks");
    account::search_traders(&client, &TraderSearch { query: "pascal".to_owned(), ..TraderSearch::default() })
        .await
        .expect("search");
    transfer::deposit_address(&client, &owner).await.expect("deposit address");
    let fills = account::fills(&client, &owner, &[], &PageRequest::limit(20)).await.expect("fills").data;
    if let Some(next) = fills.next(&PageRequest::limit(20)) {
        account::fills(&client, &owner, &[], &next).await.expect("fills page 2");
    }
    account::transfers(&client, &owner, &PageRequest::limit(20)).await.expect("transfers");
    account::position_resolutions(&client, &owner, &[], &PageRequest::limit(20)).await.expect("resolutions");
    account::collateral_returns(&client, &owner, &[], &PageRequest::limit(20)).await.expect("collateral returns");
    account::closed_positions(&client, &owner, &ClosedQuery { limit: Some(20), ..ClosedQuery::default() })
        .await
        .expect("closed");
    account::pnl_history(&client, &owner).await.expect("pnl history");
    account::pnl_movers(&client, &owner, &MoversQuery::new(MoversWindow::Week)).await.expect("movers");
}

#[tokio::test]
#[ignore = "live"]
async fn the_market_channels_stream_and_decode() {
    let client = client();
    let (symbol, owner, _) = subjects(&client).await;
    let mut feed = Feed::start(Session::new(Deployment::prod().data_ws_url));
    for channel in [
        Channel::book(&symbol),
        Channel::trades(&symbol),
        Channel::AllTrades,
        Channel::candles(&symbol, Interval::M1),
        Channel::mark_price_candles(&symbol, Interval::M1),
        Channel::account(owner),
        Channel::MarketUpdate,
    ] {
        feed.subscribe(channel).await.expect("subscribe");
    }
    let mut seen = std::collections::BTreeSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    while !(seen.contains("book") && seen.contains("account") && seen.contains("trades")) {
        let event = tokio::time::timeout_at(deadline, feed.next()).await.expect("in time").expect("an event");
        match event {
            Event::Frame(frame) => {
                if let Frame::Message { data, .. } = *frame {
                    seen.insert(match data {
                        Data::Book(_) => "book",
                        Data::Trades(_) => "trades",
                        Data::Candles(_) => "candles",
                        Data::MarkPriceCandles(_) => "mark",
                        Data::Account(_) => "account",
                        Data::Markets(_) => "markets",
                    });
                }
            }
            Event::Unreadable(bad) => panic!("{bad}"),
            Event::Opened | Event::Closed { .. } => {}
        }
    }
    assert!(seen.contains("book") && seen.contains("account"), "{seen:?}");
}
