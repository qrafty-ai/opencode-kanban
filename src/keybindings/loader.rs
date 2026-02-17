//! Keybinding configuration loader
//!
//! Loads keybinding configuration from TOML files with fallback to defaults.

use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use crate::keybindings::schema::KeybindingConfig;

/// Default keybindings config path (relative to project root)
const DEFAULT_CONFIG_PATH: &str = "config/keybindings.toml";

/// Error type for keybinding loading operations
#[derive(Debug)]
pub enum LoadError {
    ReadError {
        path: PathBuf,
        source: std::io::Error,
    },
    ParseError {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::ReadError { path, source } => {
                write!(
                    f,
                    "Failed to read keybindings from '{}': {}",
                    path.display(),
                    source
                )
            }
            LoadError::ParseError { path, source } => {
                write!(
                    f,
                    "Failed to parse keybindings from '{}': {}",
                    path.display(),
                    source
                )
            }
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LoadError::ReadError { source, .. } => Some(source),
            LoadError::ParseError { source, .. } => Some(source),
        }
    }
}

impl LoadError {
    fn read_err(path: &Path, source: std::io::Error) -> Self {
        LoadError::ReadError {
            path: path.to_path_buf(),
            source,
        }
    }

    fn parse_err(path: &Path, source: toml::de::Error) -> Self {
        LoadError::ParseError {
            path: path.to_path_buf(),
            source,
        }
    }
}

/// Load keybindings from the specified path.
///
/// If the file does not exist, falls back to the default configuration
/// from `config/keybindings.toml` in the project root.
///
/// # Arguments
/// * `path` - Path to the keybindings TOML file
///
/// # Returns
/// * `Ok(KeybindingConfig)` - Successfully loaded configuration
/// * `Err(LoadError::ParseError)` - File exists but contains invalid TOML
/// * `Err(LoadError::ReadError)` - Unexpected I/O error (not "file not found")
pub fn load_keybindings(path: &Path) -> Result<KeybindingConfig, LoadError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => toml::from_str(&contents).map_err(|e| LoadError::parse_err(path, e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => load_default_config(),
        Err(e) => Err(LoadError::read_err(path, e)),
    }
}

/// Load the default keybindings configuration.
///
/// Reads from `config/keybindings.toml` in the project root.
fn load_default_config() -> Result<KeybindingConfig, LoadError> {
    let default_path = Path::new(DEFAULT_CONFIG_PATH);
    std::fs::read_to_string(default_path)
        .map_err(|e| LoadError::read_err(default_path, e))
        .and_then(|contents| {
            toml::from_str(&contents).map_err(|e| LoadError::parse_err(default_path, e))
        })
}

/// Get the default config path for documentation/reference purposes
pub fn default_config_path() -> &'static Path {
    Path::new(DEFAULT_CONFIG_PATH)
}

#[cfg(test)]
mod tests {
    use super::{LoadError, load_default_config, load_keybindings};
    use std::fs;
    use tempfile::TempDir;

    fn valid_config() -> String {
        r#"
[navigation]
move_left = "h"
move_right = "l"
select_up = "k"
select_down = "j"
category_move_left = "H"
category_move_right = "L"
task_move_up = "K"
task_move_down = "J"

[tasks]
new_task = "n"
attach = "enter"
delete_task = "d"

[categories]
add = "c"
rename = "r"
delete = "x"
cycle_color = "t"

[dialogs]
command_palette = "ctrl-p"
help = "?"
dismiss = "esc"
confirm = "enter"
cancel = "esc"

[global]
quit = "q"
return_tmux = "ctrl-k"
project_list = "ctrl-o"
toggle_view = "\\"
submit = "enter"
toggle_checkbox = "space"
"#
        .to_string()
    }

    #[test]
    fn test_load_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("keybindings.toml");
        fs::write(&config_path, valid_config()).unwrap();

        let result = load_keybindings(&config_path);
        assert!(
            result.is_ok(),
            "Failed to load valid config: {:?}",
            result.err()
        );

