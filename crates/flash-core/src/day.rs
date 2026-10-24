//! The study day: when it starts for an account and when the next one
//! does, on the account's own local calendar. A day starts at the cutoff
//! hour (4am by default) in the account's time zone, so a change of
//! clocks keeps the cutoff at its local hour and makes that day 23 or 25
//! hours long.

use jiff::civil::Date;
use jiff::tz::{TimeZone, TimeZoneDatabase};
use jiff::{Timestamp, Zoned};

use crate::settings::UserSettings;

/// The zone an account studies in: its own when the tz database knows
/// it, UTC otherwise.
pub fn zone_or_utc(timezone: &str) -> TimeZone {
    time_zone(timezone).unwrap_or(TimeZone::UTC)
}

/// The zone an IANA name names, or None when no database knows it. The
/// host's database comes first, since it has the newest rules; the copy
/// built into the binary answers what the host lacks. A host can ship
/// without the old names browsers still send (`Asia/Calcutta`,
/// `US/Eastern`: Ubuntu moved them to `tzdata-legacy`), and jiff
/// otherwise reads one database and never falls back per name. Every
/// lookup of a stored or submitted zone goes through here.
pub fn time_zone(name: &str) -> Option<TimeZone> {
    find_in(jiff::tz::db(), name)
}

fn find_in(host: &TimeZoneDatabase, name: &str) -> Option<TimeZone> {
    host.get(name)
        .or_else(|_| TimeZoneDatabase::bundled().get(name))
        .ok()
}

/// The start of the study day holding `now_ms`: today's cutoff when it
/// has passed, yesterday's otherwise.
pub fn study_day_start_ms(timezone: &str, cutoff_hour: u8, now_ms: i64) -> i64 {
    let tz = zone_or_utc(timezone);
    let now = Timestamp::from_millisecond(now_ms).unwrap_or(Timestamp::UNIX_EPOCH);
    let local: Zoned = now.to_zoned(tz);
    let cutoff = local
        .with()
        .hour(cutoff_hour as i8)
        .minute(0)
        .second(0)
        .subsec_nanosecond(0)
        .build()
        .unwrap_or_else(|_| local.clone());
    let day_start = if cutoff.timestamp() > now {
        cutoff.yesterday().unwrap_or(cutoff)
    } else {
        cutoff
    };
    day_start.timestamp().as_millisecond()
}

/// The start of the study day after the one holding `now_ms`.
pub fn next_study_day_ms(timezone: &str, cutoff_hour: u8, now_ms: i64) -> i64 {
    let start = study_day_start_ms(timezone, cutoff_hour, now_ms);
    Timestamp::from_millisecond(start)
        .ok()
        .map(|t| t.to_zoned(zone_or_utc(timezone)))
        .and_then(|z| z.tomorrow().ok())
        .map(|z| z.timestamp().as_millisecond())
        .unwrap_or(start + 24 * 60 * 60 * 1000)
}

/// The local calendar date of the study day holding `now_ms`: before
/// the cutoff, still yesterday's.
pub fn study_date(timezone: &str, cutoff_hour: u8, now_ms: i64) -> Date {
    let start = study_day_start_ms(timezone, cutoff_hour, now_ms);
    Timestamp::from_millisecond(start)
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .to_zoned(zone_or_utc(timezone))
        .date()
}

/// Study days from the one holding `from_ms` to the one holding `to_ms`,
/// 0 when `to_ms` is on an earlier one.
pub fn study_days_between(timezone: &str, cutoff_hour: u8, from_ms: i64, to_ms: i64) -> u32 {
    let date = |ms| study_date(timezone, cutoff_hour, ms);
    date(from_ms)
        .until(date(to_ms))
        .map_or(0, |span| span.get_days().max(0) as u32)
}

/// The start of the study day of local date `date`: its cutoff hour, or
/// the first moment after it when a change of clocks skips that hour.
pub fn study_day_start_of(timezone: &str, cutoff_hour: u8, date: Date) -> i64 {
    let at = date
        .at(cutoff_hour as i8, 0, 0, 0)
        .to_zoned(zone_or_utc(timezone))
        .map(|z| z.timestamp().as_millisecond())
        .unwrap_or(0);
    study_day_start_ms(timezone, cutoff_hour, at)
}

impl UserSettings {
    /// Start of this account's current study day (e.g. 4am local).
    pub fn study_day_start_ms(&self, now_ms: i64) -> i64 {
        study_day_start_ms(&self.timezone, self.day_cutoff_hour, now_ms)
    }

    /// Start of this account's next study day.
    pub fn next_study_day_ms(&self, now_ms: i64) -> i64 {
        next_study_day_ms(&self.timezone, self.day_cutoff_hour, now_ms)
    }

    /// The local date of this account's current study day.
    pub fn study_date(&self, now_ms: i64) -> Date {
        study_date(&self.timezone, self.day_cutoff_hour, now_ms)
    }

    /// Start of the study day of `date` on this account's calendar.
    pub fn study_day_start_of(&self, date: Date) -> i64 {
        study_day_start_of(&self.timezone, self.day_cutoff_hour, date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host database that knows nothing stands in for one without the
    /// old links: the built-in copy still answers.
    #[test]
    fn a_name_the_host_lacks_comes_from_the_built_in_copy() {
        let host = TimeZoneDatabase::none();
        for name in ["Asia/Calcutta", "Asia/Saigon", "US/Eastern", "Asia/Kolkata"] {
            assert!(find_in(&host, name).is_some(), "{name}");
        }
        assert!(find_in(&host, "Not/A_Zone").is_none());
    }

    #[test]
    fn an_old_name_keeps_its_offset() {
        let at = Timestamp::from_second(1_791_437_440).unwrap();
        let calcutta = at.to_zoned(time_zone("Asia/Calcutta").unwrap());
        let kolkata = at.to_zoned(time_zone("Asia/Kolkata").unwrap());
        assert_eq!(calcutta.offset(), kolkata.offset());
        assert_eq!(calcutta.offset().seconds(), 5 * 3600 + 1800);
    }
}
