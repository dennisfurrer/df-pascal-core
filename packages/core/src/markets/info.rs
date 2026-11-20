//! Market listings: specs (fees and tick grid), display attributes, prices, stats and resolutions.
//! Markets can be listed live, recently resolved, by symbol, event or game, or by text search.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::{ApiError, Call, Reply, Transport, read};
use crate::utils::dec::{Dec, Round};
use crate::utils::lenient;
use crate::utils::order::{Lookback, MarketSort, MarketStatus, ResolvedOrderBy, SearchOrderBy};
use crate::utils::price;

/// The event code of a symbol: the part before the `.` (`NYCMAYOR_25NOV04` of
/// `NYCMAYOR_25NOV04.MAMDANI`). Markets sharing it resolve on the same event.
#[must_use]
pub fn event_code(symbol: &str) -> &str {
    symbol.split_once('.').map_or(symbol, |(event, _)| event)
}

/// A market's fee rates and tick grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketSpec {
    pub symbol: String,
    /// Takers pay `size * price * (1 - price) * taker_fee_rate`.
    pub taker_fee_rate: Dec,
    /// Makers receive this share of the taker fee.
    pub maker_rebate_share: Dec,
    pub tick_size_min: Dec,
    pub tick_sig_figs: u32,
}

impl MarketSpec {
    /// The tick size at `price`.
    #[must_use]
    pub fn tick_size(&self, price: Dec) -> Dec {
        price::tick_size(price, self.tick_size_min, self.tick_sig_figs)
    }

    #[must_use]
    pub fn is_on_tick(&self, price: Dec) -> bool {
        price::is_on_tick(price, self.tick_size_min, self.tick_sig_figs)
    }

    /// Rounds `price` to the tick grid.
    #[must_use]
    pub fn round(&self, price: Dec, round: Round) -> Dec {
        price::round_to_tick(price, self.tick_size_min, self.tick_sig_figs, round)
    }

    /// One tick up or down from `price`.
    #[must_use]
    pub fn step(&self, price: Dec, up: bool) -> Dec {
        price::step(price, self.tick_size_min, self.tick_sig_figs, up)
    }

    #[must_use]
    pub fn taker_fee(&self, size: u64, price: Dec) -> Dec {
        price::taker_fee(size, price, self.taker_fee_rate)
    }

    #[must_use]
    pub fn maker_rebate(&self, size: u64, price: Dec) -> Dec {
        price::maker_rebate(size, price, self.taker_fee_rate, self.maker_rebate_share)
    }
}

/// A market from the read API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Market {
    pub symbol: String,
    pub taker_fee_rate: Dec,
    pub maker_rebate_share: Dec,
    pub tick_size_min: Dec,
    pub tick_sig_figs: u32,
    /// True when the event's markets are mutually exclusive and collectively exhaustive, so
    /// collateral can be returned ([`crate::trade::collateral`]).
    #[serde(default)]
    pub is_mece: bool,
    #[serde(default)]
    pub display_attributes: DisplayAttributes,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub listing_ts_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<MarketStats>,
    /// The median of whichever of best bid, best ask and last trade there are.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_price: Option<Dec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_bid: Option<Dec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_ask: Option<Dec>,
    /// Deprecated by Pascal: equal to `long_interest`.
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub open_interest: u64,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub long_interest: Option<u64>,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub short_interest: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<MarketResolution>,
}

impl Market {
    #[must_use]
    pub fn spec(&self) -> MarketSpec {
        MarketSpec {
            symbol: self.symbol.clone(),
            taker_fee_rate: self.taker_fee_rate,
            maker_rebate_share: self.maker_rebate_share,
            tick_size_min: self.tick_size_min,
            tick_sig_figs: self.tick_sig_figs,
        }
    }

    #[must_use]
    pub fn event_code(&self) -> &str {
        event_code(&self.symbol)
    }

    #[must_use]
    pub const fn is_resolved(&self) -> bool {
        self.resolution.is_some()
    }

    /// `event_description: market_description`, or the event description or symbol when parts
    /// are empty.
    #[must_use]
    pub fn title(&self) -> String {
        let attributes = &self.display_attributes;
        match (attributes.event_description.is_empty(), attributes.market_description.is_empty()) {
            (false, false) => format!("{}: {}", attributes.event_description, attributes.market_description),
            (false, true) => attributes.event_description.clone(),
            _ => self.symbol.clone(),
        }
    }

