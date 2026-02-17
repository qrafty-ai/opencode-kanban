//! Keybinding configuration schema
//!
//! Defines serde-serializable structs for parsing keybinding TOML config.
//! Schema is designed to be loader-friendly for future dynamic reloading.

use serde::{Deserialize, Serialize};

/// Root keybinding configuration containing all action sections
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct KeybindingConfig {
    pub navigation: NavigationBindings,
    pub tasks: TaskBindings,
    pub categories: CategoryBindings,
    pub dialogs: DialogBindings,
    pub global: GlobalBindings,
}

/// Navigation keybindings for column and task selection
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct NavigationBindings {
    pub move_left: String,
    #[serde(default)]
    pub move_left_alt: Option<String>,
    pub move_right: String,
    #[serde(default)]
    pub move_right_alt: Option<String>,
    pub select_up: String,
    #[serde(default)]
    pub select_up_alt: Option<String>,
    pub select_down: String,
    #[serde(default)]
    pub select_down_alt: Option<String>,
    pub category_move_left: String,
    pub category_move_right: String,
    pub task_move_up: String,
    pub task_move_down: String,
}

/// Task action keybindings
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TaskBindings {
    pub new_task: String,
    pub attach: String,
    #[serde(default)]
    pub attach_alt: Option<String>,
    pub delete_task: String,
}

/// Category management keybindings
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CategoryBindings {
    pub add: String,
    pub rename: String,
    pub delete: String,
    pub cycle_color: String,
}

/// Dialog interaction keybindings
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct DialogBindings {
    pub command_palette: String,
    pub help: String,
    pub dismiss: String,
    #[serde(default)]
    pub dismiss_alt: Option<String>,
    pub confirm: String,
    pub cancel: String,
}

/// Global application keybindings
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct GlobalBindings {
    pub quit: String,
    pub return_tmux: String,
    pub project_list: String,
    pub toggle_view: String,
    pub submit: String,
    pub toggle_checkbox: String,
}

#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
#[test]
fn parse_default() {
    let config_path = PathBuf::from("config/keybindings.toml");

    let config_content = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("Failed to read config/keybindings.toml: {}", e));

    let config: KeybindingConfig = toml::from_str(&config_content)
        .unwrap_or_else(|e| panic!("Failed to parse config/keybindings.toml: {}", e));

    assert_eq!(
        config.navigation.move_left, "h",
        "Navigation move_left should be 'h'"
    );
    assert_eq!(
        config.navigation.move_right, "l",
        "Navigation move_right should be 'l'"
    );
    assert_eq!(
        config.navigation.select_up, "k",
        "Navigation select_up should be 'k'"
    );
    assert_eq!(
        config.navigation.select_down, "j",
        "Navigation select_down should be 'j'"
    );
    assert_eq!(
        config.navigation.category_move_left, "H",
        "Category move left should be 'H'"
    );
    assert_eq!(
        config.navigation.category_move_right, "L",
        "Category move right should be 'L'"
    );
    assert_eq!(config.tasks.new_task, "n", "Task new_task should be 'n'");
    assert_eq!(
        config.tasks.attach, "enter",
        "Task attach should be 'enter'"
    );
    assert_eq!(config.categories.add, "c", "Category add should be 'c'");
    assert_eq!(
        config.categories.rename, "r",
        "Category rename should be 'r'"
    );
    assert_eq!(
        config.categories.delete, "x",
        "Category delete should be 'x'"
    );
    assert_eq!(
        config.dialogs.command_palette, "ctrl-p",
        "Command palette should be 'ctrl-p'"
    );
    assert_eq!(config.dialogs.help, "?", "Help should be '?'");
    assert_eq!(config.dialogs.dismiss, "esc", "Dismiss should be 'esc'");
    assert_eq!(config.global.quit, "q", "Global quit should be 'q'");
    assert!(
        !config.navigation.move_left.is_empty(),
        "navigation.move_left must not be empty"
    );
    assert!(
        !config.navigation.move_right.is_empty(),
        "navigation.move_right must not be empty"
    );
    assert!(
        !config.tasks.new_task.is_empty(),
        "tasks.new_task must not be empty"
    );
    assert!(
        !config.categories.add.is_empty(),
        "categories.add must not be empty"
    );
    assert!(
        !config.dialogs.help.is_empty(),
        "dialogs.help must not be empty"
    );
    assert!(
        !config.global.quit.is_empty(),
        "global.quit must not be empty"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keybinding_config_deserialize() {
        let toml_str = r#"
[navigation]
move_left = "h"
move_left_alt = "left"
move_right = "l"
move_right_alt = "right"
select_up = "k"
select_up_alt = "up"
select_down = "j"
select_down_alt = "down"
category_move_left = "H"
category_move_right = "L"
task_move_up = "K"
task_move_down = "J"

[tasks]
new_task = "n"
attach = "enter"
attach_alt = "ctrl-e"
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
dismiss_alt = "ctrl-c"
confirm = "enter"
cancel = "esc"

[global]
quit = "q"
return_tmux = "ctrl-k"
project_list = "ctrl-o"
toggle_view = "\\"
submit = "enter"
toggle_checkbox = "space"
"#;
        let config: KeybindingConfig = toml::from_str(toml_str).expect("Failed to parse TOML");
        assert_eq!(config.navigation.move_left, "h");
        assert_eq!(config.navigation.move_right, "l");
        assert_eq!(config.tasks.new_task, "n");
        assert_eq!(config.categories.add, "c");
        assert_eq!(config.dialogs.command_palette, "ctrl-p");
        assert_eq!(config.global.quit, "q");
    }

    #[test]
    fn test_keybinding_config_roundtrip() {
        let config = KeybindingConfig {
            navigation: NavigationBindings {
                move_left: "h".to_string(),
                move_left_alt: Some("left".to_string()),
                move_right: "l".to_string(),
                move_right_alt: Some("right".to_string()),
                select_up: "k".to_string(),
                select_up_alt: Some("up".to_string()),
                select_down: "j".to_string(),
                select_down_alt: Some("down".to_string()),
                category_move_left: "H".to_string(),
                category_move_right: "L".to_string(),
                task_move_up: "K".to_string(),
                task_move_down: "J".to_string(),
            },
            tasks: TaskBindings {
                new_task: "n".to_string(),
                attach: "enter".to_string(),
                attach_alt: Some("ctrl-e".to_string()),
                delete_task: "d".to_string(),
            },
            categories: CategoryBindings {
                add: "c".to_string(),
                rename: "r".to_string(),
                delete: "x".to_string(),
                cycle_color: "t".to_string(),
            },
            dialogs: DialogBindings {
                command_palette: "ctrl-p".to_string(),
                help: "?".to_string(),
                dismiss: "esc".to_string(),
                dismiss_alt: Some("ctrl-c".to_string()),
                confirm: "enter".to_string(),
                cancel: "esc".to_string(),
            },
            global: GlobalBindings {
                quit: "q".to_string(),
                return_tmux: "ctrl-k".to_string(),
                project_list: "ctrl-o".to_string(),
                toggle_view: "\\".to_string(),
                submit: "enter".to_string(),
                toggle_checkbox: "space".to_string(),
            },
        };
        let serialized = toml::to_string(&config).expect("Failed to serialize");
        let deserialized: KeybindingConfig =
            toml::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(config, deserialized);
    }
}
