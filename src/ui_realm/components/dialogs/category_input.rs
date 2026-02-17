use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::ui_realm::messages::{DialogField, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategoryInputMode {
    New,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusField {
    Name,
    Confirm,
    Cancel,
}

pub struct CategoryInputDialog {
    props: Props,
    mode: CategoryInputMode,
    name_input: String,
    focused_field: FocusField,
}

impl CategoryInputDialog {
    pub fn new(mode: CategoryInputMode, initial_name: impl Into<String>) -> Self {
        Self {
            props: Props::default(),
            mode,
            name_input: initial_name.into(),
            focused_field: FocusField::Name,
        }
    }

    fn title(&self) -> &'static str {
        match self.mode {
            CategoryInputMode::New => " New Category ",
            CategoryInputMode::Rename => " Rename Category ",
        }
    }

    fn confirm_label(&self) -> &'static str {
        match self.mode {
            CategoryInputMode::New => "[ Create ]",
            CategoryInputMode::Rename => "[ Rename ]",
        }
    }

    fn focus_next(&mut self) -> Msg {
        self.focused_field = match self.focused_field {
            FocusField::Name => FocusField::Confirm,
            FocusField::Confirm => FocusField::Cancel,
            FocusField::Cancel => FocusField::Name,
        };
        Msg::FocusField(self.focused_dialog_field())
    }

    fn focus_previous(&mut self) -> Msg {
        self.focused_field = match self.focused_field {
            FocusField::Name => FocusField::Cancel,
            FocusField::Confirm => FocusField::Name,
            FocusField::Cancel => FocusField::Confirm,
        };
        Msg::FocusField(self.focused_dialog_field())
    }

    fn focused_dialog_field(&self) -> DialogField {
        match self.focused_field {
            FocusField::Name => DialogField::Name,
            FocusField::Confirm => DialogField::Confirm,
            FocusField::Cancel => DialogField::CancelCategory,
        }
    }

    fn on_confirm(&self) -> Msg {
        match self.focused_field {
            FocusField::Cancel => Msg::DismissDialog,
            FocusField::Name | FocusField::Confirm => Msg::SubmitCategoryInput,
        }
    }

    fn render_input(&self, frame: &mut Frame, area: Rect) {
        let border = if self.focused_field == FocusField::Name {
            Color::Yellow
        } else {
            Color::Gray
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Name ")
            .border_style(Style::default().fg(border));

        let paragraph = if self.focused_field == FocusField::Name {
            Paragraph::new(Line::from(vec![
                Span::raw(self.name_input.as_str()),
                Span::styled("█", Style::default().bg(Color::Yellow).fg(Color::Black)),
            ]))
        } else {
            Paragraph::new(self.name_input.as_str())
        };

        frame.render_widget(paragraph.block(block), area);
    }

    fn render_button(&self, frame: &mut Frame, area: Rect, label: &str, focused_field: FocusField) {
        let is_focused = self.focused_field == focused_field;
        let border = if is_focused {
            Color::Yellow
        } else {
            Color::Gray
        };
        let style = if is_focused {
            Style::default().bg(Color::Yellow).fg(Color::Black)
        } else {
            Style::default()
        };

        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(style)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(border)),
                ),
            area,
        );
    }
}

impl MockComponent for CategoryInputDialog {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(self.title());
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(3),
            ])
            .split(inner);

        self.render_input(frame, rows[0]);

        let buttons = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[2]);

        self.render_button(frame, buttons[0], self.confirm_label(), FocusField::Confirm);
        self.render_button(frame, buttons[1], "[ Cancel ]", FocusField::Cancel);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        let focused_index = match self.focused_field {
            FocusField::Name => 0,
            FocusField::Confirm => 1,
            FocusField::Cancel => 2,
        };
        State::One(StateValue::U16(focused_index))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for CategoryInputDialog {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::DismissDialog),
            Event::Keyboard(KeyEvent { code: Key::Tab, .. })
            | Event::Keyboard(KeyEvent {
                code: Key::Down,
                modifiers: KeyModifiers::NONE,
            }) => Some(self.focus_next()),
            Event::Keyboard(KeyEvent {
                code: Key::BackTab, ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Up,
                modifiers: KeyModifiers::NONE,
            }) => Some(self.focus_previous()),
            Event::Keyboard(KeyEvent {
                code: Key::Left,
                modifiers: KeyModifiers::NONE,
            }) if self.focused_field == FocusField::Confirm => {
                self.focused_field = FocusField::Cancel;
                Some(Msg::FocusField(self.focused_dialog_field()))
            }
            Event::Keyboard(KeyEvent {
                code: Key::Right,
                modifiers: KeyModifiers::NONE,
            }) if self.focused_field == FocusField::Cancel => {
                self.focused_field = FocusField::Confirm;
                Some(Msg::FocusField(self.focused_dialog_field()))
            }
            Event::Keyboard(KeyEvent {
                code: Key::Backspace,
                ..
            }) => {
                if self.focused_field == FocusField::Name {
                    self.name_input.pop();
                }
                None
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char(ch),
                modifiers,
            }) if self.focused_field == FocusField::Name
                && !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.name_input.push(ch);
                None
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => Some(self.on_confirm()),
            _ => None,
        }
    }
}

