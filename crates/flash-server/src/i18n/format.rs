//! Numbers, dates and times as the reader's own region writes them, from
//! the Unicode CLDR through ICU4X: "12.345" in Spain and "12,345" in
//! Mexico, "3 sept" in Spain and "3 de sept" in Colombia, "15:05" in
//! Spain and "3:05 p. m." in Argentina. English pages keep the forms they
//! have always used (`crate::when`).

use icu_calendar::{Date, Gregorian};
use icu_datetime::{fieldsets, FixedCalendarDateTimeFormatter, NoCalendarFormatter};
use icu_decimal::DecimalFormatter;
use icu_locale_core::Locale;
use icu_time::{DateTime, Time};

/// A region's numbers alone ("12.345", "1,5"), for what writes numbers
/// and no dates (a message's variables).
pub struct Decimals(DecimalFormatter);

impl Decimals {
    /// The number formatter for `tag`, or None when ICU4X has no data
    /// for it.
    pub fn new(tag: &str) -> Option<Decimals> {
        let locale = Locale::try_from_str(tag).ok()?;
        DecimalFormatter::try_new((&locale).into(), Default::default())
            .ok()
            .map(Decimals)
    }

    /// A whole number: "12.345", "12,345".
    pub fn integer(&self, n: i64) -> String {
        self.0.format(&fixed_decimal::Decimal::from(n)).to_string()
    }

    /// A number with at most `digits` after the separator: "1,5", "1.5".
    pub fn decimal(&self, n: f64, digits: u8) -> String {
        let scaled = (n * 10f64.powi(i32::from(digits))).round() as i64;
        let mut d = fixed_decimal::Decimal::from(scaled);
        d.multiply_pow10(-i16::from(digits));
        d.absolute.trim_end();
        self.0.format(&d).to_string()
    }
}

/// The formatters for one region's tag ("es-MX"), made once and kept.
pub struct Formats {
    decimal: Decimals,
    day_month: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::MD>,
    day_month_numeric: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::MD>,
    month: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::M>,
    day_month_year: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::YMD>,
    weekday: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::E>,
    weekday_long: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::E>,
    time: NoCalendarFormatter<fieldsets::T>,
    hour: NoCalendarFormatter<fieldsets::T>,
    stamp: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::MDT>,
    stamp_year: FixedCalendarDateTimeFormatter<Gregorian, fieldsets::YMDT>,
}

impl Formats {
    /// The formatters for `tag`, or None when ICU4X has no data for it
    /// (it falls back through the tag's parents, so any Spanish tag has).
    pub fn new(tag: &str) -> Option<Formats> {
        let locale = Locale::try_from_str(tag).ok()?;
        let prefs = || (&locale).into();
        use fieldsets::{E, M, MD, MDT, T, YMD, YMDT};
        use icu_datetime::options::TimePrecision;
        Some(Formats {
            decimal: Decimals::new(tag)?,
            day_month: FixedCalendarDateTimeFormatter::try_new(prefs(), MD::medium()).ok()?,
            day_month_numeric: FixedCalendarDateTimeFormatter::try_new(prefs(), MD::short())
                .ok()?,
            month: FixedCalendarDateTimeFormatter::try_new(prefs(), M::medium()).ok()?,
            day_month_year: FixedCalendarDateTimeFormatter::try_new(prefs(), YMD::medium()).ok()?,
            weekday: FixedCalendarDateTimeFormatter::try_new(prefs(), E::medium()).ok()?,
            weekday_long: FixedCalendarDateTimeFormatter::try_new(prefs(), E::long()).ok()?,
            time: NoCalendarFormatter::try_new(
                prefs(),
                T::short().with_time_precision(TimePrecision::Minute),
            )
            .ok()?,
            hour: NoCalendarFormatter::try_new(
                prefs(),
                T::short().with_time_precision(TimePrecision::Hour),
            )
            .ok()?,
            stamp: FixedCalendarDateTimeFormatter::try_new(
                prefs(),
                MDT::medium().with_time_precision(TimePrecision::Second),
            )
            .ok()?,
            stamp_year: FixedCalendarDateTimeFormatter::try_new(
                prefs(),
                YMDT::medium().with_time_precision(TimePrecision::Second),
            )
            .ok()?,
        })
    }

    /// A whole number: "12.345", "12,345".
    pub fn integer(&self, n: i64) -> String {
        self.decimal.integer(n)
    }

    /// A number with at most `digits` after the separator: "1,5", "1.5".
    pub fn decimal(&self, n: f64, digits: u8) -> String {
        self.decimal.decimal(n, digits)
    }

    /// Day and month: "3 sept", "3 de sept".
    pub fn day_month(&self, day: jiff::civil::Date) -> String {
        date(day).map_or_else(String::new, |d| self.day_month.format(&d).to_string())
    }

