use std::collections::HashMap;
use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::keybindings::loader::{LoadError, load_keybindings};
use crate::keybindings::schema::KeybindingConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    NavigateLeft,
    NavigateRight,
    SelectUp,
    SelectDown,
    MoveTaskLeft,
    MoveTaskRight,
    MoveTaskUp,
    MoveTaskDown,
    OpenNewTaskDialog,
    AttachTask,
    OpenDeleteTaskDialog,
    OpenAddCategoryDialog,
    OpenRenameCategoryDialog,
    OpenDeleteCategoryDialog,
    CycleCategoryColor,
    OpenCommandPalette,
    ToggleHelp,
    DismissDialog,
    ConfirmAction,
    CancelAction,
    Quit,
    ReturnToTmux,
    OpenProjectList,
    ToggleViewMode,
    Submit,
    ToggleCheckbox,
}

#[derive(Debug, Clone)]
pub struct KeybindingDispatcher {
    config: KeybindingConfig,
    bindings: HashMap<KeyChord, Action>,
}

impl KeybindingDispatcher {
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let config = load_keybindings(path)?;
        Ok(Self::from_config(config))
    }

    pub fn from_config(config: KeybindingConfig) -> Self {
        let mut bindings = HashMap::new();

        register(
            &mut bindings,
            &config.navigation.move_left,
            Action::NavigateLeft,
        );
        register_optional(
            &mut bindings,
            config.navigation.move_left_alt.as_deref(),
            Action::NavigateLeft,
        );
        register(
            &mut bindings,
            &config.navigation.move_right,
            Action::NavigateRight,
        );
        register_optional(
            &mut bindings,
            config.navigation.move_right_alt.as_deref(),
            Action::NavigateRight,
        );
        register(
            &mut bindings,
            &config.navigation.select_up,
            Action::SelectUp,
        );
        register_optional(
            &mut bindings,
            config.navigation.select_up_alt.as_deref(),
            Action::SelectUp,
        );
        register(
            &mut bindings,
            &config.navigation.select_down,
            Action::SelectDown,
        );
        register_optional(
            &mut bindings,
            config.navigation.select_down_alt.as_deref(),
            Action::SelectDown,
        );
        register(
            &mut bindings,
            &config.navigation.category_move_left,
            Action::MoveTaskLeft,
        );
        register(
            &mut bindings,
            &config.navigation.category_move_right,
            Action::MoveTaskRight,
        );
        register(
            &mut bindings,
            &config.navigation.task_move_up,
            Action::MoveTaskUp,
        );
        register(
            &mut bindings,
            &config.navigation.task_move_down,
            Action::MoveTaskDown,
        );

        register(
            &mut bindings,
            &config.tasks.new_task,
            Action::OpenNewTaskDialog,
        );
        register(&mut bindings, &config.tasks.attach, Action::AttachTask);
        register_optional(
            &mut bindings,
            config.tasks.attach_alt.as_deref(),
            Action::AttachTask,
        );
        register(
            &mut bindings,
            &config.tasks.delete_task,
            Action::OpenDeleteTaskDialog,
        );

        register(
            &mut bindings,
            &config.categories.add,
            Action::OpenAddCategoryDialog,
        );
        register(
            &mut bindings,
            &config.categories.rename,
            Action::OpenRenameCategoryDialog,
        );
        register(
            &mut bindings,
            &config.categories.delete,
            Action::OpenDeleteCategoryDialog,
        );
        register(
            &mut bindings,
            &config.categories.cycle_color,
            Action::CycleCategoryColor,
        );

        register(
            &mut bindings,
            &config.dialogs.command_palette,
            Action::OpenCommandPalette,
        );
        register(&mut bindings, &config.dialogs.help, Action::ToggleHelp);
        register(
            &mut bindings,
            &config.dialogs.dismiss,
            Action::DismissDialog,
        );
        register_optional(
            &mut bindings,
            config.dialogs.dismiss_alt.as_deref(),
            Action::DismissDialog,
        );
        register(
            &mut bindings,
            &config.dialogs.confirm,
            Action::ConfirmAction,
        );
        register(&mut bindings, &config.dialogs.cancel, Action::CancelAction);

        register(&mut bindings, &config.global.quit, Action::Quit);
        register(
            &mut bindings,
            &config.global.return_tmux,
            Action::ReturnToTmux,
        );
        register(
            &mut bindings,
            &config.global.project_list,
            Action::OpenProjectList,
        );
        register(
            &mut bindings,
            &config.global.toggle_view,
            Action::ToggleViewMode,
        );
        register(&mut bindings, &config.global.submit, Action::Submit);
        register(
            &mut bindings,
            &config.global.toggle_checkbox,
            Action::ToggleCheckbox,
        );

        Self { config, bindings }
    }

    pub fn config(&self) -> &KeybindingConfig {
        &self.config
    }

    pub fn map_key(&self, key: KeyEvent) -> Option<Action> {
        let chord = KeyChord::from_event(key);
        self.bindings.get(&chord).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct KeyChord {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl KeyChord {
    fn from_event(event: KeyEvent) -> Self {
        let (code, modifiers) = normalize_code_and_modifiers(event.code, event.modifiers);
        Self { code, modifiers }
    }

    fn parse(input: &str) -> Option<Self> {
        let value = input.trim();
        if value.is_empty() {
            return None;
        }

        let mut modifiers = KeyModifiers::NONE;
        let mut parts = value.split('-').peekable();
        while let Some(part) = parts.peek().copied() {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => {
                    modifiers.insert(KeyModifiers::CONTROL);
                    parts.next();
                }
                "shift" => {
                    modifiers.insert(KeyModifiers::SHIFT);
                    parts.next();
                }
                "alt" => {
                    modifiers.insert(KeyModifiers::ALT);
                    parts.next();
                }
                _ => break,
            }
        }

        let key_part = parts.collect::<Vec<_>>().join("-");
        let code = parse_key_code(&key_part)?;

        let (code, modifiers) = normalize_code_and_modifiers(code, modifiers);
        Some(Self { code, modifiers })
    }
}

fn parse_key_code(value: &str) -> Option<KeyCode> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "enter" => Some(KeyCode::Enter),
        "esc" | "escape" => Some(KeyCode::Esc),
        "tab" => Some(KeyCode::Tab),
        "backtab" => Some(KeyCode::BackTab),
        "backspace" => Some(KeyCode::Backspace),
        "delete" | "del" => Some(KeyCode::Delete),
        "insert" | "ins" => Some(KeyCode::Insert),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "pageup" => Some(KeyCode::PageUp),
        "pagedown" => Some(KeyCode::PageDown),
        "space" => Some(KeyCode::Char(' ')),
        _ => {
            if let Some(number) = lower.strip_prefix('f')
                && let Ok(value) = number.parse::<u8>()
            {
                return Some(KeyCode::F(value));
            }
            let mut chars = value.chars();
            let ch = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Some(KeyCode::Char(ch))
        }
    }
}