    /// The names of the long and short contracts: Yes and No unless the market says otherwise.
    #[must_use]
    pub fn outcome_names(&self) -> (String, String) {
        match &self.display_attributes.outcome_labels {
            Some(labels) => (labels.long.extended.clone(), labels.short.extended.clone()),
            None => ("Yes".to_owned(), "No".to_owned()),
        }
    }

    /// Contracts traded by takers over the last 24 hours.
    #[must_use]
    pub fn volume_24h(&self) -> u64 {
        self.stats.as_ref().map_or(0, |stats| stats.last_24h.taker_buy_volume + stats.last_24h.taker_sell_volume)
    }

    /// USD traded over the last 24 hours.
    #[must_use]
    pub fn notional_24h(&self) -> Dec {
        self.stats.as_ref().map_or(Dec::ZERO, |stats| {
            stats
                .last_24h
                .taker_buy_volume_notional
                .checked_add(stats.last_24h.taker_sell_volume_notional)
                .unwrap_or(Dec::ZERO)
        })
    }

    /// Mark price change over the last 24 hours, if both prices are known.
    #[must_use]
    pub fn change_24h(&self) -> Option<Dec> {
        let start = self.stats.as_ref()?.last_24h.mark_price_start?;
        self.mark_price?.checked_sub(start)
    }
}

/// Display metadata. Fields not modelled here are kept in `other`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DisplayAttributes {
    #[serde(default)]
    pub event_description: String,
    #[serde(default)]
    pub market_description: String,
    /// The label when the UI is reversed; `None` when reversing is not offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome_labels: Option<OutcomeLabels>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colors: Option<MarketColors>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<Topic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_page: Option<GamePage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_type: Option<MarketType>,
    #[serde(default, deserialize_with = "lenient::list")]
    pub tags: Vec<String>,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub expected_event_start_time_ms: Option<u64>,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub expected_resolution_time_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<MarketSort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scalar: Option<RangeAttributes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolves_at_expected_time: Option<bool>,
    /// Fields not modelled here, such as `range`.
    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeLabels {
    pub long: OutcomeLabel,
    pub short: OutcomeLabel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeLabel {
    /// Up to 20 characters.
    #[serde(default)]
    pub extended: String,
    /// Up to 10 characters.
    #[serde(default)]
    pub abbreviated: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketColors {
    pub market: ThemeColors,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse: Option<ThemeColors>,
}

/// `#RRGGBB` colors for the light and dark themes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeColors {
    pub light: String,
    pub dark: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub id: String,
    #[serde(default)]
    pub description: String,
}

/// A market's place on a game page: game, section (game line) and row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GamePage {
    pub game_id: String,
    #[serde(default)]
    pub game_description: String,
    #[serde(default)]
    pub section_id: String,
    #[serde(default)]
    pub section_description: String,
    #[serde(default)]
    pub section_display_order: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_display_order: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_description: Option<String>,
    /// A spread, signed from the subject's side (`-1.5`), or a total (`2.5`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

crate::utils::order::wire_enum! {
    MarketKind { Sports = "sports", CompanyAction = "company_action" }
}

crate::utils::order::wire_enum! {
    Sport {
        Soccer = "soccer",
        Baseball = "baseball",
        LeagueOfLegends = "league_of_legends",
        CounterStrike = "counter_strike",
        Dota2 = "dota_2",
        Valorant = "valorant",
        Tennis = "tennis",
        Football = "football",
        Basketball = "basketball",
        Hockey = "hockey",
    }
}

/// Structured market type. The fields present depend on `kind` and `sport`: a soccer market
/// has `period`, `teams` and `market`; a tennis market adds `match_format`; esports have
/// `series_format`; a company action has the company fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketType {
    pub kind: MarketKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sport: Option<Sport>,
    /// `reg_time`, `first_half`, `second_half`, `full_game` or `full_match`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub period: Option<String>,
    /// `best_of_one`, `best_of_three` or `best_of_five`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_format: Option<String>,
    /// `best_of_three` or `best_of_five`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<HomeAwayTeams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market: Option<GameLine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, deserialize_with = "lenient::opt_u64s", serialize_with = "lenient::opt_u64_text")]
    pub announcement_deadline_time_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeAwayTeams {
    pub home_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_short_name: Option<String>,
    #[serde(default)]
    pub home_subject_id: String,
    pub away_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub away_short_name: Option<String>,
    #[serde(default)]
    pub away_subject_id: String,
}

/// One game line: `moneyline` (an `outcome`), `spread` and handicaps (a `team` and a `line`),
/// totals (a `line`), `map_winner` (a `map_number` and an `outcome`), `both_teams_to_score`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameLine {
    #[serde(rename = "type")]
    pub kind: String,
    /// `home`, `away`, or `draw` where a draw is possible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    /// `home` or `away`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_number: Option<u32>,
}

