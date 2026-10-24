//! Times as a person reads them, in the time zone of the device they are
//! looking from: the browser writes its zone into a cookie (`ZONE_COOKIE`,
//! `static/ui.js`), and a page that shows times says which zone it used
//! (`data-zone` on `<html>`), so the script can fix a page rendered in
//! the wrong one by setting the cookie and loading it again.
//!
//! In English the clock is twelve-hour, lowercase: `3 pm`, `3:40 pm`.
//! Any other language writes times and dates as its reader's region does
//! (`i18n::format`, CLDR): `15:40` in Spain, `3:40 p. m.` in Argentina.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::cookie::CookieJar;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

/// The browser's IANA zone (`America/New_York`), as `ui.js` writes it.
pub const ZONE_COOKIE: &str = "flash-tz";

/// A named time zone.
#[derive(Clone, Debug)]
pub struct Zone {
    /// The IANA name, as the page reports it back (`data-zone`).
    pub name: String,
    pub tz: TimeZone,
}

impl Zone {
    /// The zone an IANA name stands for; None for anything else.
    pub fn named(name: &str) -> Option<Zone> {
        let name = name.trim();
        if name.is_empty() || name.len() > 64 {
            return None;
        }
        flash_core::day::time_zone(name).map(|tz| Zone {
            name: name.to_string(),
            tz,
        })
    }

    pub fn utc() -> Zone {
        Zone {
            name: "UTC".to_string(),
            tz: TimeZone::UTC,
        }
    }

    /// `America/New York` for `America/New_York`.
    pub fn label(&self) -> String {
        self.name.replace('_', " ")
    }

    /// `ms` on this zone's clock; None for an instant out of range.
    pub fn at(&self, ms: i64) -> Option<Zoned> {
        Timestamp::from_millisecond(ms)
            .ok()
            .map(|t| t.to_zoned(self.tz.clone()))
    }
}

/// The zone of the device the request came from, when its page has told
/// us (`ZONE_COOKIE`); None before it has, or for a cookie that names no
/// zone. A handler falls back to the account's saved zone, then UTC
/// (`or_saved`).
#[derive(Clone, Debug, Default)]
pub struct ViewerZone(pub Option<Zone>);

impl ViewerZone {
    /// The device's zone, else the account's saved one, else UTC.
    pub fn or_saved(self, saved: &str) -> Zone {
        self.0
            .or_else(|| Zone::named(saved))
            .unwrap_or_else(Zone::utc)
    }
}

impl<S: Send + Sync> FromRequestParts<S> for ViewerZone {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        Ok(ViewerZone(
            jar.get(ZONE_COOKIE).and_then(|c| Zone::named(c.value())),
        ))
    }
}

/// `3 pm` on the hour, `3:40 pm` off it.
pub fn clock(ms: i64, zone: &Zone) -> String {
    let Some(z) = zone.at(ms) else {
        return String::new();
    };
    if let Some(f) = crate::i18n::tr().formats() {
        return if z.minute() == 0 {
            f.hour(z.time())
        } else {
            f.time(z.time())
        };
    }
    if z.minute() == 0 {
        z.strftime("%-I %P").to_string()
    } else {
        z.strftime("%-I:%M %P").to_string()
    }
}

/// `Oct 6`, with the year when it is not the year of `now_ms` (`Oct 6,
/// 2025`).
pub fn day(ms: i64, zone: &Zone, now_ms: i64) -> String {
    let (Some(z), Some(now)) = (zone.at(ms), zone.at(now_ms)) else {
        return String::new();
    };
    if let Some(f) = crate::i18n::tr().formats() {
        return if z.year() == now.year() {
            f.day_month(z.date())
        } else {
            f.day_month_year(z.date())
        };
    }
    if z.year() == now.year() {
        z.strftime("%b %-d").to_string()
    } else {
        z.strftime("%b %-d, %Y").to_string()
    }
}

