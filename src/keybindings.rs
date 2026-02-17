use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum KeyContext {
    Global,
    ProjectList,
    Board,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum KeyAction {
    ToggleHelp,
    OpenPalette,
    Quit,
    ToggleView,
    ShrinkPanel,
    ExpandPanel,
    ProjectUp,
    ProjectDown,
    ProjectConfirm,
    NewProject,
    NavigateLeft,
    NavigateRight,
    SelectDown,
    SelectUp,
    NewTask,
    AddCategory,
    CycleCategoryColor,
    RenameCategory,
    DeleteCategory,
    DeleteTask,
    MoveTaskLeft,
    MoveTaskRight,
    MoveTaskDown,
    MoveTaskUp,
    AttachTask,
    CycleTodoVisualization,
    Dismiss,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBinding {
    fn matches(&self, key: KeyEvent) -> bool {
        match (&self.code, key.code) {
            (KeyCode::Char(left), KeyCode::Char(right)) => {
                let left = normalize_char(*left, self.modifiers);
                let right = normalize_char(right, key.modifiers);
                if left != right {
                    return false;
                }
                normalize_modifiers(self.modifiers) == normalize_modifiers(key.modifiers)
            }
            _ => self.code == key.code && self.modifiers == key.modifiers,
        }
    }
}

impl fmt::Display for KeyBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.contains(KeyModifiers::ALT) {
            parts.push("Alt".to_string());
        }
        if self.modifiers.contains(KeyModifiers::SHIFT) {
            parts.push("Shift".to_string());
        }

        parts.push(match self.code {
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "BackTab".to_string(),
            KeyCode::Backspace => "Backspace".to_string(),
            KeyCode::Left => "Left".to_string(),
            KeyCode::Right => "Right".to_string(),
            KeyCode::Up => "Up".to_string(),
            KeyCode::Down => "Down".to_string(),
            KeyCode::Home => "Home".to_string(),
            KeyCode::End => "End".to_string(),
            KeyCode::PageUp => "PageUp".to_string(),
            KeyCode::PageDown => "PageDown".to_string(),
            KeyCode::Delete => "Delete".to_string(),
            KeyCode::Insert => "Insert".to_string(),
            KeyCode::F(n) => format!("F{n}"),
            KeyCode::Char(ch) => ch.to_string(),
            KeyCode::Null => "Null".to_string(),
            _ => "Unknown".to_string(),
        });

        write!(f, "{}", parts.join("+"))
    }
}

#[derive(Debug, Clone)]
pub struct ActionBinding {
    pub id: &'static str,
    pub action: KeyAction,
    pub description: &'static str,
    pub bindings: Vec<KeyBinding>,
}

#[derive(Debug, Clone)]
pub struct Keybindings {
    global: Vec<ActionBinding>,
    project_list: Vec<ActionBinding>,
    board: Vec<ActionBinding>,
}

#[derive(Debug, Deserialize, Default)]
struct KeybindingsFile {
    #[serde(default)]
    global: HashMap<String, Vec<String>>,
    #[serde(default)]
    project_list: HashMap<String, Vec<String>>,
    #[serde(default)]
    board: HashMap<String, Vec<String>>,
}

struct ActionDef {
    id: &'static str,
    action: KeyAction,
    description: &'static str,
    defaults: &'static [&'static str],
}

const GLOBAL_DEFS: &[ActionDef] = &[
    ActionDef {
        id: "toggle_help",
        action: KeyAction::ToggleHelp,
        description: "toggle help",
        defaults: &["?"],
    },
    ActionDef {
        id: "open_palette",
        action: KeyAction::OpenPalette,
        description: "open command palette",
        defaults: &["Ctrl+P"],
    },
    ActionDef {
        id: "quit",
        action: KeyAction::Quit,
        description: "quit",
        defaults: &["q"],
    },
    ActionDef {
        id: "toggle_view",
        action: KeyAction::ToggleView,
        description: "toggle side panel",
        defaults: &["v"],
    },
    ActionDef {
        id: "shrink_panel",
        action: KeyAction::ShrinkPanel,
        description: "narrow side panel",
        defaults: &["<"],
    },
    ActionDef {
        id: "expand_panel",
        action: KeyAction::ExpandPanel,
        description: "widen side panel",
        defaults: &[">"],
    },
];

