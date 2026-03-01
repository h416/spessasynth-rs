/// load_date.rs
/// Purpose: Parse a date string from various formats into a NaiveDateTime.
/// Ported from: src/utils/load_date.ts
use chrono::{Local, NaiveDate, NaiveDateTime};
use regex::Regex;

use crate::utils::loggin::spessa_synth_warn;

/// Portuguese → English weekday and month translations.
/// Needed for soundfont date strings like "sábado 26 setembro 2020, 16:40:14".
const PORTUGUESE_TRANSLATIONS: &[(&str, &str)] = &[
    // Weekdays
    ("domingo", "Sunday"),
    ("segunda-feira", "Monday"),
    ("terça-feira", "Tuesday"),
    ("quarta-feira", "Wednesday"),
    ("quinta-feira", "Thursday"),
    ("sexta-feira", "Friday"),
    ("sábado", "Saturday"),
    // Months
    ("janeiro", "January"),
    ("fevereiro", "February"),
    ("março", "March"),
    ("abril", "April"),
    ("maio", "May"),
    ("junho", "June"),
    ("julho", "July"),
    ("agosto", "August"),
    ("setembro", "September"),
    ("outubro", "October"),
    ("novembro", "November"),
    ("dezembro", "December"),
];

/// Date/datetime format strings tried (in order) when parsing an English date.
const DATE_FORMATS: &[&str] = &[
    // ISO 8601
    "%Y-%m-%dT%H:%M:%S",
    "%Y-%m-%dT%H:%M:%SZ",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d",
    // Day Month Year (European style)
    "%d %B %Y %H:%M:%S", // "26 September 2020 16:40:14"
    "%d %B %Y",          // "26 September 2020"
    "%d %b %Y %H:%M:%S", // "26 Sep 2020 16:40:14"
    "%d %b %Y",          // "26 Sep 2020"
    // Month Day, Year (US style)
    "%B %d, %Y %H:%M:%S", // "September 26, 2020 16:40:14"
    "%B %d, %Y",          // "September 26, 2020"
    "%B %d %Y %H:%M:%S",  // "September 26 2020 16:40:14"
    "%B %d %Y",           // "September 26 2020"
    "%b %d, %Y %H:%M:%S", // "Sep 26, 2020 16:40:14"
    "%b %d, %Y",          // "Sep 26, 2020"
    // With full weekday name (used after Portuguese → English translation)
    "%A %d %B %Y, %H:%M:%S", // "Saturday 26 September 2020, 16:40:14"
    "%A %d %B %Y",           // "Saturday 26 September 2020"
    "%A, %d %B %Y",          // "Saturday, 26 September 2020"
    // Numeric slash formats
    "%m/%d/%Y", // "09/26/2020"
    "%m/%d/%y", // "09/26/20"
    "%Y/%m/%d", // "2020/09/26"
];

/// Try to parse `s` using each format in `DATE_FORMATS`.
fn parse_with_formats(s: &str) -> Option<NaiveDateTime> {
    for fmt in DATE_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return d.and_hms_opt(0, 0, 0);
        }
    }
    None
}

/// Apply Portuguese → English word translations, then attempt to parse the result.
/// Equivalent to: `tryTranslate` in load_date.ts
fn try_translate(date_string: &str) -> Option<NaiveDateTime> {
    let mut translated = date_string.to_string();
    for (pt, en) in PORTUGUESE_TRANSLATIONS {
        let pattern = format!("(?i){}", regex::escape(pt));
        let re = Regex::new(&pattern).unwrap();
        translated = re.replace_all(&translated, *en).into_owned();
    }
    parse_with_formats(&translated)
}

/// Try to parse a `DD.MM.YYYY` formatted date string.
/// Equivalent to: `tryDotted` in load_date.ts
fn try_dotted(date_string: &str) -> Option<NaiveDateTime> {
    let re = Regex::new(r"^(\d{2})\.(\d{2})\.(\d{4})$").unwrap();
    let caps = re.captures(date_string)?;
    let day: u32 = caps[1].parse().ok()?;
    let month: u32 = caps[2].parse().ok()?;
    let year: i32 = caps[3].parse().ok()?;
    NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(0, 0, 0)
}

