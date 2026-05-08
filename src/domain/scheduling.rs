use chrono::{DateTime, Duration, LocalResult, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;

pub trait DailyWindowTopic {
    fn daily_card_count(&self) -> Option<i64>;
    fn daily_time_zone(&self) -> Option<&str>;
    fn daily_update_time(&self) -> Option<&str>;
}

pub fn parse_daily_update_time(value: &str) -> NaiveTime {
    NaiveTime::parse_from_str(value.trim(), "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(value.trim(), "%H:%M:%S"))
        .unwrap_or(NaiveTime::MIN)
}

pub fn parse_utc_offset_seconds(value: &str) -> Option<i64> {
    let value = value.trim().to_ascii_uppercase();
    let rest = value
        .strip_prefix("UTC")
        .or_else(|| value.strip_prefix("GMT"))?;
    if rest.is_empty() {
        return Some(0);
    }
    let (sign, rest) = match rest.as_bytes().first()? {
        b'+' => (1_i64, &rest[1..]),
        b'-' => (-1_i64, &rest[1..]),
        _ => return None,
    };
    let (hours, minutes) = if let Some((hours, minutes)) = rest.split_once(':') {
        (hours.parse::<i64>().ok()?, minutes.parse::<i64>().ok()?)
    } else {
        (rest.parse::<i64>().ok()?, 0)
    };
    if !(0..=14).contains(&hours) || !(0..60).contains(&minutes) {
        return None;
    }
    Some(sign * (hours * 3600 + minutes * 60))
}

pub fn daily_window_start(time_zone: &str, update_time: &str) -> DateTime<Utc> {
    if let Some(offset_seconds) = parse_utc_offset_seconds(time_zone) {
        let update_time = parse_daily_update_time(update_time);
        let local_now = (Utc::now() + Duration::seconds(offset_seconds)).naive_utc();
        let mut start_date = local_now.date();
        if local_now.time() < update_time {
            start_date = start_date
                .checked_sub_signed(Duration::days(1))
                .unwrap_or(start_date);
        }
        let start_utc = start_date.and_time(update_time) - Duration::seconds(offset_seconds);
        return DateTime::<Utc>::from_naive_utc_and_offset(start_utc, Utc);
    }

    let tz = time_zone.parse::<Tz>().unwrap_or(chrono_tz::UTC);
    let update_time = parse_daily_update_time(update_time);
    let local_now = Utc::now().with_timezone(&tz);
    let mut start_date = local_now.date_naive();
    if local_now.time() < update_time {
        start_date = start_date
            .checked_sub_signed(Duration::days(1))
            .unwrap_or(start_date);
    }
    resolve_local_time(tz, start_date.and_time(update_time))
}

pub fn topic_daily_window_start<T: DailyWindowTopic>(
    topic: &T,
    default_tz: &str,
    default_time: &str,
) -> DateTime<Utc> {
    daily_window_start(
        topic
            .daily_time_zone()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(default_tz),
        topic
            .daily_update_time()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(default_time),
    )
}

pub fn topic_daily_card_count<T: DailyWindowTopic>(topic: &T) -> usize {
    topic
        .daily_card_count()
        .and_then(|value| usize::try_from(value).ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
        .min(20)
}

fn resolve_local_time(tz: Tz, local: chrono::NaiveDateTime) -> DateTime<Utc> {
    for offset_minutes in 0..180 {
        let candidate = local + Duration::minutes(offset_minutes);
        match tz.from_local_datetime(&candidate) {
            LocalResult::Single(dt) => return dt.with_timezone(&Utc),
            LocalResult::Ambiguous(earliest, _) => return earliest.with_timezone(&Utc),
            LocalResult::None => {}
        }
    }
    Utc::now()
}
