//! Prints the book, recent trades and hourly candles of the busiest market, or SYMBOL, from
//! Pascal's production read API. `cargo run -p df-pascal-core --example market_data [SYMBOL]`

use df_pascal_core::Client;
use df_pascal_core::client::native::{Native, clock};
use df_pascal_core::markets::{self, Range};
use df_pascal_core::utils::{Interval, PageRequest};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::reader(Native::new(), clock);
    let offset = client.sync_clock().await?;
    let markets = markets::list(&client).await?;
    println!("{} live markets at seq {:?}; clock offset {offset} ms", markets.data.len(), markets.context.seq);

    let symbol = match std::env::args().nth(1) {
        Some(symbol) => symbol,
        None => markets
            .data
            .iter()
            .max_by_key(|market| market.volume_24h())
            .map(|market| market.symbol.clone())
            .ok_or("no markets")?,
    };
    let market = markets.data.iter().find(|market| market.symbol == symbol).ok_or("unknown symbol")?;
    let (long, short) = market.outcome_names();
    println!("{}  ({long} / {short})", market.title());
    println!(
        "mark {:?}  24h volume {} contracts  fee {}",
        market.mark_price,
        market.volume_24h(),
        market.taker_fee_rate
    );

    let book = markets::book(&client, &symbol).await?.data;
    println!("\n{:>10} {:>10} | {:<10} {:<10}", "bid size", "bid", "ask", "ask size");
    for row in 0..5 {
        let bid = book
            .bids
            .get(row)
            .map_or(("".into(), "".into()), |level| (level.size.to_string(), level.price.to_fixed(6)));
        let ask = book
            .asks
            .get(row)
            .map_or(("".into(), "".into()), |level| (level.price.to_fixed(6), level.size.to_string()));
        println!("{:>10} {:>10} | {:<10} {:<10}", bid.0, bid.1, ask.0, ask.1);
    }

    let trades = markets::trades(&client, &[&symbol], &PageRequest::limit(5)).await?.data;
    println!("\nlast trades");
    for trade in &trades.items {
        println!("  {} {} {} @ {}", trade.trade_ts_ms, trade.taker_side, trade.size, trade.price.to_fixed(6));
    }

    let candles = markets::trade_candles(&client, &symbol, &Range::last(Interval::H1, client.now_ms(), 6)).await?.data;
    println!("\nhourly candles");
    for candle in candles {
        println!(
            "  {} o {} h {} l {} c {} v {}",
            candle.time, candle.open, candle.high, candle.low, candle.close, candle.volume
        );
    }
    Ok(())
}