fn normalize_code_and_modifiers(code: KeyCode, modifiers: KeyModifiers) -> (KeyCode, KeyModifiers) {
    match code {
        KeyCode::Char(ch) if ch.is_ascii_uppercase() => {
            let mut normalized_modifiers = modifiers;
            normalized_modifiers.insert(KeyModifiers::SHIFT);
            (KeyCode::Char(ch.to_ascii_lowercase()), normalized_modifiers)
        }
        _ => (code, modifiers),
    }
}

fn register(bindings: &mut HashMap<KeyChord, Action>, key: &str, action: Action) {
    if let Some(chord) = KeyChord::parse(key) {
        bindings.entry(chord).or_insert(action);
    }
}

fn register_optional(bindings: &mut HashMap<KeyChord, Action>, key: Option<&str>, action: Action) {
    if let Some(key) = key {
        register(bindings, key, action);
    }
}

#[cfg(test)]
fn test_dispatcher() -> KeybindingDispatcher {
    let config = load_keybindings(Path::new("config/keybindings.toml"))
        .expect("default keybinding config should load");
    KeybindingDispatcher::from_config(config)
}

#[cfg(test)]
#[test]
fn map_quit_key() {
    let dispatcher = test_dispatcher();
    let mapped = dispatcher.map_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert_eq!(mapped, Some(Action::Quit));
}

#[cfg(test)]
#[test]
fn map_new_task_key() {
    let dispatcher = test_dispatcher();
    let mapped = dispatcher.map_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
    assert_eq!(mapped, Some(Action::OpenNewTaskDialog));
}

#[cfg(test)]
#[test]
fn unbound_key_returns_none() {
    let dispatcher = test_dispatcher();
    let mapped = dispatcher.map_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    assert_eq!(mapped, None);
}

#[cfg(test)]
#[test]
fn map_ctrl_modified_key() {
    let dispatcher = test_dispatcher();
    let mapped = dispatcher.map_key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert_eq!(mapped, Some(Action::OpenCommandPalette));
}

#[cfg(test)]
#[test]
fn map_shifted_key() {
    let dispatcher = test_dispatcher();
    let mapped = dispatcher.map_key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::SHIFT));
    assert_eq!(mapped, Some(Action::MoveTaskLeft));
}

#[cfg(test)]
#[test]
fn map_alt_modified_key_from_config() {
    let mut config = test_dispatcher().config().clone();
    config.tasks.attach_alt = Some("alt-a".to_string());

    let dispatcher = KeybindingDispatcher::from_config(config);
    let mapped = dispatcher.map_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT));
    assert_eq!(mapped, Some(Action::AttachTask));
}
