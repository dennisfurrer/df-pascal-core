//! Markets grouped into events or games, as the markets page and the rail show them.

use df_pascal_core::Dec;
use df_pascal_core::markets::{Market, Sport};
use df_pascal_core::utils::Round;

use crate::icons::Icon;

#[derive(Clone)]
pub struct EventGroup {
    pub title: String,
    pub category: String,
    pub icon: Icon,
    pub teams: Option<Teams>,
    pub starts_ms: Option<u64>,
    pub resolves_ms: Option<u64>,
    pub volume: Dec,
    /// The headline line of a game (the lowest section order), or every market of an event.
    pub markets: Vec<Market>,
    /// Markets in the game's other lines (spreads, totals).
    pub other_lines: usize,
    pub all: usize,
}

#[derive(Clone)]
pub struct Teams {
    pub home: String,
    pub home_short: String,
    pub away: String,
    pub away_short: String,
}

/// The resolution price once resolved, otherwise the mark price.
pub fn chance(market: &Market) -> Option<Dec> {
    market.resolution.as_ref().map(|resolution| resolution.price).or(market.mark_price)
}

/// A chance from 0 to 1 as a whole percent, for bar widths.
pub fn percent(value: Option<Dec>) -> i128 {
    value.and_then(|value| value.units_at(2, Round::Nearest)).unwrap_or(0).clamp(0, 100)
}

/// The outcome's own name within its event.
pub fn outcome_name(market: &Market) -> String {
    let name = &market.display_attributes.market_description;
    if name.is_empty() { market.symbol.clone() } else { name.clone() }
}

fn group_key(market: &Market) -> String {
    match &market.display_attributes.game_page {
        Some(page) if !page.game_id.is_empty() => format!("game:{}", page.game_id),
        _ => market.event_code().to_owned(),
    }
}

fn section_order(market: &Market) -> i64 {
    market.display_attributes.game_page.as_ref().map_or(0, |page| page.section_display_order)
}

fn row_order(market: &Market) -> Option<i64> {
    market.display_attributes.game_page.as_ref().and_then(|page| page.row_display_order)
}

/// The home and away teams of a sports market.
pub fn teams(market: &Market) -> Option<Teams> {
    let kind = market.display_attributes.market_type.as_ref()?;
    let teams = kind.teams.as_ref()?;
    Some(Teams {
        home: teams.home_name.clone(),
        home_short: teams.home_short_name.clone().unwrap_or_else(|| monogram(&teams.home_name)),
        away: teams.away_name.clone(),
        away_short: teams.away_short_name.clone().unwrap_or_else(|| monogram(&teams.away_name)),
    })
}

/// The other outcomes of the same event, in display order.
pub fn siblings(market: &Market, all: &[Market]) -> Vec<Market> {
    let code = market.event_code();
    let mut found: Vec<Market> = all.iter().filter(|other| other.event_code() == code).cloned().collect();
    if found.iter().all(|other| row_order(other).is_some()) {
        found.sort_by_key(row_order);
    } else {
        found.sort_by_key(|other| std::cmp::Reverse(chance(other)));
    }
    found
}

/// The icon and label for a market's category, from its sport and tags.
pub fn category(market: &Market) -> (Icon, String) {
    let attributes = &market.display_attributes;
    let tags = &attributes.tags;
    let has = |name: &str| tags.iter().any(|tag| tag == name);
    let sport = attributes.market_type.as_ref().and_then(|kind| kind.sport.clone());
    let icon = match sport {
        Some(Sport::Soccer) => Icon::Goal,
        Some(Sport::Dota2 | Sport::LeagueOfLegends | Sport::CounterStrike | Sport::Valorant) => Icon::Gamepad2,
        Some(Sport::Tennis) => Icon::Volleyball,
        Some(_) => Icon::Trophy,
        None if has("Esports") => Icon::Gamepad2,
        None if has("Soccer") => Icon::Goal,
        None if has("Sports") => Icon::Trophy,
        None if has("Elections") || has("Midterms") => Icon::Vote,
        None if has("Geopolitics") => Icon::Earth,
        None if has("Politics") => Icon::Landmark,
        None if has("Crypto") => Icon::Bitcoin,
        None if has("AI") => Icon::BrainCircuit,
        None if has("Tech") => Icon::Cpu,
        None if has("Pre-IPO") || has("IPO") || has("Companies") => Icon::Building2,
        None if has("Commodities") => Icon::Fuel,
        None if has("Macro") => Icon::Banknote,
        None if has("Finance") => Icon::ChartCandlestick,
        None if has("Culture") => Icon::Clapperboard,
        None if has("Prediction Markets") => Icon::ChartLine,
        None => Icon::Sparkles,
    };
    let label = match (tags.first(), tags.get(1)) {
        (Some(first), Some(second)) => format!("{first}, {second}"),
        (Some(first), None) => first.clone(),
        _ => attributes.topic.as_ref().map(|topic| topic.description.clone()).unwrap_or_default(),
    };
    (icon, label)
}

