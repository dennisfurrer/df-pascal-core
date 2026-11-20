//! Number and time formatting with exact decimals, no floats.

use df_pascal_core::Dec;
use df_pascal_core::utils::Round;

/// A dollar price with at least two decimal places: `0.55`, `0.0755`.
pub fn price(value: Dec) -> String {
    let text = value.to_string();
    match text.split_once('.') {
        None => format!("{text}.00"),
        Some((_, fraction)) if fraction.len() < 2 => format!("{text}0"),
        Some(_) => text,
    }
}

pub fn opt_price(value: Option<Dec>) -> String {
    value.map_or_else(|| "-".to_owned(), price)
}

/// A price as a percentage to one decimal place: `0.215` is `21.5%`.
pub fn chance(value: Dec) -> String {
    let percent = value.checked_mul(Dec::from_int(100)).unwrap_or(Dec::ZERO).round(1, Round::Nearest);
    format!("{percent}%")
}

/// Dollars to two places with thousands separators: `$1,234.50`, `-$3.10`.
pub fn usd(value: Dec) -> String {
    let fixed = value.round(2, Round::Nearest).to_fixed(2);
    let (sign, digits) = match fixed.strip_prefix('-') {
        Some(rest) => ("-", rest.to_owned()),
        None => ("", fixed),
    };
    let (whole, fraction) = digits.split_once('.').unwrap_or((&digits, "00"));
    format!("{sign}${}.{fraction}", group(whole))
}

/// Dollars with an explicit sign: `+$1.20`, `-$3.10`.
pub fn signed_usd(value: Dec) -> String {
    match value.is_negative() {
        true => usd(value),
        false => format!("+{}", usd(value)),
    }
}

pub fn count(value: impl std::fmt::Display) -> String {
    let text = value.to_string();
    match text.strip_prefix('-') {
        Some(rest) => format!("-{}", group(rest)),
        None => group(&text),
    }
}

fn group(digits: &str) -> String {
    let mut out = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(character);
    }
    out
}

/// Unix milliseconds as a UTC date and time: `2026-11-20 14:03`.
pub fn time(ms: u64) -> String {
    let seconds = ms / 1_000;
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let rest = seconds % 86_400;
    let (year, month, day) = civil(days);
    format!("{year}-{month:02}-{day:02} {:02}:{:02}", rest / 3_600, (rest % 3_600) / 60)
}

/// The UTC date only: `2026-11-20`.
pub fn date(ms: u64) -> String {
    time(ms).chars().take(10).collect()
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Time since `ms` in the largest whole unit: `12s`, `5m`, `3h`, `2d`.
pub fn ago(ms: u64, now_ms: u64) -> String {
    let seconds = now_ms.saturating_sub(ms) / 1_000;
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3_600 => format!("{}m", seconds / 60),
        3_600..86_400 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// This browser's offset from UTC in milliseconds, east positive.
fn local_offset_ms() -> i64 {
    let minutes = js_sys::Date::new_0().get_timezone_offset() as i64;
    -minutes * 60_000
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn local_parts(ms: u64) -> (i64, i64, u32, u32, u64, u64) {
    let local = i64::try_from(ms).unwrap_or(0) + local_offset_ms();
    let seconds = local.max(0) / 1_000;
    let days = seconds / 86_400;
    let rest = u64::try_from(seconds % 86_400).unwrap_or(0);
    let (year, month, day) = civil(days);
    (days, year, month, day, rest / 3_600, (rest % 3_600) / 60)
}

/// Local date and time, short: `Today 20:30`, `Tomorrow 18:00`, `Nov 30, 01:00`, `Jan 1 2028`.
pub fn moment(ms: u64, now_ms: u64) -> String {
    let (days, year, month, day, hour, minute) = local_parts(ms);
    let (today, this_year, ..) = local_parts(now_ms);
    let name = MONTHS[usize::try_from(month.saturating_sub(1)).unwrap_or(0) % 12];
    match days - today {
        0 => format!("Today {hour:02}:{minute:02}"),
        1 => format!("Tomorrow {hour:02}:{minute:02}"),
        -1 => format!("Yesterday {hour:02}:{minute:02}"),
        _ if year == this_year => format!("{name} {day}, {hour:02}:{minute:02}"),
        _ => format!("{name} {day} {year}"),
    }
}

/// Dollars without cents above a thousand, with a suffix above a million: `$842.10`, `$7,381`,
/// `$1.2M`.
pub fn usd_short(value: Dec) -> String {
    let whole = value.units_at(0, Round::Down).unwrap_or(0);
    match whole {
        1_000_000.. => format!("${}.{}M", whole / 1_000_000, (whole % 1_000_000) / 100_000),
        1_000.. => format!("${}", group(&whole.to_string())),
        _ => usd(value),
    }
}