/// `Oct 6, 2026`.
pub fn date(ms: i64, zone: &Zone) -> String {
    let Some(z) = zone.at(ms) else {
        return String::new();
    };
    match crate::i18n::tr().formats() {
        Some(f) => f.day_month_year(z.date()),
        None => z.strftime("%b %-d, %Y").to_string(),
    }
}

/// `Oct 6, 3:40:12 pm`, with the year when it is not the year of
/// `now_ms`: one exact moment, to the second.
pub fn stamp(ms: i64, zone: &Zone, now_ms: i64) -> String {
    let (Some(z), Some(now)) = (zone.at(ms), zone.at(now_ms)) else {
        return String::new();
    };
    if let Some(f) = crate::i18n::tr().formats() {
        return f.stamp(z.datetime(), z.year() != now.year());
    }
    if z.year() == now.year() {
        z.strftime("%b %-d, %-I:%M:%S %P").to_string()
    } else {
        z.strftime("%b %-d, %Y, %-I:%M:%S %P").to_string()
    }
}

/// How long ago `ts_ms` was ("3h ago"), as `web::format_ago` says it,
/// with a date in `zone` past a month instead of a UTC one.
pub fn ago(now_ms: i64, ts_ms: i64, zone: &Zone) -> crate::i18n::Words {
    const MONTH_MS: i64 = 30 * 24 * 60 * 60 * 1000;
    if now_ms - ts_ms >= MONTH_MS {
        return crate::i18n::Words::formatted(date(ts_ms, zone));
    }
    crate::web::format_ago(now_ms, ts_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-06 15:40:12 UTC.
    const T: i64 = 1_791_301_212_000;

    fn zone(name: &str) -> Zone {
        Zone::named(name).unwrap()
    }

    #[test]
    fn the_clock_is_twelve_hours_and_lowercase() {
        let utc = Zone::utc();
        assert_eq!(clock(T, &utc), "3:40 pm");
        let hour = T - 40 * 60_000 - 12_000;
        assert_eq!(clock(hour, &utc), "3 pm");
        // Midnight and noon.
        assert_eq!(clock(1_791_244_800_000, &utc), "12 am");
        assert_eq!(
            clock(1_791_244_800_000 + 12 * 3_600_000 + 30 * 60_000, &utc),
            "12:30 pm"
        );
        // A half-hour zone: 15:40 UTC is 21:10 in Kolkata.
        assert_eq!(clock(T, &zone("Asia/Kolkata")), "9:10 pm");
    }

    #[test]
    fn days_and_dates_are_the_zone_s() {
        let ny = zone("America/New_York");
        // 02:00 UTC on Oct 6 is still Oct 5 in New York.
        let early = 1_791_252_000_000;
        assert_eq!(day(early, &ny, T), "Oct 5");
        assert_eq!(day(early, &Zone::utc(), T), "Oct 6");
        assert_eq!(date(early, &ny), "Oct 5, 2026");
        // Another year names it.
        let last_year = T - 365 * 24 * 3_600_000;
        assert_eq!(day(last_year, &ny, T), "Oct 6, 2025");
        assert_eq!(stamp(T, &ny, T), "Oct 6, 11:40:12 am");
        assert_eq!(stamp(last_year, &Zone::utc(), T), "Oct 6, 2025, 3:40:12 pm");
    }

    #[test]
    fn only_a_real_zone_is_taken() {
        assert!(Zone::named("America/New_York").is_some());
        assert!(Zone::named("Not/AZone").is_none());
        assert!(Zone::named("").is_none());
        assert_eq!(
            ViewerZone(None).or_saved("Europe/Paris").name,
            "Europe/Paris"
        );
        assert_eq!(ViewerZone(None).or_saved("nonsense").name, "UTC");
        assert_eq!(
            ViewerZone(Zone::named("Asia/Tokyo"))
                .or_saved("Europe/Paris")
                .name,
            "Asia/Tokyo"
        );
        assert_eq!(zone("America/New_York").label(), "America/New York");
    }
}
