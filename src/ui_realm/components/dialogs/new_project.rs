use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::projects;
use crate::ui_realm::messages::{DialogField, Msg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusField {
    Name,
    Create,
    Cancel,
}

pub struct NewProjectDialog {
    props: Props,
    name_input: String,
    focused_field: FocusField,
}

impl NewProjectDialog {
    pub fn new() -> Self {
        Self {
            props: Props::default(),
            name_input: String::new(),
            focused_field: FocusField::Name,
        }
    }

    fn focus_next(&mut self) -> Msg {
        self.focused_field = match self.focused_field {
            FocusField::Name => FocusField::Create,
            FocusField::Create => FocusField::Cancel,
            FocusField::Cancel => FocusField::Name,
        };
        Msg::FocusField(self.focused_dialog_field())
    }

    fn focus_previous(&mut self) -> Msg {
        self.focused_field = match self.focused_field {
            FocusField::Name => FocusField::Cancel,
            FocusField::Create => FocusField::Name,
            FocusField::Cancel => FocusField::Create,
        };
        Msg::FocusField(self.focused_dialog_field())
    }

    fn focused_dialog_field(&self) -> DialogField {
        match self.focused_field {
            FocusField::Name => DialogField::Name,
            FocusField::Create => DialogField::Create,
            FocusField::Cancel => DialogField::Cancel,
        }
    }

    fn preview_path(&self) -> String {
        let name = self.name_input.trim();
        if name.is_empty() {
            "Path: (enter project name)".to_string()
        } else {
            format!("Path: {}", projects::get_project_path(name).display())
        }
    }

    fn on_confirm(&self) -> Msg {
        match self.focused_field {
            FocusField::Cancel => Msg::DismissDialog,
            FocusField::Name | FocusField::Create => Msg::CreateProject,
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

    fn render_button(&self, frame: &mut Frame, area: Rect, label: &str, target: FocusField) {
        let is_focused = self.focused_field == target;
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

impl Default for NewProjectDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl MockComponent for NewProjectDialog {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" New Project ");
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
                Constraint::Length(1),
                Constraint::Length(3),
            ])
            .split(inner);

        self.render_input(frame, rows[0]);
        frame.render_widget(Paragraph::new(self.preview_path()), rows[2]);

        let buttons = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[3]);

        self.render_button(frame, buttons[0], "[ Create ]", FocusField::Create);
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
            FocusField::Create => 1,
            FocusField::Cancel => 2,
        };
        State::One(StateValue::U16(focused_index))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for NewProjectDialog {
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
            }) if self.focused_field == FocusField::Create => {
                self.focused_field = FocusField::Cancel;
                Some(Msg::FocusField(self.focused_dialog_field()))
            }
            Event::Keyboard(KeyEvent {
                code: Key::Right,
                modifiers: KeyModifiers::NONE,
            }) if self.focused_field == FocusField::Cancel => {
                self.focused_field = FocusField::Create;
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
        ComponentId::NewProject,
        Box::new(NewProjectDialog::new()),
    );

    let mut terminal = MockTerminal::new(72, 12);
    let output = render_component(&mut app, ComponentId::NewProject, &mut terminal);

    assert!(output.contains("New Project"), "dialog title should render");
    assert!(output.contains("Name"), "name input should render");
    assert!(output.contains("Path:"), "path preview should render");
    assert!(output.contains("[ Create ]"), "create action should render");
    assert!(output.contains("[ Cancel ]"), "cancel action should render");
}

#[cfg(test)]
#[test]
fn input_updates() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::NewProject,
        Box::new(NewProjectDialog::new()),
    );

    let typed = send_key_to_component(
        &driver,
        &mut app,
        &[KeyCode::Char('m'), KeyCode::Char('y'), KeyCode::Backspace],
        1,
    );
    assert!(typed.is_empty(), "typing should not emit messages");

    let focused = send_key_to_component(&driver, &mut app, &[KeyCode::Tab], 1);
    assert_eq!(focused, vec![Msg::FocusField(DialogField::Create)]);

    let ignored_char = send_key_to_component(&driver, &mut app, &[KeyCode::Char('z')], 1);
    assert!(
        ignored_char.is_empty(),
        "char input outside name field is ignored"
    );

    let mut terminal = MockTerminal::new(72, 12);
    let output = render_component(&mut app, ComponentId::NewProject, &mut terminal);
    assert!(
        output.contains("m"),
        "backspace should remove last character"
    );
    assert!(
        !output.contains("mz"),
        "name should not update after focus leaves field"
    );

    let no_panic = send_key_to_component(
        &driver,
        &mut mount_component_for_test(
            &driver,
            ComponentId::NewProject,
            Box::new(NewProjectDialog::new()),
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
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::NewProject,
        Box::new(NewProjectDialog::new()),
    );

    let create_messages =
        send_key_to_component(&driver, &mut app, &[KeyCode::Char('d'), KeyCode::Enter], 1);
    assert_eq!(create_messages, vec![Msg::CreateProject]);

    let cancel_focus = send_key_to_component(&driver, &mut app, &[KeyCode::Tab, KeyCode::Tab], 1);
    assert_eq!(
        cancel_focus,
        vec![
            Msg::FocusField(DialogField::Create),
            Msg::FocusField(DialogField::Cancel),
        ]
    );

    let cancel_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Enter], 1);
    assert_eq!(cancel_messages, vec![Msg::DismissDialog]);
}