/// Try to parse a `DD MM YY` or `DD  MM YY` (AWE32/SFEDT) date string.
/// The month field is zero-indexed in the original format.
/// Equivalent to: `tryAWE` in load_date.ts
fn try_awe(date_string: &str) -> Option<NaiveDateTime> {
    let re = Regex::new(r"^(\d{1,2})\s{1,2}(\d{1,2})\s{1,2}(\d{2})$").unwrap();
    let caps = re.captures(date_string)?;
    let day: u32 = caps[1].parse().ok()?;
    let month: u32 = caps[2].parse::<u32>().ok()? + 1; // 0-indexed in original
    let year_str = caps[3].to_string();
    // Format as "MM/DD/YY" and let chrono decide the century
    // (chrono: 00-68 → 2000-2068, 69-99 → 1969-1999)
    let formatted = format!("{:02}/{:02}/{}", month, day, year_str);
    NaiveDate::parse_from_str(&formatted, "%m/%d/%y")
        .ok()?
        .and_hms_opt(0, 0, 0)
}

/// Try to extract a 4-digit year from `date_string` and return January 1st of that year.
/// Equivalent to: `tryYear` in load_date.ts
fn try_year(date_string: &str) -> Option<NaiveDateTime> {
    let re = Regex::new(r"\b(\d{4})\b").unwrap();
    let caps = re.captures(date_string)?;
    let year: i32 = caps[1].parse().ok()?;
    NaiveDate::from_ymd_opt(year, 1, 1)?.and_hms_opt(0, 0, 0)
}

