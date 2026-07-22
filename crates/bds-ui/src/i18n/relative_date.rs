use bds_core::i18n::{UiLocale, translate, translate_with};
use chrono::{DateTime, Datelike, Local, NaiveDate};

/// chrono format locale per sidebar_views.allium LocaleMapping
/// (ui_locale → format_locale, e.g. "de" → "de-DE").
fn format_locale(locale: UiLocale) -> chrono::Locale {
    match locale {
        UiLocale::En => chrono::Locale::en_US,
        UiLocale::De => chrono::Locale::de_DE,
        UiLocale::Fr => chrono::Locale::fr_FR,
        UiLocale::It => chrono::Locale::it_IT,
        UiLocale::Es => chrono::Locale::es_ES,
    }
}

/// Per sidebar_views.allium RelativeDateFormat, with
/// diff_days = (today - timestamp.date).days on local calendar dates:
///   diff_days = 0 → locale time string
///   diff_days = 1 → localized "Yesterday"
///   diff_days < 7 → short weekday name
///   otherwise     → short month name + numeric day
pub fn format_relative_date(locale: UiLocale, unix_ms: i64, today: NaiveDate) -> String {
    let timestamp = DateTime::from_timestamp_millis(unix_ms)
        .unwrap_or_default()
        .with_timezone(&Local);
    let chrono_locale = format_locale(locale);
    let diff_days = (today - timestamp.date_naive()).num_days();

    if diff_days == 0 {
        timestamp.format_localized("%X", chrono_locale).to_string()
    } else if diff_days == 1 {
        translate(locale, "sidebar.chatYesterday")
    } else if diff_days < 7 {
        timestamp.format_localized("%a", chrono_locale).to_string()
    } else {
        let month = timestamp.format_localized("%b", chrono_locale).to_string();
        let day = timestamp.day().to_string();
        translate_with(
            locale,
            "sidebar.relativeDateMonthDay",
            &[("month", &month), ("day", &day)],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn local_ms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
        Local
            .with_ymd_and_hms(y, mo, d, h, mi, s)
            .single()
            .expect("unambiguous local time")
            .timestamp_millis()
    }

    fn day(y: i32, mo: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, mo, d).expect("valid date")
    }

    // Reference "today" throughout: Tuesday, 2026-02-17.
    const TODAY: (i32, u32, u32) = (2026, 2, 17);

    fn today() -> NaiveDate {
        day(TODAY.0, TODAY.1, TODAY.2)
    }

    #[test]
    fn same_day_shows_locale_time() {
        let ts = local_ms(2026, 2, 17, 14, 5, 7);
        let en = format_relative_date(UiLocale::En, ts, today());
        assert!(en.contains("05:07"), "expected time string, got {en}");
        assert!(en.contains("PM"), "en-US uses 12-hour clock, got {en}");
        let de = format_relative_date(UiLocale::De, ts, today());
        assert!(de.contains("14:05"), "de-DE uses 24-hour clock, got {de}");
    }

    #[test]
    fn one_day_ago_is_yesterday_even_under_24_hours() {
        // 23:59 yesterday vs. any time today: calendar-day diff is 1.
        let ts = local_ms(2026, 2, 16, 23, 59, 0);
        assert_eq!(format_relative_date(UiLocale::En, ts, today()), "Yesterday");
        assert_eq!(format_relative_date(UiLocale::De, ts, today()), "Gestern");
    }

    #[test]
    fn two_days_ago_shows_short_weekday() {
        // 2026-02-15 is a Sunday.
        let ts = local_ms(2026, 2, 15, 12, 0, 0);
        assert_eq!(format_relative_date(UiLocale::En, ts, today()), "Sun");
        assert_eq!(format_relative_date(UiLocale::De, ts, today()), "So");
    }

    #[test]
    fn six_days_ago_still_shows_weekday() {
        // 2026-02-11 is a Wednesday: diff_days = 6 is the last weekday case.
        let ts = local_ms(2026, 2, 11, 12, 0, 0);
        assert_eq!(format_relative_date(UiLocale::En, ts, today()), "Wed");
    }

    #[test]
    fn seven_days_ago_shows_month_and_day() {
        // diff_days = 7 is the first month+day case.
        let ts = local_ms(2026, 2, 10, 12, 0, 0);
        assert_eq!(format_relative_date(UiLocale::En, ts, today()), "Feb 10");
        assert_eq!(format_relative_date(UiLocale::De, ts, today()), "10. Feb");
    }

    #[test]
    fn month_day_order_is_day_first_in_romance_locales() {
        let ts = local_ms(2026, 2, 10, 12, 0, 0);
        for locale in [UiLocale::Fr, UiLocale::It, UiLocale::Es] {
            let result = format_relative_date(locale, ts, today());
            assert!(
                result.starts_with("10 "),
                "expected day-first order for {locale}, got {result}"
            );
        }
    }

    #[test]
    fn distant_past_shows_month_and_day() {
        let ts = local_ms(2025, 6, 3, 12, 0, 0);
        assert_eq!(format_relative_date(UiLocale::En, ts, today()), "Jun 3");
    }

    #[test]
    fn future_timestamp_uses_weekday_branch() {
        // Spec formula: diff_days < 7 → weekday; a future date has a
        // negative diff and therefore falls into the weekday case.
        // 2026-02-20 is a Friday.
        let ts = local_ms(2026, 2, 20, 12, 0, 0);
        assert_eq!(format_relative_date(UiLocale::En, ts, today()), "Fri");
    }

    #[test]
    fn yesterday_is_translated_in_all_locales() {
        let ts = local_ms(2026, 2, 16, 12, 0, 0);
        for locale in UiLocale::all() {
            let result = format_relative_date(*locale, ts, today());
            assert_ne!(
                result, "sidebar.chatYesterday",
                "missing yesterday translation for {locale}"
            );
            assert!(!result.is_empty());
        }
    }

    #[test]
    fn month_day_pattern_exists_in_all_locales() {
        let ts = local_ms(2025, 6, 3, 12, 0, 0);
        for locale in UiLocale::all() {
            let result = format_relative_date(*locale, ts, today());
            assert_ne!(
                result, "sidebar.relativeDateMonthDay",
                "missing month-day pattern for {locale}"
            );
            assert!(result.contains('3'), "day missing for {locale}: {result}");
        }
    }
}