/// Groups `markets` by game (when they have a game page) or by event, busiest first.
pub fn group(markets: Vec<Market>) -> Vec<EventGroup> {
    let mut buckets: Vec<(String, Vec<Market>)> = Vec::new();
    for market in markets {
        let key = group_key(&market);
        match buckets.iter_mut().find(|(known, _)| *known == key) {
            Some((_, list)) => list.push(market),
            None => buckets.push((key, vec![market])),
        }
    }
    let mut groups: Vec<EventGroup> = buckets.into_iter().filter_map(|(_, list)| build(list)).collect();
    groups.sort_by_key(|group| std::cmp::Reverse(group.volume));
    groups
}

fn build(mut list: Vec<Market>) -> Option<EventGroup> {
    let volume = list.iter().fold(Dec::ZERO, |sum, market| sum.checked_add(market.notional_24h()).unwrap_or(sum));
    let all = list.len();
    let head = list.iter().map(section_order).min().unwrap_or(0);
    let headline_event = list.iter().find(|market| section_order(market) == head)?.event_code().to_owned();
    let (mut markets, rest): (Vec<Market>, Vec<Market>) =
        list.drain(..).partition(|market| market.event_code() == headline_event);
    if markets.iter().all(|market| row_order(market).is_some()) {
        markets.sort_by_key(row_order);
    } else {
        markets.sort_by_key(|market| std::cmp::Reverse(chance(market)));
    }
    let first = markets.first()?;
    let attributes = &first.display_attributes;
    let title = match &attributes.game_page {
        Some(page) if !page.game_description.is_empty() => page.game_description.clone(),
        _ if !attributes.event_description.is_empty() => attributes.event_description.clone(),
        _ => first.title(),
    };
    let teams = teams(first);
    let (icon, category) = category(first);
    Some(EventGroup {
        title,
        category,
        icon,
        teams,
        starts_ms: attributes.expected_event_start_time_ms,
        resolves_ms: attributes.expected_resolution_time_ms,
        volume,
        other_lines: rest.len(),
        all,
        markets,
    })
}

/// A short mark for a name: a number in it ("Dec 31 2027" is "31", "[$1T - $1.5T]" is "1T"),
/// otherwise up to three capitals ("Real Madrid" is "RM", "Arsenal" is "ARS").
pub fn monogram(name: &str) -> String {
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|word| !word.is_empty()).collect();
    if let Some(number) = words.iter().find(|word| word.starts_with(|c: char| c.is_ascii_digit())) {
        return number.chars().take(3).collect::<String>().to_uppercase();
    }
    match words.as_slice() {
        [] => "?".to_owned(),
        [one] => one.chars().take(3).collect::<String>().to_uppercase(),
        many => many.iter().take(2).filter_map(|word| word.chars().next()).collect::<String>().to_uppercase(),
    }
}

const PALETTE: [&str; 8] = ["#ec7a4f", "#db9c67", "#e0b44c", "#6fb8a4", "#7f9cf5", "#b48ead", "#d0707d", "#8fb0c9"];

/// The outcome's badge: its short name and a colour, from the market's theme colours when it has
/// them, otherwise picked from the name.
pub fn badge(market: &Market, teams: Option<&Teams>) -> (String, String) {
    let name = outcome_name(market);
    let short = match teams {
        Some(teams) if name == teams.home => teams.home_short.clone(),
        Some(teams) if name == teams.away => teams.away_short.clone(),
        _ => monogram(&name),
    };
    let colour =
        market.display_attributes.colors.as_ref().map(|colors| colors.market.dark.clone()).unwrap_or_else(|| {
            let hash = name.bytes().fold(7_usize, |hash, byte| hash.wrapping_mul(31).wrapping_add(usize::from(byte)));
            PALETTE[hash % PALETTE.len()].to_owned()
        });
    (short, colour)
}

/// The icon for a tag chip.
pub fn tag_icon(tag: &str) -> Icon {
    match tag {
        "Soccer" | "EPL" | "LaLiga" | "Bundesliga" | "UCL" | "Serie A" | "UECL" | "Eredivisie" | "Ligue 1" | "MLS" => {
            Icon::Goal
        }
        "Esports" | "Dota 2" | "BLAST Slam" => Icon::Gamepad2,
        "Tennis" => Icon::Volleyball,
        "Sports" | "Football" | "NCAA Football" | "NFL" | "Hockey" | "NHL" | "Basketball" | "NBA" => Icon::Trophy,
        "Elections" | "Midterms" => Icon::Vote,
        "Politics" | "USA" | "France" | "Spain" => Icon::Landmark,
        "Geopolitics" => Icon::Earth,
        "Crypto" => Icon::Bitcoin,
        "AI" => Icon::BrainCircuit,
        "Tech" | "Meta" => Icon::Cpu,
        "Pre-IPO" | "IPO" | "Companies" => Icon::Building2,
        "Commodities" => Icon::Fuel,
        "Macro" => Icon::Banknote,
        "Finance" => Icon::ChartCandlestick,
        "Culture" => Icon::Clapperboard,
        "Prediction Markets" => Icon::ChartLine,
        _ => Icon::CircleDot,
    }
}
