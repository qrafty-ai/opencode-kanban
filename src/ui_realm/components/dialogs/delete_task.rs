use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::ui_realm::messages::Msg;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DeleteTaskContext {
    pub title: String,
    pub repo_name: String,
    pub branch: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum DeleteTaskFocus {
    Yes,
    No,
}

pub struct DeleteTaskDialog {
    props: Props,
    selected_task: Option<DeleteTaskContext>,
    focused: DeleteTaskFocus,
}

impl DeleteTaskDialog {
    pub fn new(selected_task: Option<DeleteTaskContext>) -> Self {
        Self {
            props: Props::default(),
            selected_task,
            focused: DeleteTaskFocus::No,
        }
    }

    fn toggle_focus(&mut self) -> Msg {
        self.focused = match self.focused {
            DeleteTaskFocus::Yes => DeleteTaskFocus::No,
            DeleteTaskFocus::No => DeleteTaskFocus::Yes,
        };
        Msg::FocusButton(self.focused_label().to_string())
    }

    fn focus_yes(&mut self) -> Msg {
        self.focused = DeleteTaskFocus::Yes;
        Msg::FocusButton(self.focused_label().to_string())
    }

    fn focus_no(&mut self) -> Msg {
        self.focused = DeleteTaskFocus::No;
        Msg::FocusButton(self.focused_label().to_string())
    }

    fn focused_label(&self) -> &'static str {
        match self.focused {
            DeleteTaskFocus::Yes => "Yes",
            DeleteTaskFocus::No => "No",
        }
    }

    fn render_confirmation_text(&self) -> Vec<String> {
        match &self.selected_task {
            Some(task) => vec![
                format!("Delete \"{}\"?", task.title),
                format!("({}:{})", task.repo_name, task.branch),
            ],
            None => vec![
                "No task selected.".to_string(),
                "Nothing will be deleted.".to_string(),
            ],
        }
    }

    fn button_style(&self, button: DeleteTaskFocus) -> Style {
        if self.focused == button {
            Style::default().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::default()
        }
    }
}

impl MockComponent for DeleteTaskDialog {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Delete Task ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Length(3),
            ])
            .split(inner);

        let content = self.render_confirmation_text().join("\n");
        frame.render_widget(
            Paragraph::new(content).alignment(Alignment::Center),
            rows[0],
        );

        let button_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[2]);

        frame.render_widget(
            Paragraph::new("[ Yes ]")
                .alignment(Alignment::Center)
                .style(self.button_style(DeleteTaskFocus::Yes))
                .block(Block::default().borders(Borders::ALL)),
            button_row[0],
        );
        frame.render_widget(
            Paragraph::new("[ No ]")
                .alignment(Alignment::Center)
                .style(self.button_style(DeleteTaskFocus::No))
                .block(Block::default().borders(Borders::ALL)),
            button_row[1],
        );
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        let focused = match self.focused {
            DeleteTaskFocus::Yes => 0,
            DeleteTaskFocus::No => 1,
        };
        State::One(StateValue::U16(focused))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for DeleteTaskDialog {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Left, ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Right, ..
            })
            | Event::Keyboard(KeyEvent { code: Key::Tab, .. })
            | Event::Keyboard(KeyEvent {
                code: Key::BackTab, ..
            }) => Some(self.toggle_focus()),
            Event::Keyboard(KeyEvent {
                code: Key::Char('y'),
                ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('Y'),
                ..
            }) => Some(self.focus_yes()),
            Event::Keyboard(KeyEvent {
                code: Key::Char('n'),
                ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('N'),
                ..
            }) => Some(self.focus_no()),
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => match (self.focused, self.selected_task.is_some()) {
                (DeleteTaskFocus::Yes, true) => Some(Msg::ConfirmDeleteTask),
                _ => Some(Msg::DismissDialog),
            },
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::DismissDialog),
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
        ComponentId::DeleteTask,
        Box::new(DeleteTaskDialog::new(Some(DeleteTaskContext {
            title: "Fix flaky test".to_string(),
            repo_name: "kanban".to_string(),
            branch: "fix/flaky-test".to_string(),
        }))),
    );

    let output = render_simple_component(&mut app, ComponentId::DeleteTask);
    assert!(output.contains("Delete Task"), "dialog title should render");
    assert!(
        output.contains("Delete \"Fix flaky test\"?"),
        "task title should render in confirmation text"
    );
    assert!(
        output.contains("(kanban:fix/flaky-test)"),
        "repo and branch context should render"
    );
    assert!(output.contains("[ Yes ]"), "yes button should render");
    assert!(output.contains("[ No ]"), "no button should render");
}

#[cfg(test)]
#[test]
fn focus_navigation() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::DeleteTask,
        Box::new(DeleteTaskDialog::new(None)),
    );

    let messages = send_key_to_component(
        &driver,
        &mut app,
        &[KeyCode::Tab, KeyCode::Right, KeyCode::Left],
        1,
    );
    assert_eq!(
        messages,
        vec![
            Msg::FocusButton("Yes".to_string()),
            Msg::FocusButton("No".to_string()),
            Msg::FocusButton("Yes".to_string())
        ]
    );
}

#[cfg(test)]
#[test]
fn confirm_emits_msg() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::DeleteTask,
        Box::new(DeleteTaskDialog::new(Some(DeleteTaskContext {
            title: "Cleanup".to_string(),
            repo_name: "kanban".to_string(),
            branch: "chore/cleanup".to_string(),
        }))),
    );

    let messages =
        send_key_to_component(&driver, &mut app, &[KeyCode::Char('y'), KeyCode::Enter], 1);
    assert_eq!(
        messages,
        vec![Msg::FocusButton("Yes".to_string()), Msg::ConfirmDeleteTask]
    );
}
