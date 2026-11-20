//! Market data: market listings and search, order books, candles, public trades, top holders, server
//! time, usernames and referral code lookup.

pub mod book;
pub mod candles;
pub mod holders;
pub mod info;
pub mod system;
pub mod trades;

pub use book::{Level, OrderBook, book, books};
pub use candles::{
    Candle, MarkPriceCandle, MarkPriceCandles, Range, mark_price_candles, mark_price_candles_batch, trade_candles,
};
pub use holders::{Holder, Holders, top_holders};
pub use info::{
    DisplayAttributes, GameLine, GamePage, HomeAwayTeams, Market, MarketColors, MarketKind, MarketResolution,
    MarketSpec, MarketStats, MarketType, OutcomeLabel, OutcomeLabels, RangeAttributes, Reference, ResolvedQuery,
    Search, Sport, StatsSnapshot, TextSearch, ThemeColors, Topic, event_code, list, resolved, search, text_search,
};
pub use system::{ReferralCodeInfo, referral_code, server_time, usernames};
pub use trades::{PublicTrade, RecentTrades, recent, trades};