const PROJECT_LIST_DEFS: &[ActionDef] = &[
    ActionDef {
        id: "up",
        action: KeyAction::ProjectUp,
        description: "select previous project",
        defaults: &["k", "Up"],
    },
    ActionDef {
        id: "down",
        action: KeyAction::ProjectDown,
        description: "select next project",
        defaults: &["j", "Down"],
    },
    ActionDef {
        id: "confirm",
        action: KeyAction::ProjectConfirm,
        description: "open project",
        defaults: &["Enter"],
    },
    ActionDef {
        id: "new_project",
        action: KeyAction::NewProject,
        description: "new project",
        defaults: &["n"],
    },
];

const BOARD_DEFS: &[ActionDef] = &[
    ActionDef {
        id: "navigate_left",
        action: KeyAction::NavigateLeft,
        description: "move focus left",
        defaults: &["h", "Left"],
    },
    ActionDef {
        id: "navigate_right",
        action: KeyAction::NavigateRight,
        description: "move focus right",
        defaults: &["l", "Right"],
    },
    ActionDef {
        id: "select_down",
        action: KeyAction::SelectDown,
        description: "select next task",
        defaults: &["j", "Down"],
    },
    ActionDef {
        id: "select_up",
        action: KeyAction::SelectUp,
        description: "select previous task",
        defaults: &["k", "Up"],
    },
    ActionDef {
        id: "new_task",
        action: KeyAction::NewTask,
        description: "new task",
        defaults: &["n"],
    },
    ActionDef {
        id: "add_category",
        action: KeyAction::AddCategory,
        description: "add category",
        defaults: &["c"],
    },
    ActionDef {
        id: "cycle_category_color",
        action: KeyAction::CycleCategoryColor,
        description: "cycle category color",
        defaults: &["p"],
    },
    ActionDef {
        id: "rename_category",
        action: KeyAction::RenameCategory,
        description: "rename category",
        defaults: &["r"],
    },
    ActionDef {
        id: "delete_category",
        action: KeyAction::DeleteCategory,
        description: "delete category",
        defaults: &["x"],
    },
    ActionDef {
        id: "delete_task",
        action: KeyAction::DeleteTask,
        description: "delete task",
        defaults: &["d"],
    },
    ActionDef {
        id: "move_task_left",
        action: KeyAction::MoveTaskLeft,
        description: "move task left",
        defaults: &["H"],
    },
    ActionDef {
        id: "move_task_right",
        action: KeyAction::MoveTaskRight,
        description: "move task right",
        defaults: &["L"],
    },
    ActionDef {
        id: "move_task_down",
        action: KeyAction::MoveTaskDown,
        description: "move task down",
        defaults: &["J"],
    },
    ActionDef {
        id: "move_task_up",
        action: KeyAction::MoveTaskUp,
        description: "move task up",
        defaults: &["K"],
    },
    ActionDef {
        id: "attach",
        action: KeyAction::AttachTask,
        description: "attach selected task",
        defaults: &["Enter"],
    },
    ActionDef {
        id: "cycle_todo_visualization",
        action: KeyAction::CycleTodoVisualization,
        description: "cycle todo visualization",
        defaults: &["t"],
    },
    ActionDef {
        id: "dismiss",
        action: KeyAction::Dismiss,
        description: "dismiss",
        defaults: &["Esc"],
    },
];