    /// Day and month in figures: "3/9".
    pub fn day_month_numeric(&self, day: jiff::civil::Date) -> String {
        date(day).map_or_else(String::new, |d| {
            self.day_month_numeric.format(&d).to_string()
        })
    }

    /// The month alone, short: "sept".
    pub fn month(&self, day: jiff::civil::Date) -> String {
        date(day).map_or_else(String::new, |d| self.month.format(&d).to_string())
    }

    /// Day, month and year: "3 sept 2026".
    pub fn day_month_year(&self, day: jiff::civil::Date) -> String {
        date(day).map_or_else(String::new, |d| self.day_month_year.format(&d).to_string())
    }

    /// The weekday, short: "jue".
    pub fn weekday(&self, day: jiff::civil::Date) -> String {
        date(day).map_or_else(String::new, |d| self.weekday.format(&d).to_string())
    }

    /// The weekday, whole: "jueves".
    pub fn weekday_long(&self, day: jiff::civil::Date) -> String {
        date(day).map_or_else(String::new, |d| self.weekday_long.format(&d).to_string())
    }

    /// Hour and minute: "15:05", "3:05 p. m.".
    pub fn time(&self, at: jiff::civil::Time) -> String {
        time(at).map_or_else(String::new, |t| self.time.format(&t).to_string())
    }

    /// The hour alone: "15", "3 p. m.".
    pub fn hour(&self, at: jiff::civil::Time) -> String {
        time(at).map_or_else(String::new, |t| self.hour.format(&t).to_string())
    }

    /// Day, month and time to the second, with the year when `year`.
    pub fn stamp(&self, at: jiff::civil::DateTime, year: bool) -> String {
        let (Some(d), Some(t)) = (date(at.date()), time(at.time())) else {
            return String::new();
        };
        let dt = DateTime { date: d, time: t };
        if year {
            self.stamp_year.format(&dt).to_string()
        } else {
            self.stamp.format(&dt).to_string()
        }
    }
}

fn date(day: jiff::civil::Date) -> Option<Date<Gregorian>> {
    let month = u8::try_from(day.month()).ok()?;
    let d = u8::try_from(day.day()).ok()?;
    Date::try_new_gregorian(i32::from(day.year()), month, d).ok()
}

fn time(at: jiff::civil::Time) -> Option<Time> {
    let h = u8::try_from(at.hour()).ok()?;
    let m = u8::try_from(at.minute()).ok()?;
    let s = u8::try_from(at.second()).ok()?;
    Time::try_new(h, m, s, 0).ok()
}

#[cfg(test)]
mod tests {
    use super::Formats;

    /// What CLDR says each region writes, as ICU4X 2.3 carries it (CLDR
    /// 48.2.1). Node 24's Intl (CLDR 47) agrees on every number; on dates
    /// CLDR 48 pads Mexico's hour and gives Colombia a numeric medium
    /// date, and the newer data stands.
    #[test]
    fn regions_write_as_cldr_says() {
        let day = jiff::civil::date(2026, 9, 3);
        let at = jiff::civil::time(15, 5, 9, 0);
        let nbsp = '\u{a0}';
        let rows: &[(&str, &str, &str, &str, &str, &str)] = &[
            ("es-ES", "1234", "12.345", "1234,5", "3 sept", "15:05"),
            ("es-MX", "1,234", "12,345", "1,234.5", "3 sep", "03:05 p.m."),
            (
                "es-AR",
                "1.234",
                "12.345",
                "1.234,5",
                "3 sept",
                "3:05 p. m.",
            ),
            ("es-CO", "1.234", "12.345", "1.234,5", "3/9", "3:05 p. m."),
            (
                "es-CR",
                "1 234",
                "12 345",
                "1 234,5",
                "3 sept",
                "3:05 p. m.",
            ),
            (
                "es-419",
                "1,234",
                "12,345",
                "1,234.5",
                "3 sept",
                "3:05 p.m.",
            ),
        ];
        for (tag, a, b, c, md, hm) in rows {
            let f = Formats::new(tag).unwrap();
            let clean = |s: String| s.replace(['\u{202f}', nbsp], " ");
            assert_eq!(clean(f.integer(1234)), clean(a.to_string()), "{tag}");
            assert_eq!(clean(f.integer(12345)), clean(b.to_string()), "{tag}");
            assert_eq!(clean(f.decimal(1234.5, 1)), clean(c.to_string()), "{tag}");
            assert_eq!(clean(f.day_month(day)), clean(md.to_string()), "{tag}");
            assert_eq!(clean(f.time(at)), clean(hm.to_string()), "{tag}");
        }
        let es = Formats::new("es-ES").unwrap();
        assert_eq!(es.weekday(day), "jue");
        assert_eq!(es.day_month_numeric(day), "3/9");
        assert_eq!(es.month(day), "sept");
        assert_eq!(es.weekday_long(day), "jueves");
        assert_eq!(es.decimal(1.5, 1), "1,5");
        assert_eq!(es.decimal(2.0, 1), "2");
    }
}
