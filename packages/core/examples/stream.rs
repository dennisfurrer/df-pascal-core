//! Subscribes to the book and trades of the busiest market, or SYMBOL, for 30 seconds and keeps
//! a local copy of the book.
//! `cargo run -p df-pascal-core --example stream [SYMBOL]`

use std::time::Duration;

use df_pascal_core::client::native::{Feed, Native, clock};
use df_pascal_core::live::store::Book;
use df_pascal_core::live::{Channel, Data, Event, Frame, Session};
use df_pascal_core::{Client, markets};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::reader(Native::new(), clock);
    let symbol = match std::env::args().nth(1) {
        Some(symbol) => symbol,
        None => {
            let markets = markets::list(&client).await?.data;
            markets
                .iter()
                .max_by_key(|market| market.volume_24h())
                .map(|market| market.symbol.clone())
                .ok_or("no markets")?
        }
    };
    let mut feed = Feed::start(Session::new(client.deployment().data_ws_url.clone()));
    feed.subscribe(Channel::book(&symbol)).await?;
    feed.subscribe(Channel::trades(&symbol)).await?;
    let mut book = Book::default();
    let stop = tokio::time::Instant::now() + Duration::from_secs(30);
    while let Ok(Some(event)) = tokio::time::timeout_at(stop, feed.next()).await {
        match event {
            Event::Frame(frame) => match *frame {
                Frame::Message { kind, data: Data::Book(update), context, .. } => {
                    book.apply(kind, &update, context.seq);
                    let bid =
                        book.best_bid().map(|level| format!("{} x {}", level.size, level.price)).unwrap_or_default();
                    let ask =
                        book.best_ask().map(|level| format!("{} @ {}", level.size, level.price)).unwrap_or_default();
                    println!("book  {bid:>16} | {ask:<16}  seq {:?}", context.seq);
                }
                Frame::Message { data: Data::Trades(trades), .. } => {
                    for trade in trades {
                        println!("trade {} {} @ {}", trade.taker_side, trade.size, trade.price);
                    }
                }
                other => println!("{other:?}"),
            },
            Event::Opened => println!("open"),
            Event::Closed { reason, retry } => println!("closed: {reason}; again in {} ms", retry.after_ms),
            Event::Unreadable(bad) => println!("unreadable: {bad}"),
        }
    }
    Ok(())
}
