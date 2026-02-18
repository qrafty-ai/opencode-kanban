use chrono::{DateTime, Utc};

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
