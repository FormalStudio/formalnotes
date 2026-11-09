//! Short labels: relative dates and counts.

use jiff::Timestamp;

/// "Just now", "5 min ago", "3 h ago", "Yesterday", "Mon", then the date.
pub fn relative(then: Timestamp, now: Timestamp) -> String {
    let minutes = now.duration_since(then).as_secs().max(0) / 60;
    let tz = jiff::tz::TimeZone::system();
    let (day_then, day_now) = (then.to_zoned(tz.clone()).date(), now.to_zoned(tz.clone()).date());
    let days = day_now.since(day_then).map(|s| s.get_days()).unwrap_or(0);
    match (minutes, days) {
        (..1, _) => "Just now".to_owned(),
        (1..60, _) => format!("{minutes} min ago"),
        (_, 0) => format!("{} h ago", minutes / 60),
        (_, 1) => "Yesterday".to_owned(),
        (_, 2..7) => then.to_zoned(tz).strftime("%A").to_string(),
        _ if day_then.year() == day_now.year() => then.to_zoned(tz).strftime("%b %-d").to_string(),
        _ => then.to_zoned(tz).strftime("%b %-d, %Y").to_string(),
    }
}

pub fn items(n: usize) -> String {
    if n == 1 { "1 item".to_owned() } else { format!("{n} items") }
}

pub fn words(n: usize) -> String {
    if n == 1 { "1 word".to_owned() } else { format!("{n} words") }
}
