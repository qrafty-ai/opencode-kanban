use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::Rect;
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::ui_realm::messages::{DialogField, Msg};

pub struct NewTaskDialog {
    props: Props,
    repo_input: String,
    branch_input: String,
    base_input: String,
    title_input: String,
    ensure_base_up_to_date: bool,
    focused_field: DialogField,
}

impl NewTaskDialog {
    pub fn new() -> Self {
        Self {
            props: Props::default(),
            repo_input: String::new(),
            branch_input: String::new(),
            base_input: String::new(),
            title_input: String::new(),
            ensure_base_up_to_date: true,
            focused_field: DialogField::Repo,
        }
    }

    fn focus_order() -> &'static [DialogField] {
        &[
            DialogField::Repo,
            DialogField::Branch,
            DialogField::Base,
            DialogField::Title,
            DialogField::EnsureBaseUpToDate,
            DialogField::Create,
            DialogField::Cancel,
        ]
    }

    fn focused_index(&self) -> usize {
        Self::focus_order()
            .iter()
            .position(|field| field == &self.focused_field)
            .unwrap_or(0)
    }

    fn set_focused_by_index(&mut self, index: usize) -> DialogField {
        let order = Self::focus_order();
        let field = order[index % order.len()].clone();
        self.focused_field = field.clone();
        field
    }

    fn focus_next(&mut self) -> DialogField {
        self.set_focused_by_index(self.focused_index() + 1)
    }

    fn focus_previous(&mut self) -> DialogField {
        let order_len = Self::focus_order().len();
        let current = self.focused_index();
        let previous = if current == 0 {
            order_len - 1
        } else {
            current - 1
        };
        self.set_focused_by_index(previous)
    }

    fn focus_create_or_cancel(&mut self, target: DialogField) -> DialogField {
        self.focused_field = target.clone();
        target
    }

    fn active_text_input_mut(&mut self) -> Option<&mut String> {
        match self.focused_field {
            DialogField::Repo => Some(&mut self.repo_input),
            DialogField::Branch => Some(&mut self.branch_input),
            DialogField::Base => Some(&mut self.base_input),
            DialogField::Title => Some(&mut self.title_input),
            _ => None,
        }
    }

    fn toggle_ensure_base(&mut self) {
        self.ensure_base_up_to_date = !self.ensure_base_up_to_date;
    }

    fn focused_line(label: &str, value: &str, focused: bool) -> Line<'static> {
        let marker = if focused { ">" } else { " " };
        let style = if focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        Line::from(vec![
            Span::styled(format!("{marker} {label}: "), style),
            Span::styled(value.to_string(), style),
        ])
    }

    fn focused_checkbox_line(&self) -> Line<'static> {
        let marker = if self.focused_field == DialogField::EnsureBaseUpToDate {
            ">"
        } else {
            " "
        };
        let check = if self.ensure_base_up_to_date {
            "[x]"
        } else {
            "[ ]"
        };
        let style = if self.focused_field == DialogField::EnsureBaseUpToDate {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        Line::from(Span::styled(
            format!("{marker} Ensure base up-to-date: {check}"),
            style,
        ))
    }

    fn focused_actions_line(&self) -> Line<'static> {
        let create_style = if self.focused_field == DialogField::Create {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default()
        };
        let cancel_style = if self.focused_field == DialogField::Cancel {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default()
        };
        Line::from(vec![
            Span::raw("  "),
            Span::styled("[ Create ]", create_style),
            Span::raw("  "),
            Span::styled("[ Cancel ]", cancel_style),
        ])
    }
}