/// A scalar market's range: `lower` to `upper`, scaled by `scale` decimal places, displayed
/// with `prefix` and `suffix`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeAttributes {
    #[serde(deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub upper: i64,
    #[serde(deserialize_with = "lenient::i64s", serialize_with = "lenient::i64_text")]
    pub lower: i64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub scale: u64,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub suffix: String,
}

/// The market on another venue that this market resolves against. `polygon` names a Polygon condition;
/// `polymarket` (deprecated) also carries a slug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub kind: String,
    #[serde(default)]
    pub condition_id: String,
    /// If this token wins, the market resolves to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_outcome_token_id: Option<String>,
    /// If this token wins, the market resolves to 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse_outcome_token_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_slug: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketResolution {
    /// The settlement price.
    pub price: Dec,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub seq: u64,
    #[serde(deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub market_resolution_ts_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketStats {
    pub last_24h: StatsSnapshot,
    pub all_time: StatsSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatsSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark_price_start: Option<Dec>,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub taker_buy_volume: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub taker_sell_volume: u64,
    #[serde(default)]
    pub taker_buy_volume_notional: Dec,
    #[serde(default)]
    pub taker_sell_volume_notional: Dec,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub taker_buy_trade_count: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub taker_sell_trade_count: u64,
    #[serde(default, deserialize_with = "lenient::u64s", serialize_with = "lenient::u64_text")]
    pub window_end_ts_ms_exclusive: u64,
}

/// Live markets, by symbol.
pub async fn list(client: &impl Transport) -> Result<Reply<Vec<Market>>, ApiError> {
    read(client, Call::get("/api/v1/markets")).await
}

/// Which resolved markets to list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedQuery {
    /// 4h, 24h (default) or 3d.
    pub lookback: Option<Lookback>,
    pub order_by: Option<ResolvedOrderBy>,
    /// 50 (default), 100 or 200.
    pub limit: Option<u32>,
}

/// Markets resolved within the lookback.
pub async fn resolved(client: &impl Transport, query: &ResolvedQuery) -> Result<Reply<Vec<Market>>, ApiError> {
    let call = Call::get("/api/v1/markets/resolved")
        .maybe("lookback", query.lookback.as_ref())
        .maybe("order_by", query.order_by.as_ref())
        .maybe("limit", query.limit);
    read(client, call).await
}

/// Exact lookup by symbols, event codes and game ids. Returns the union, including resolved
/// markets.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Search {
    /// Up to 500.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<String>,
    /// Up to 50.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<String>,
    /// Up to 50.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub games: Vec<String>,
}

pub async fn search(client: &impl Transport, search: &Search) -> Result<Reply<Vec<Market>>, ApiError> {
    let body = serde_json::to_string(search).unwrap_or_default();
    read(client, Call::search("/api/v1/markets/search", body)).await
}

/// Fuzzy search over symbols, titles, outcome names, sections and tags. Rate limit: 60 requests
/// a minute per IP.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct TextSearch {
    /// 1 to 32 characters once trimmed.
    pub query: String,
    /// Only markets with all of these tags (up to 5).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Excludes markets with any of these tags (up to 5).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exclude_tags: Vec<String>,
    /// Excludes markets that reference another venue.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub exclude_referenced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MarketStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<SearchOrderBy>,
    /// Up to 100; 50 by default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

impl TextSearch {
    #[must_use]
    pub fn new(query: impl Into<String>) -> TextSearch {
        TextSearch { query: query.into(), ..TextSearch::default() }
    }
}

pub async fn text_search(client: &impl Transport, search: &TextSearch) -> Result<Reply<Vec<Market>>, ApiError> {
    let body = serde_json::to_string(search).unwrap_or_default();
    read(client, Call::search("/api/v1/markets/text-search", body)).await
}
