use chrono::{DateTime, Utc};
use nucleo::{Matcher, Utf32Str};
use std::panic::{AssertUnwindSafe, UnwindSafe, catch_unwind};

pub fn recency_frequency_bonus(
    use_count: i64,
    last_used_rfc3339: &str,
    now: DateTime<Utc>,
    frequency_weight: f64,
    recency_weight: f64,
    recency_half_life_hours: f64,
    scale: f64,
) -> f64 {
    let normalized_frequency = (1.0 + use_count.max(0) as f64).ln();
    let recency_bonus = DateTime::parse_from_rfc3339(last_used_rfc3339)
        .ok()
        .map(|last_used| {
            let hours_since_last_used =
                (now - last_used.with_timezone(&Utc)).num_seconds().max(0) as f64 / 3600.0;
            2f64.powf(-hours_since_last_used / recency_half_life_hours)
        })
        .unwrap_or(0.0);

    (normalized_frequency * frequency_weight + recency_bonus * recency_weight) * scale
}

pub fn ascii_case_insensitive_subsequence(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }

    if !haystack.is_ascii() || !needle.is_ascii() {
        return true;
    }

    let mut needle_iter = needle.bytes().map(|b| b.to_ascii_lowercase());
    let mut next = needle_iter.next();
    for hay in haystack.bytes().map(|b| b.to_ascii_lowercase()) {
        if Some(hay) == next {
            next = needle_iter.next();
            if next.is_none() {
                return true;
            }
        }
    }

    false
}

pub fn normalize_fuzzy_needle(input: &str) -> String {
    input
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn catch_panic_option<T, F>(f: F) -> Option<T>
where
    F: FnOnce() -> Option<T> + UnwindSafe,
{
    catch_unwind(f).ok().flatten()
}

pub fn safe_fuzzy_indices(
    matcher: &mut Matcher,
    haystack: Utf32Str<'_>,
    needle: Utf32Str<'_>,
    matched_indices: &mut Vec<u32>,
) -> Option<u16> {
    catch_panic_option(AssertUnwindSafe(|| {
        matcher.fuzzy_indices(haystack, needle, matched_indices)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_case_insensitive_subsequence_handles_ascii_matching() {
        assert!(ascii_case_insensitive_subsequence("Open Settings", "opset"));
        assert!(ascii_case_insensitive_subsequence("Delete Task", "dt"));
        assert!(!ascii_case_insensitive_subsequence("Delete Task", "dz"));
    }

    #[test]
    fn ascii_case_insensitive_subsequence_bypasses_non_ascii() {
        assert!(ascii_case_insensitive_subsequence("caf\u{00E9}", "cafe"));
        assert!(ascii_case_insensitive_subsequence("cafe", "caf\u{00E9}"));
    }

    #[test]
    fn normalize_fuzzy_needle_lowercases_and_strips_control_chars() {
        assert_eq!(normalize_fuzzy_needle("  Open\nSet\t  "), "openset");
    }

    #[test]
    fn catch_panic_option_returns_none_on_panic() {
        let value: Option<u8> = catch_panic_option(|| panic!("boom"));
        assert_eq!(value, None);
    }

    #[test]
    fn catch_panic_option_preserves_non_panic_result() {
        let value = catch_panic_option(|| Some(42u8));
        assert_eq!(value, Some(42));
    }
}