impl Default for NewTaskDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl MockComponent for NewTaskDialog {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" New Task ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let repo_value = if self.repo_input.is_empty() {
            ""
        } else {
            self.repo_input.as_str()
        };
        let branch_value = if self.branch_input.is_empty() {
            ""
        } else {
            self.branch_input.as_str()
        };
        let base_value = if self.base_input.is_empty() {
            ""
        } else {
            self.base_input.as_str()
        };
        let title_value = if self.title_input.is_empty() {
            ""
        } else {
            self.title_input.as_str()
        };

        let lines = vec![
            Self::focused_line("Repo", repo_value, self.focused_field == DialogField::Repo),
            Self::focused_line(
                "Branch",
                branch_value,
                self.focused_field == DialogField::Branch,
            ),
            Self::focused_line("Base", base_value, self.focused_field == DialogField::Base),
            Self::focused_line(
                "Title",
                title_value,
                self.focused_field == DialogField::Title,
            ),
            self.focused_checkbox_line(),
            Line::default(),
            self.focused_actions_line(),
        ];

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.focused_index() as u16))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for NewTaskDialog {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Tab, .. })
            | Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => Some(Msg::FocusField(self.focus_next())),
            Event::Keyboard(KeyEvent {
                code: Key::BackTab, ..
            })
            | Event::Keyboard(KeyEvent { code: Key::Up, .. }) => {
                Some(Msg::FocusField(self.focus_previous()))
            }
            Event::Keyboard(KeyEvent {
                code: Key::Left, ..
            }) => match self.focused_field {
                DialogField::Cancel => Some(Msg::FocusField(
                    self.focus_create_or_cancel(DialogField::Create),
                )),
                DialogField::Create => Some(Msg::FocusField(
                    self.focus_create_or_cancel(DialogField::Cancel),
                )),
                _ => None,
            },
            Event::Keyboard(KeyEvent {
                code: Key::Right, ..
            }) => match self.focused_field {
                DialogField::Create => Some(Msg::FocusField(
                    self.focus_create_or_cancel(DialogField::Cancel),
                )),
                DialogField::Cancel => Some(Msg::FocusField(
                    self.focus_create_or_cancel(DialogField::Create),
                )),
                _ => None,
            },
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => match self.focused_field {
                DialogField::Create => Some(Msg::CreateTask),
                DialogField::Cancel => Some(Msg::DismissDialog),
                DialogField::EnsureBaseUpToDate => {
                    self.toggle_ensure_base();
                    Some(Msg::ToggleCheckbox(DialogField::EnsureBaseUpToDate))
                }
                _ => Some(Msg::FocusField(self.focus_next())),
            },
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::DismissDialog),
            Event::Keyboard(KeyEvent {
                code: Key::Backspace,
                ..
            }) => {
                if let Some(input) = self.active_text_input_mut() {
                    input.pop();
                }
                None
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char(' '),
                ..
            }) => {
                if self.focused_field == DialogField::EnsureBaseUpToDate {
                    self.toggle_ensure_base();
                    Some(Msg::ToggleCheckbox(DialogField::EnsureBaseUpToDate))
                } else if let Some(input) = self.active_text_input_mut() {
                    input.push(' ');
                    None
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char(ch),
                ..
            }) => {
                if let Some(input) = self.active_text_input_mut() {
                    input.push(ch);
                }
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
use crate::ui_realm::ComponentId;
#[cfg(test)]
use crate::ui_realm::tests::harness::EventDriver;
#[cfg(test)]
use crate::ui_realm::tests::helpers::{
    mount_component_for_test, render_simple_component, send_key_to_component,
};
#[cfg(test)]
use crossterm::event::KeyCode;

#[cfg(test)]
#[test]
fn renders() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::NewTask,
        Box::new(NewTaskDialog::new()),
    );

    let output = render_simple_component(&mut app, ComponentId::NewTask);
    assert!(output.contains("New Task"), "dialog title should render");
    assert!(output.contains("Repo:"), "repo field should render");
    assert!(output.contains("Branch:"), "branch field should render");
    assert!(output.contains("Base:"), "base field should render");
    assert!(output.contains("Title:"), "title field should render");
    assert!(output.contains("[ Create ]"), "create action should render");
    assert!(output.contains("[ Cancel ]"), "cancel action should render");
}

#[cfg(test)]
#[test]
fn focus_navigation() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::NewTask,
        Box::new(NewTaskDialog::new()),
    );

    let messages = send_key_to_component(
        &driver,
        &mut app,
        &[KeyCode::Down, KeyCode::Down, KeyCode::BackTab],
        1,
    );

    assert_eq!(
        messages,
        vec![
            Msg::FocusField(DialogField::Branch),
            Msg::FocusField(DialogField::Base),
            Msg::FocusField(DialogField::Branch),
        ],
        "focus movement should cycle deterministically between fields"
    );
}

#[cfg(test)]
#[test]
fn confirm_emits_msg() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::NewTask,
        Box::new(NewTaskDialog::new()),
    );

    let confirm_messages = send_key_to_component(
        &driver,
        &mut app,
        &[
            KeyCode::Tab,
            KeyCode::Tab,
            KeyCode::Tab,
            KeyCode::Tab,
            KeyCode::Tab,
            KeyCode::Enter,
        ],
        1,
    );
    assert!(
        confirm_messages.contains(&Msg::CreateTask),
        "enter on create action should emit CreateTask"
    );

    let cancel_messages =
        send_key_to_component(&driver, &mut app, &[KeyCode::Right, KeyCode::Enter], 1);
    assert!(
        cancel_messages.contains(&Msg::DismissDialog),
        "enter on cancel action should emit DismissDialog"
    );
}