        let config = result.unwrap();
        assert_eq!(config.navigation.move_left, "h");
        assert_eq!(config.tasks.new_task, "n");
    }

    #[test]
    fn test_missing_file_returns_default() {
        let temp_dir = TempDir::new().unwrap();
        let missing_path = temp_dir.path().join("nonexistent.toml");

        let result = load_keybindings(&missing_path);
        assert!(
            result.is_ok(),
            "Missing file should return default, got: {:?}",
            result.err()
        );

        let config = result.unwrap();
        assert_eq!(config.navigation.move_left, "h");
        assert_eq!(config.navigation.move_right, "l");
    }

    #[test]
    fn test_malformed_toml_returns_error() {
        let temp_dir = TempDir::new().unwrap();
        let bad_path = temp_dir.path().join("bad.toml");
        fs::write(&bad_path, "this is [not valid toml @@@").unwrap();

        let result = load_keybindings(&bad_path);
        assert!(result.is_err(), "Malformed TOML should return error");

        match result.unwrap_err() {
            LoadError::ParseError { path, .. } => {
                assert_eq!(path, bad_path, "Error should reference the bad file path");
            }
            other => panic!("Expected ParseError, got: {:?}", other),
        }
    }

    #[test]
    fn test_error_includes_path_context() {
        let temp_dir = TempDir::new().unwrap();
        let bad_path = temp_dir.path().join("bad.toml");
        fs::write(&bad_path, "invalid toml").unwrap();

        let result = load_keybindings(&bad_path);
        let err = result.unwrap_err();
        let err_string = err.to_string();

        assert!(
            err_string.contains("bad.toml"),
            "Error message should mention the path, got: {}",
            err_string
        );
    }

    #[test]
    fn test_load_default() {
        let result = load_default_config();
        assert!(
            result.is_ok(),
            "Default config should load: {:?}",
            result.err()
        );

        let config = result.unwrap();
        assert_eq!(config.navigation.move_left, "h");
        assert_eq!(config.navigation.move_right, "l");
    }

    #[test]
    fn test_valid_config_with_alternate_keys() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("keybindings.toml");

        let config_with_alts = r#"
[navigation]
move_left = "a"
move_left_alt = "left"
move_right = "d"
move_right_alt = "right"
select_up = "w"
select_up_alt = "up"
select_down = "s"
select_down_alt = "down"
category_move_left = "A"
category_move_right = "D"
task_move_up = "W"
task_move_down = "S"

[tasks]
new_task = "N"
attach = "tab"
attach_alt = "ctrl-t"
delete_task = "X"

[categories]
add = "C"
rename = "R"
delete = "D"
cycle_color = "T"

[dialogs]
command_palette = "ctrl-f"
help = "f1"
dismiss = "q"
dismiss_alt = "ctrl-g"
confirm = "y"
cancel = "n"

[global]
quit = "Q"
return_tmux = "ctrl-\\"
project_list = "ctrl-l"
toggle_view = "|"
submit = "ctrl-m"
toggle_checkbox = " "
"#;
        fs::write(&config_path, config_with_alts).unwrap();

        let result = load_keybindings(&config_path).unwrap();
        assert_eq!(result.navigation.move_left, "a");
        assert_eq!(result.navigation.move_left_alt, Some("left".to_string()));
        assert_eq!(result.tasks.attach_alt, Some("ctrl-t".to_string()));
    }
}

// Module-level acceptance tests - placed at loader module root for command compatibility
#[cfg(test)]
use std::fs;
#[cfg(test)]
use tempfile::TempDir;

#[cfg(test)]
#[test]
fn load_default() {
    // Test that default config can be loaded - inline implementation
    let default_path = std::path::Path::new(DEFAULT_CONFIG_PATH);
    let content = std::fs::read_to_string(default_path).expect("Default config should be readable");
    let config: KeybindingConfig = toml::from_str(&content).expect("Default config should parse");
    assert_eq!(config.navigation.move_left, "h");
    assert_eq!(config.navigation.move_right, "l");
}

#[cfg(test)]
#[test]
fn missing_file() {
    let temp_dir = TempDir::new().unwrap();
    let missing_path = temp_dir.path().join("nonexistent.toml");
    let result = load_keybindings(&missing_path);
    assert!(
        result.is_ok(),
        "Missing file should return default: {:?}",
        result.err()
    );
    let config = result.unwrap();
    assert_eq!(config.navigation.move_left, "h");
}

#[cfg(test)]
#[test]
fn malformed_toml() {
    let temp_dir = TempDir::new().unwrap();
    let bad_path = temp_dir.path().join("bad.toml");
    fs::write(&bad_path, "this is [not valid toml @@@").unwrap();
    let result = load_keybindings(&bad_path);
    assert!(result.is_err(), "Malformed TOML should return error");
    match result.unwrap_err() {
        LoadError::ParseError { path, .. } => {
            assert_eq!(path, bad_path);
        }
        other => panic!("Expected ParseError, got: {:?}", other),
    }
}