impl Keybindings {
    pub fn load() -> Self {
        let file = load_file();
        let mut keybindings = Self {
            global: build_section(KeyContext::Global, GLOBAL_DEFS, &file.global),
            project_list: build_section(
                KeyContext::ProjectList,
                PROJECT_LIST_DEFS,
                &file.project_list,
            ),
            board: build_section(KeyContext::Board, BOARD_DEFS, &file.board),
        };

        keybindings.validate_conflicts();
        keybindings
    }

    pub fn action_for_key(&self, context: KeyContext, key: KeyEvent) -> Option<KeyAction> {
        self.bindings_for(context)
            .iter()
            .find(|binding| {
                binding
                    .bindings
                    .iter()
                    .any(|candidate| candidate.matches(key))
            })
            .map(|binding| binding.action)
    }

    pub fn command_palette_keybinding(&self, command_id: &str) -> Option<String> {
        match command_id {
            "switch_project" => self.display_for(KeyContext::Global, KeyAction::OpenPalette),
            "new_task" => self.display_for(KeyContext::Board, KeyAction::NewTask),
            "attach_task" => self.display_for(KeyContext::Board, KeyAction::AttachTask),
            "add_category" => self.display_for(KeyContext::Board, KeyAction::AddCategory),
            "rename_category" => self.display_for(KeyContext::Board, KeyAction::RenameCategory),
            "delete_category" => self.display_for(KeyContext::Board, KeyAction::DeleteCategory),
            "delete_task" => self.display_for(KeyContext::Board, KeyAction::DeleteTask),
            "move_task_left" => self.display_for(KeyContext::Board, KeyAction::MoveTaskLeft),
            "move_task_right" => self.display_for(KeyContext::Board, KeyAction::MoveTaskRight),
            "move_task_up" => self.display_for(KeyContext::Board, KeyAction::MoveTaskUp),
            "move_task_down" => self.display_for(KeyContext::Board, KeyAction::MoveTaskDown),
            "navigate_left" => self.display_for(KeyContext::Board, KeyAction::NavigateLeft),
            "navigate_right" => self.display_for(KeyContext::Board, KeyAction::NavigateRight),
            "select_up" => self.display_for(KeyContext::Board, KeyAction::SelectUp),
            "select_down" => self.display_for(KeyContext::Board, KeyAction::SelectDown),
            "cycle_todo_visualization" => {
                self.display_for(KeyContext::Board, KeyAction::CycleTodoVisualization)
            }
            "help" => self.display_for(KeyContext::Global, KeyAction::ToggleHelp),
            "quit" => self.display_for(KeyContext::Global, KeyAction::Quit),
            _ => None,
        }
    }