#[cfg(test)]
use crate::ui_realm::ComponentId;
#[cfg(test)]
use crate::ui_realm::tests::harness::{EventDriver, MockTerminal};
#[cfg(test)]
use crate::ui_realm::tests::helpers::{
    mount_component_for_test, render_component, send_key_to_component,
};
#[cfg(test)]
use crossterm::event::KeyCode;

#[cfg(test)]
#[test]
fn renders() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::CategoryInput,
        Box::new(CategoryInputDialog::new(CategoryInputMode::New, "")),
    );
    let mut terminal = MockTerminal::new(52, 11);
    let output = render_component(&mut app, ComponentId::CategoryInput, &mut terminal);

    assert!(
        output.contains("New Category"),
        "new mode title should render"
    );
    assert!(output.contains("[ Create ]"), "create button should render");
    assert!(output.contains("[ Cancel ]"), "cancel button should render");

    let mut rename_app = mount_component_for_test(
        &driver,
        ComponentId::CategoryInput,
        Box::new(CategoryInputDialog::new(
            CategoryInputMode::Rename,
            "Backlog",
        )),
    );
    let rename_output = render_component(
        &mut rename_app,
        ComponentId::CategoryInput,
        &mut MockTerminal::new(52, 11),
    );
    assert!(
        rename_output.contains("Rename Category"),
        "rename mode title should render"
    );
    assert!(
        rename_output.contains("[ Rename ]"),
        "rename button label should render"
    );
    assert!(
        rename_output.contains("Backlog"),
        "initial input value should render"
    );
}

#[cfg(test)]
#[test]
fn input_updates() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::CategoryInput,
        Box::new(CategoryInputDialog::new(CategoryInputMode::New, "")),
    );

    let typed = send_key_to_component(
        &driver,
        &mut app,
        &[KeyCode::Char('m'), KeyCode::Char('n'), KeyCode::Backspace],
        1,
    );
    assert!(typed.is_empty(), "typing should not emit messages");

    let focused = send_key_to_component(&driver, &mut app, &[KeyCode::Tab], 1);
    assert_eq!(focused, vec![Msg::FocusField(DialogField::Confirm)]);

    let ignored_char = send_key_to_component(&driver, &mut app, &[KeyCode::Char('z')], 1);
    assert!(
        ignored_char.is_empty(),
        "char input outside name field is ignored"
    );

    let mut terminal = MockTerminal::new(52, 11);
    let output = render_component(&mut app, ComponentId::CategoryInput, &mut terminal);
    assert!(
        output.contains("m"),
        "backspace should remove last character"
    );
    assert!(
        !output.contains("mz"),
        "input should not update after focus leaves name"
    );

    let no_panic = send_key_to_component(
        &driver,
        &mut mount_component_for_test(
            &driver,
            ComponentId::CategoryInput,
            Box::new(CategoryInputDialog::new(CategoryInputMode::New, "")),
        ),
        &[KeyCode::Backspace, KeyCode::Backspace],
        1,
    );
    assert!(
        no_panic.is_empty(),
        "backspace on empty input should not panic or emit"
    );
}

#[cfg(test)]
#[test]
fn confirm_emits_msg() {
    let driver = EventDriver::default();
    let mut create_app = mount_component_for_test(
        &driver,
        ComponentId::CategoryInput,
        Box::new(CategoryInputDialog::new(CategoryInputMode::New, "Ideas")),
    );
    let create_messages = send_key_to_component(&driver, &mut create_app, &[KeyCode::Enter], 1);
    assert_eq!(create_messages, vec![Msg::SubmitCategoryInput]);

    let rename_driver = EventDriver::default();
    let mut rename_app = mount_component_for_test(
        &rename_driver,
        ComponentId::CategoryInput,
        Box::new(CategoryInputDialog::new(CategoryInputMode::Rename, "Todo")),
    );
    let _ = send_key_to_component(&rename_driver, &mut rename_app, &[KeyCode::Tab], 2);
    let rename_messages =
        send_key_to_component(&rename_driver, &mut rename_app, &[KeyCode::Enter], 2);
    assert_eq!(rename_messages, vec![Msg::SubmitCategoryInput]);

    let cancel_driver = EventDriver::default();
    let mut cancel_app = mount_component_for_test(
        &cancel_driver,
        ComponentId::CategoryInput,
        Box::new(CategoryInputDialog::new(CategoryInputMode::Rename, "Todo")),
    );
    let _ = send_key_to_component(&cancel_driver, &mut cancel_app, &[KeyCode::Tab], 2);
    let _ = send_key_to_component(&cancel_driver, &mut cancel_app, &[KeyCode::Tab], 2);
    let cancel_messages =
        send_key_to_component(&cancel_driver, &mut cancel_app, &[KeyCode::Enter], 2);
    assert_eq!(cancel_messages, vec![Msg::DismissDialog]);
}
