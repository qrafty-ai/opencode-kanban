use std::process::Command;

fn normalized_version(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(without_prefix) = trimmed.strip_prefix('v')
        && without_prefix
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit())
    {
        return without_prefix.to_string();
    }
    trimmed.to_string()
}

fn version_from_git() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    let normalized = normalized_version(value.trim());
    if normalized.is_empty() {
        return None;
    }

    Some(normalized)
}

fn build_version() -> String {
    if let Ok(version) = std::env::var("OPENCODE_KANBAN_VERSION") {
        let normalized = normalized_version(&version);
        if !normalized.is_empty() {
            return normalized;
        }
    }

    version_from_git().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}

fn main() {
    println!("cargo:rerun-if-env-changed=OPENCODE_KANBAN_VERSION");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/packed-refs");

    let version = build_version();
    println!("cargo:rustc-env=OPENCODE_KANBAN_BUILD_VERSION={version}");
}
