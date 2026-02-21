use chrono::{DateTime, Local, Utc};

pub(crate) fn log_kind_label(raw: Option<&str>) -> String {
    let normalized = raw.unwrap_or("text").trim().to_ascii_lowercase();
    let value = match normalized.as_str() {
        "text" => "SAY".to_string(),
        "tool" => "TOOL".to_string(),
        "reasoning" => "THINK".to_string(),
        "step-start" => "STEP+".to_string(),
        "step-finish" => "STEP-".to_string(),
        "subtask" => "SUBTASK".to_string(),
        "patch" => "PATCH".to_string(),
        "agent" => "AGENT".to_string(),
        "snapshot" => "SNAP".to_string(),
        "retry" => "RETRY".to_string(),
        "compaction" => "COMPACT".to_string(),
        "file" => "FILE".to_string(),
        other => other.to_ascii_uppercase(),
    };

    if value.is_empty() {
        "TEXT".to_string()
    } else {
        value
    }
}

pub(crate) fn log_role_label(raw: Option<&str>) -> String {
    let value = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "unknown".to_string());
    value.to_ascii_uppercase()
}

pub(crate) fn log_time_label(raw: Option<&str>) -> String {
    let Some(value) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return "--:--:--".to_string();
    };

    if let Some(ts) = format_numeric_timestamp(value) {
        return ts;
    }

    if let Some((_, right)) = value.split_once('T') {
        let hhmmss = right.chars().take(8).collect::<String>();
        if hhmmss.len() == 8 {
            return hhmmss;
        }
    }

    if let Some((_, right)) = value.split_once(' ') {
        let hhmmss = right.chars().take(8).collect::<String>();
        if hhmmss.len() == 8 {
            return hhmmss;
        }
    }

    value.to_string()
}

pub(crate) fn format_numeric_timestamp(raw: &str) -> Option<String> {
    let value = raw.parse::<f64>().ok()?;
    if !value.is_finite() {
        return None;
    }

    let absolute = value.abs();
    let (seconds, nanos) = if absolute >= 1_000_000_000_000_000_000.0 {
        let sec = (value / 1_000_000_000.0).trunc() as i64;
        let nano = (value % 1_000_000_000.0).abs() as u32;
        (sec, nano)
    } else if absolute >= 1_000_000_000_000_000.0 {
        let sec = (value / 1_000_000.0).trunc() as i64;
        let nano = ((value % 1_000_000.0).abs() * 1_000.0) as u32;
        (sec, nano)
    } else if absolute >= 1_000_000_000_000.0 {
        let sec = (value / 1_000.0).trunc() as i64;
        let nano = ((value % 1_000.0).abs() * 1_000_000.0) as u32;
        (sec, nano)
    } else {
        let sec = value.trunc() as i64;
        let nano = ((value - value.trunc()).abs() * 1_000_000_000.0) as u32;
        (sec, nano)
    };

    let dt: DateTime<Utc> = DateTime::from_timestamp(seconds, nanos)?;
    Some(dt.with_timezone(&Local).format("%H:%M:%S").to_string())
}
