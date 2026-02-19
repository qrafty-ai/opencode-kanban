use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, anyhow};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::theme::ThemePreset;

const DEFAULT_THEME: &str = "default";
const DEFAULT_DEFAULT_VIEW: &str = "kanban";
const MIN_POLL_INTERVAL_MS: u64 = 500;
const MAX_POLL_INTERVAL_MS: u64 = 30_000;
const DEFAULT_POLL_INTERVAL_MS: u64 = 1_000;
const MIN_SIDE_PANEL_WIDTH: u16 = 20;
const MAX_SIDE_PANEL_WIDTH: u16 = 80;
const DEFAULT_SIDE_PANEL_WIDTH: u16 = 40;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub default_view: String,
    pub poll_interval_ms: u64,
    pub side_panel_width: u16,
    pub keybindings: KeybindingsConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub global: HashMap<String, Vec<String>>,
    pub project_list: HashMap<String, Vec<String>>,
    pub board: HashMap<String, Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: DEFAULT_THEME.to_string(),
            default_view: DEFAULT_DEFAULT_VIEW.to_string(),
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
            side_panel_width: DEFAULT_SIDE_PANEL_WIDTH,
            keybindings: KeybindingsConfig::default(),
        }
    }
}

impl Settings {
    pub fn config_path() -> Option<PathBuf> {
        let mut path = dirs::config_dir()?;
        path.push("opencode-kanban");
        path.push("settings.toml");
        Some(path)
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> Self {
        if !path.exists() {
            return Self::default();
        }

        match fs::read_to_string(path) {
            Ok(contents) => match toml::from_str::<Self>(&contents) {
                Ok(mut settings) => {
                    settings.validate();
                    settings
                }
                Err(error) => {
                    warn!(
                        "failed to parse settings config '{}': {}",
                        path.display(),
                        error
                    );
                    Self::default()
                }
            },
            Err(error) => {
                warn!(
                    "failed to read settings config '{}': {}",
                    path.display(),
                    error
                );
                Self::default()
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::config_path().ok_or_else(|| anyhow!("unable to determine config path"))?;
        self.save_to_path(&path)
    }

    fn save_to_path(&self, path: &Path) -> anyhow::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow!("invalid settings config path"))?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory '{}'", parent.display()))?;

        let mut validated = self.clone();
        validated.validate();
        let contents =
            toml::to_string_pretty(&validated).context("failed to serialize settings to TOML")?;

        let file_name = path
            .file_name()
            .ok_or_else(|| anyhow!("invalid settings config file name"))?
            .to_string_lossy()
            .to_string();
        let tmp_path = path.with_file_name(format!(".{file_name}.tmp"));

        fs::write(&tmp_path, contents).with_context(|| {
            format!(
                "failed to write temporary settings file '{}'",
                tmp_path.display()
            )
        })?;
        fs::rename(&tmp_path, path).with_context(|| {
            format!(
                "failed to atomically rename settings file '{}' to '{}'",
                tmp_path.display(),
                path.display()
            )
        })?;

        Ok(())
    }

    fn validate(&mut self) {
        self.poll_interval_ms = self
            .poll_interval_ms
            .clamp(MIN_POLL_INTERVAL_MS, MAX_POLL_INTERVAL_MS);
        self.side_panel_width = self
            .side_panel_width
            .clamp(MIN_SIDE_PANEL_WIDTH, MAX_SIDE_PANEL_WIDTH);

        self.theme = match ThemePreset::from_str(&self.theme) {
            Ok(preset) => preset.as_str().to_string(),
            Err(()) => {
                warn!(
                    "invalid theme '{}' in settings config; falling back to default",
                    self.theme
                );
                DEFAULT_THEME.to_string()
            }
        };

        self.default_view = match self.default_view.trim().to_ascii_lowercase().as_str() {
            "kanban" => "kanban".to_string(),
            "detail" => "detail".to_string(),
            _ => {
                warn!(
                    "invalid default_view '{}' in settings config; falling back to {}",
                    self.default_view, DEFAULT_DEFAULT_VIEW
                );
                DEFAULT_DEFAULT_VIEW.to_string()
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let mut path = std::env::temp_dir();
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            let id = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            path.push(format!("opencode-kanban-settings-test-{timestamp}-{id}"));
            fs::create_dir_all(&path).expect("failed to create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn settings_file_path(temp_dir: &TempDir) -> PathBuf {
        temp_dir
            .path()
            .join("opencode-kanban")
            .join("settings.toml")
    }

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.theme, "default");
        assert_eq!(settings.default_view, "kanban");
        assert_eq!(settings.poll_interval_ms, 1_000);
        assert_eq!(settings.side_panel_width, 40);
        assert_eq!(settings.keybindings, KeybindingsConfig::default());
    }

    #[test]
    fn test_load_missing_file() {
        let temp_dir = TempDir::new();
        let path = settings_file_path(&temp_dir);
        let settings = Settings::load_from_path(&path);
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn test_load_malformed_toml() {
        let temp_dir = TempDir::new();
        let path = settings_file_path(&temp_dir);
        fs::create_dir_all(path.parent().expect("settings path should have parent"))
            .expect("failed to create config dir");
        fs::write(&path, "theme = \"mono\"\npoll_interval_ms = [invalid")
            .expect("failed to write malformed settings");

        let settings = Settings::load_from_path(&path);
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn test_load_partial_toml() {
        let temp_dir = TempDir::new();
        let path = settings_file_path(&temp_dir);
        fs::create_dir_all(path.parent().expect("settings path should have parent"))
            .expect("failed to create config dir");
        fs::write(&path, "theme = \"mono\"").expect("failed to write partial settings");

        let settings = Settings::load_from_path(&path);
        assert_eq!(settings.theme, "mono");
        assert_eq!(settings.default_view, DEFAULT_DEFAULT_VIEW);
        assert_eq!(settings.poll_interval_ms, DEFAULT_POLL_INTERVAL_MS);
        assert_eq!(settings.side_panel_width, DEFAULT_SIDE_PANEL_WIDTH);
        assert_eq!(settings.keybindings, KeybindingsConfig::default());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let temp_dir = TempDir::new();
        let path = settings_file_path(&temp_dir);
        let mut expected = Settings {
            theme: "high-contrast".to_string(),
            default_view: "detail".to_string(),
            poll_interval_ms: 2_500,
            side_panel_width: 55,
            keybindings: KeybindingsConfig::default(),
        };
        expected.validate();

        expected
            .save_to_path(&path)
            .expect("failed to save settings for roundtrip test");
        let loaded = Settings::load_from_path(&path);

        assert_eq!(loaded, expected);
    }

    #[test]
    fn test_validate_clamps_values() {
        let mut settings = Settings {
            theme: "default".to_string(),
            default_view: "kanban".to_string(),
            poll_interval_ms: 1,
            side_panel_width: 999,
            keybindings: KeybindingsConfig::default(),
        };

        settings.validate();

        assert_eq!(settings.poll_interval_ms, MIN_POLL_INTERVAL_MS);
        assert_eq!(settings.side_panel_width, MAX_SIDE_PANEL_WIDTH);

        settings.poll_interval_ms = u64::MAX;
        settings.side_panel_width = 0;
        settings.validate();

        assert_eq!(settings.poll_interval_ms, MAX_POLL_INTERVAL_MS);
        assert_eq!(settings.side_panel_width, MIN_SIDE_PANEL_WIDTH);
    }

    #[test]
    fn test_validate_invalid_theme() {
        let mut settings = Settings {
            theme: "retro-wave".to_string(),
            default_view: "kanban".to_string(),
            ..Settings::default()
        };

        settings.validate();

        assert_eq!(settings.theme, "default");
    }

    #[test]
    fn test_validate_light_theme_alias() {
        let mut settings = Settings {
            theme: "day".to_string(),
            ..Settings::default()
        };

        settings.validate();

        assert_eq!(settings.theme, "light");
    }

    #[test]
    fn test_validate_invalid_default_view() {
        let mut settings = Settings {
            default_view: "list".to_string(),
            ..Settings::default()
        };

        settings.validate();

        assert_eq!(settings.default_view, "kanban");
    }

    #[test]
    fn test_atomic_write_creates_dirs() {
        let temp_dir = TempDir::new();
        let path = settings_file_path(&temp_dir);

        let settings = Settings {
            theme: "mono".to_string(),
            default_view: "detail".to_string(),
            ..Settings::default()
        };

        settings
            .save_to_path(&path)
            .expect("failed to save settings to nested path");

        assert!(path.exists());
        assert!(
            path.parent()
                .expect("settings path should have parent")
                .exists()
        );
    }
}