    pub fn help_lines(&self) -> Vec<String> {
        vec![
            "Keyboard shortcuts".to_string(),
            String::new(),
            "Global".to_string(),
            format!(
                "  {}: open command palette",
                self.display_for(KeyContext::Global, KeyAction::OpenPalette)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: quit",
                self.display_for(KeyContext::Global, KeyAction::Quit)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: toggle help",
                self.display_for(KeyContext::Global, KeyAction::ToggleHelp)
                    .unwrap_or_else(|| "-".to_string())
            ),
            String::new(),
            "Project List".to_string(),
            format!(
                "  {}: select previous project",
                self.display_for(KeyContext::ProjectList, KeyAction::ProjectUp)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: select next project",
                self.display_for(KeyContext::ProjectList, KeyAction::ProjectDown)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: open project",
                self.display_for(KeyContext::ProjectList, KeyAction::ProjectConfirm)
                    .unwrap_or_else(|| "-".to_string())
            ),
            String::new(),
            "Board".to_string(),
            format!(
                "  {}: move focus left",
                self.display_for(KeyContext::Board, KeyAction::NavigateLeft)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: move focus right",
                self.display_for(KeyContext::Board, KeyAction::NavigateRight)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: select next task",
                self.display_for(KeyContext::Board, KeyAction::SelectDown)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: select previous task",
                self.display_for(KeyContext::Board, KeyAction::SelectUp)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: attach selected task",
                self.display_for(KeyContext::Board, KeyAction::AttachTask)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: cycle todo visualization",
                self.display_for(KeyContext::Board, KeyAction::CycleTodoVisualization)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: new task",
                self.display_for(KeyContext::Board, KeyAction::NewTask)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {}: add/rename/delete category",
                [
                    self.display_for(KeyContext::Board, KeyAction::AddCategory)
                        .unwrap_or_else(|| "-".to_string()),
                    self.display_for(KeyContext::Board, KeyAction::RenameCategory)
                        .unwrap_or_else(|| "-".to_string()),
                    self.display_for(KeyContext::Board, KeyAction::DeleteCategory)
                        .unwrap_or_else(|| "-".to_string()),
                ]
                .join(" / ")
            ),
            format!(
                "  {}: delete task",
                self.display_for(KeyContext::Board, KeyAction::DeleteTask)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {} / {}: move task left/right",
                self.display_for(KeyContext::Board, KeyAction::MoveTaskLeft)
                    .unwrap_or_else(|| "-".to_string()),
                self.display_for(KeyContext::Board, KeyAction::MoveTaskRight)
                    .unwrap_or_else(|| "-".to_string())
            ),
            format!(
                "  {} / {}: move task down/up",
                self.display_for(KeyContext::Board, KeyAction::MoveTaskDown)
                    .unwrap_or_else(|| "-".to_string()),
                self.display_for(KeyContext::Board, KeyAction::MoveTaskUp)
                    .unwrap_or_else(|| "-".to_string())
            ),
            String::new(),
            "Dialogs".to_string(),
            "  Enter: confirm".to_string(),
            "  Esc: cancel".to_string(),
        ]
    }

