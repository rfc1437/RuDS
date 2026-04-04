use chrono::{DateTime, Utc, TimeZone};

/// Convert Unix milliseconds to ISO 8601 string (e.g., `2005-11-13T12:00:00.000Z`).
pub fn unix_ms_to_iso(ms: i64) -> String {
    let dt = Utc.timestamp_millis_opt(ms).single().unwrap_or_default();
    dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Parse an ISO 8601 string back to Unix milliseconds.
pub fn iso_to_unix_ms(iso: &str) -> Result<i64, String> {
    let dt = iso
        .parse::<DateTime<Utc>>()
        .map_err(|e| format!("invalid ISO 8601 timestamp '{iso}': {e}"))?;
    Ok(dt.timestamp_millis())
}

/// Extract zero-padded (YYYY, MM) from Unix milliseconds.
pub fn year_month_from_unix_ms(ms: i64) -> (String, String) {
    let dt = Utc.timestamp_millis_opt(ms).single().unwrap_or_default();
    (dt.format("%Y").to_string(), dt.format("%m").to_string())
}

/// Extract zero-padded (YYYY, MM, DD) from Unix milliseconds.
pub fn year_month_day_from_unix_ms(ms: i64) -> (String, String, String) {
    let dt = Utc.timestamp_millis_opt(ms).single().unwrap_or_default();
    (
        dt.format("%Y").to_string(),
        dt.format("%m").to_string(),
        dt.format("%d").to_string(),
    )
}

/// Current time as Unix milliseconds.
pub fn now_unix_ms() -> i64 {
    Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_conversion() {
        let ms: i64 = 1131883200000; // 2005-11-13T12:00:00.000Z
        let iso = unix_ms_to_iso(ms);
        assert_eq!(iso, "2005-11-13T12:00:00.000Z");
        assert_eq!(iso_to_unix_ms(&iso).unwrap(), ms);
    }

    #[test]
    fn roundtrip_with_real_millis() {
        let ms: i64 = 1741376097243; // some real timestamp with millis
        let iso = unix_ms_to_iso(ms);
        assert!(iso.ends_with("Z"));
        assert_eq!(iso_to_unix_ms(&iso).unwrap(), ms);
    }

    #[test]
    fn year_month_extraction() {
        let ms: i64 = 1131883200000;
        let (y, m) = year_month_from_unix_ms(ms);
        assert_eq!(y, "2005");
        assert_eq!(m, "11");
    }

    #[test]
    fn year_month_day_extraction() {
        let ms: i64 = 1131883200000;
        let (y, m, d) = year_month_day_from_unix_ms(ms);
        assert_eq!(y, "2005");
        assert_eq!(m, "11");
        assert_eq!(d, "13");
    }

    #[test]
    fn now_is_recent() {
        let ms = now_unix_ms();
        // Should be after 2024-01-01
        assert!(ms > 1704067200000);
    }

    #[test]
    fn invalid_iso_returns_error() {
        assert!(iso_to_unix_ms("not a date").is_err());
    }
}