/// Parse a date string into a `NaiveDateTime`, using multiple fallback strategies.
///
/// Strategies (in order):
/// 1. Strip ordinal suffixes ("26th" → "26") and " at " separator, then try common formats.
/// 2. Apply Portuguese → English word translations, then try common formats.
/// 3. Try `DD.MM.YYYY` dotted format.
/// 4. Try `DD MM YY` / `DD  MM YY` AWE/SFEDT format.
/// 5. Extract the first 4-digit year and return January 1st of that year.
/// 6. Emit a warning and return the current local datetime.
///
/// Equivalent to: `parseDateString` in load_date.ts
pub fn parse_date_string(date_string: &str) -> NaiveDateTime {
    let date_string = date_string.trim();
    if date_string.is_empty() {
        return Local::now().naive_local();
    }

    // Remove ordinal suffixes: "1st" → "1", "2nd" → "2", "26th" → "26", etc.
    // Replace " at " separator: "16 June 2020 at 14:30" → "16 June 2020 14:30"
    let ordinal_re = Regex::new(r"\b(\d+)(?:st|nd|rd|th)\b").unwrap();
    let at_re = Regex::new(r"(?i)\s+at\s+").unwrap();
    let step1 = ordinal_re.replace_all(date_string, "$1");
    let filtered = at_re.replace_all(&step1, " ");

    if let Some(dt) = parse_with_formats(&filtered) {
        return dt;
    }

    // Fallbacks use the original trimmed string (matching TypeScript behavior)
    if let Some(dt) = try_translate(date_string) {
        return dt;
    }
    if let Some(dt) = try_dotted(date_string) {
        return dt;
    }
    if let Some(dt) = try_awe(date_string) {
        return dt;
    }
    if let Some(dt) = try_year(date_string) {
        return dt;
    }

    spessa_synth_warn(&format!(
        "Invalid date: \"{date_string}\". Replacing with the current date!"
    ));
    Local::now().naive_local()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
    }

    fn ymdhms(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d)
            .unwrap()
            .and_hms_opt(h, mi, s)
            .unwrap()
    }

    // --- Empty string / whitespace only ---

    #[test]
    fn test_empty_string_does_not_panic() {
        let _ = parse_date_string("");
    }

    #[test]
    fn test_whitespace_only_does_not_panic() {
        let _ = parse_date_string("   ");
    }

    // --- ISO 8601 ---

    #[test]
    fn test_iso_date() {
        assert_eq!(parse_date_string("2020-09-26"), ymd(2020, 9, 26));
    }

    #[test]
    fn test_iso_datetime() {
        assert_eq!(
            parse_date_string("2020-09-26T16:40:14"),
            ymdhms(2020, 9, 26, 16, 40, 14)
        );
    }

    #[test]
    fn test_iso_datetime_with_space() {
        assert_eq!(
            parse_date_string("2020-09-26 16:40:14"),
            ymdhms(2020, 9, 26, 16, 40, 14)
        );
    }

    // --- Ordinal suffix removal (st / nd / rd / th) ---

    #[test]
    fn test_ordinal_st() {
        assert_eq!(parse_date_string("1st June 2020"), ymd(2020, 6, 1));
    }

    #[test]
    fn test_ordinal_nd() {
        assert_eq!(parse_date_string("2nd June 2020"), ymd(2020, 6, 2));
    }

    #[test]
    fn test_ordinal_rd() {
        assert_eq!(parse_date_string("3rd June 2020"), ymd(2020, 6, 3));
    }

    #[test]
    fn test_ordinal_th() {
        assert_eq!(parse_date_string("September 26th, 2020"), ymd(2020, 9, 26));
    }

    // --- " at " separator removal ---

    #[test]
    fn test_at_separator() {
        assert_eq!(
            parse_date_string("26 June 2020 at 16:40:14"),
            ymdhms(2020, 6, 26, 16, 40, 14)
        );
    }

    #[test]
    fn test_ordinal_and_at_combined() {
        assert_eq!(
            parse_date_string("September 26th, 2020 at 16:40:14"),
            ymdhms(2020, 9, 26, 16, 40, 14)
        );
    }

    // --- Portuguese translation ---

    #[test]
    fn test_portuguese_full_datetime() {
        // "sábado 26 setembro 2020, 16:40:14" → "Saturday 26 September 2020, 16:40:14"
        // 2020-09-26 is actually a Saturday
        assert_eq!(
            parse_date_string("sábado 26 setembro 2020, 16:40:14"),
            ymdhms(2020, 9, 26, 16, 40, 14)
        );
    }

    #[test]
    fn test_portuguese_date_only() {
        // "26 setembro 2020" → "26 September 2020"
        assert_eq!(parse_date_string("26 setembro 2020"), ymd(2020, 9, 26));
    }

    #[test]
    fn test_portuguese_month_name() {
        // Various month names: "janeiro" / "dezembro" etc.
        assert_eq!(parse_date_string("15 janeiro 2019"), ymd(2019, 1, 15));
        assert_eq!(parse_date_string("31 dezembro 2021"), ymd(2021, 12, 31));
    }

    // --- Dotted format DD.MM.YYYY ---

    #[test]
    fn test_dotted_format() {
        assert_eq!(parse_date_string("26.09.2020"), ymd(2020, 9, 26));
    }

    #[test]
    fn test_dotted_format_january() {
        assert_eq!(parse_date_string("01.01.2000"), ymd(2000, 1, 1));
    }

    // --- AWE32/SFEDT format "DD MM YY" ---

    #[test]
    fn test_awe_double_space() {
        // "26  9 20": day=26, month=9(0-indexed)+1=10(October), year=20 → 2020
        assert_eq!(parse_date_string("26  9 20"), ymd(2020, 10, 26));
    }

    #[test]
    fn test_awe_single_space() {
        // "26 9 20" same as above
        assert_eq!(parse_date_string("26 9 20"), ymd(2020, 10, 26));
    }

    #[test]
    fn test_awe_testcase_trim() {
        // Test case: " 4  0  97"
        // trim → "4  0  97": day=4, month=0+1=1(January), year=97 → 1997
        assert_eq!(parse_date_string(" 4  0  97"), ymd(1997, 1, 4));
    }

    #[test]
    fn test_awe_year_1990s() {
        // year=96 → 1996 (chrono: 69-99 → 1969-1999)
        assert_eq!(parse_date_string("15 0 96"), ymd(1996, 1, 15));
    }

    // --- 4-digit year only fallback ---

    #[test]
    fn test_year_only_bare() {
        // "2020" → January 1, 2020
        assert_eq!(parse_date_string("2020"), ymd(2020, 1, 1));
    }

    #[test]
    fn test_year_embedded_in_text() {
        assert_eq!(parse_date_string("some text 2020 more"), ymd(2020, 1, 1));
    }

    // --- Completely invalid input ---

    #[test]
    fn test_totally_invalid_does_not_panic() {
        let _ = parse_date_string("totally invalid garbage");
    }
}