    fn display_for(&self, context: KeyContext, action: KeyAction) -> Option<String> {
        self.bindings_for(context)
            .iter()
            .find(|binding| binding.action == action)
            .map(|binding| {
                binding
                    .bindings
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
    }

    fn bindings_for(&self, context: KeyContext) -> &[ActionBinding] {
        match context {
            KeyContext::Global => &self.global,
            KeyContext::ProjectList => &self.project_list,
            KeyContext::Board => &self.board,
        }
    }

    fn validate_conflicts(&mut self) {
        for context in [
            KeyContext::Global,
            KeyContext::ProjectList,
            KeyContext::Board,
        ] {
            let mut seen: HashMap<String, &'static str> = HashMap::new();
            for binding in self.bindings_for(context) {
                for key in &binding.bindings {
                    let key_name = key.to_string();
                    if let Some(first_action) = seen.get(&key_name) {
                        warn!(
                            "keybinding conflict in {:?}: '{}' used by '{}' and '{}' (first wins)",
                            context, key_name, first_action, binding.id
                        );
                    } else {
                        seen.insert(key_name, binding.id);
                    }
                }
            }
        }
    }
}

fn build_section(
    context: KeyContext,
    defs: &[ActionDef],
    overrides: &HashMap<String, Vec<String>>,
) -> Vec<ActionBinding> {
    let mut output = Vec::new();
    for def in defs {
        let source = overrides.get(def.id).cloned().unwrap_or_else(|| {
            def.defaults
                .iter()
                .map(|binding| binding.to_string())
                .collect()
        });

        let mut parsed = Vec::new();
        for raw in source {
            match parse_binding(&raw) {
                Some(binding) => parsed.push(binding),
                None => warn!(
                    "invalid keybinding '{}' for action '{}' in {:?}; ignoring",
                    raw, def.id, context
                ),
            }
        }

        if parsed.is_empty() {
            warn!(
                "no valid keybindings for action '{}' in {:?}; falling back to defaults",
                def.id, context
            );
            parsed = def
                .defaults
                .iter()
                .filter_map(|raw| parse_binding(raw))
                .collect();
        }

        output.push(ActionBinding {
            id: def.id,
            action: def.action,
            description: def.description,
            bindings: parsed,
        });
    }
    output
}

fn load_file() -> KeybindingsFile {
    let Some(path) = config_path() else {
        return KeybindingsFile::default();
    };

    if !path.exists() {
        return KeybindingsFile::default();
    }

    match fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<KeybindingsFile>(&contents) {
            Ok(file) => file,
            Err(error) => {
                warn!(
                    "failed to parse keybindings config '{}': {}",
                    path.display(),
                    error
                );
                KeybindingsFile::default()
            }
        },
        Err(error) => {
            warn!(
                "failed to read keybindings config '{}': {}",
                path.display(),
                error
            );
            KeybindingsFile::default()
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let mut path = dirs::config_dir()?;
    path.push("opencode-kanban");
    path.push("keybindings.toml");
    Some(path)
}

fn normalize_modifiers(mut modifiers: KeyModifiers) -> KeyModifiers {
    modifiers.remove(KeyModifiers::SHIFT);
    modifiers
}

fn normalize_char(ch: char, modifiers: KeyModifiers) -> char {
    if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        ch.to_ascii_lowercase()
    } else {
        ch
    }
}

fn parse_binding(raw: &str) -> Option<KeyBinding> {
    let mut modifiers = KeyModifiers::empty();
    let mut key: Option<&str> = None;

    for part in raw.split('+').map(str::trim).filter(|s| !s.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers.insert(KeyModifiers::CONTROL),
            "alt" => modifiers.insert(KeyModifiers::ALT),
            "shift" => modifiers.insert(KeyModifiers::SHIFT),
            _ => {
                if key.is_some() {
                    return None;
                }
                key = Some(part);
            }
        }
    }

    let key = key?;
    let lower = key.to_ascii_lowercase();
    let code = match lower.as_str() {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "backspace" => KeyCode::Backspace,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "space" => KeyCode::Char(' '),
        _ if lower.starts_with('f') && lower.len() <= 3 => {
            let n = lower[1..].parse::<u8>().ok()?;
            KeyCode::F(n)
        }
        _ if key.chars().count() == 1 => {
            let ch = normalize_char(key.chars().next()?, modifiers);
            KeyCode::Char(ch)
        }
        _ => return None,
    };

    Some(KeyBinding { code, modifiers })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ctrl_binding() {
        let binding = parse_binding("Ctrl+P").expect("binding");
        assert_eq!(binding.code, KeyCode::Char('p'));
        assert!(binding.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn shifted_alpha_is_distinct() {
        let lower = parse_binding("h").expect("binding");
        let upper = parse_binding("H").expect("binding");
        assert_ne!(lower, upper);

        assert!(upper.matches(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT)));
        assert!(!upper.matches(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty())));
    }

    #[test]
    fn shifted_symbol_matches_without_shift_modifier() {
        let binding = parse_binding("?").expect("binding");
        assert!(binding.matches(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)));
    }

    #[test]
    fn parse_arrow_binding() {
        let binding = parse_binding("Left").expect("binding");
        assert_eq!(binding.code, KeyCode::Left);
        assert_eq!(binding.modifiers, KeyModifiers::empty());
    }

    #[test]
    fn invalid_binding_returns_none() {
        assert!(parse_binding("Ctrl+Alt+Shift+Left+Extra").is_none());
    }

    #[test]
    fn defaults_resolve_actions() {
        let keys = Keybindings::load();
        let action = keys.action_for_key(
            KeyContext::Global,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty()),
        );
        assert_eq!(action, Some(KeyAction::Quit));
    }

    #[test]
    fn defaults_include_cycle_todo_visualization() {
        let keys = Keybindings::load();
        let action = keys.action_for_key(
            KeyContext::Board,
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::empty()),
        );
        assert_eq!(action, Some(KeyAction::CycleTodoVisualization));
    }
}
