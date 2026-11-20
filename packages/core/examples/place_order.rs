//! Builds a signed post-only bid one tick below the best bid and prints the request. With
//! `--send`, places it and then cancels it. Reads `PASCAL_OWNER_PUBLIC_KEY` and
//! `PASCAL_TRADING_PRIVATE_KEY` from the environment.
//!
//! `cargo run -p df-pascal-core --example place_order -- SYMBOL [--send]`

use df_pascal_core::client::native::{Native, clock};
use df_pascal_core::trade::{self, CancelBy, OrderRequest};
use df_pascal_core::utils::{Round, Side};
use df_pascal_core::{Client, Keypair, Pubkey, Trader, markets};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let symbol = args.iter().find(|arg| !arg.starts_with("--")).ok_or("usage: place_order SYMBOL [--send]")?;
    let send = args.iter().any(|arg| arg == "--send");
    let owner = Pubkey::parse(&std::env::var("PASCAL_OWNER_PUBLIC_KEY")?)?;
    let key = Keypair::parse(&std::env::var("PASCAL_TRADING_PRIVATE_KEY")?)?;
    let client = Client::new(Native::new(), clock, Trader::new(owner, key));
    client.sync_clock().await?;

    let book = markets::book(&client, symbol).await?.data;
    let market =
        markets::search(&client, &markets::Search { symbols: vec![symbol.clone()], ..Default::default() }).await?.data;
    let spec = market.first().ok_or("unknown symbol")?.spec();
    let best = book.best_bid().map_or(spec.tick_size_min, |level| level.price);
    let price = spec.round(spec.step(best, false), Round::Down).max(spec.tick_size_min);
    let client_order_id = trade::client_order_id(client.now_ms(), 0);
    let order = OrderRequest::limit(symbol.clone(), Side::Bid, price, 1, client_order_id).post_only();

    let signed = trade::sign_place(&client, &order)?;
    println!("{}", serde_json::to_string_pretty(&[&signed])?);
    if !send {
        println!("\nDry run. Add --send to place it.");
        return Ok(());
    }
    let placed = client.send(&signed).await?;
    println!("placed {} at seq {:?}", placed.data.order.id, placed.context.seq);
    let canceled = trade::cancel(&client, CancelBy::ClientOrderId(client_order_id)).await?;
    println!("canceled; {} left open", canceled.data.size_remaining);
    Ok(())
}
