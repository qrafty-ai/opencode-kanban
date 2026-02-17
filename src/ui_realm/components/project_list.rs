use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::ui_realm::messages::Msg;

pub struct ProjectList {
    props: Props,
    projects: Vec<String>,
    selected: usize,
}

impl ProjectList {
    pub fn new(projects: Vec<String>) -> Self {
        Self {
            props: Props::default(),
            projects,
            selected: 0,
        }
    }

    fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn select_down(&mut self) {
        if self.selected + 1 < self.projects.len() {
            self.selected += 1;
        }
    }
}

impl MockComponent for ProjectList {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Projects ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 {
            return;
        }

        if self.projects.is_empty() {
            frame.render_widget(
                Paragraph::new("No projects available").alignment(Alignment::Center),
                inner,
            );
            return;
        }

        let lines = self
            .projects
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let is_selected = index == self.selected;
                let style = if is_selected {
                    Style::default().fg(Color::Yellow).bg(Color::DarkGray)
                } else {
                    Style::default()
                };
                let prefix = if is_selected { "> " } else { "  " };
                Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(name.as_str(), style),
                ])
            })
            .collect::<Vec<_>>();

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.selected as u16))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for ProjectList {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Up, .. })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('k'),
                ..
            }) => {
                self.select_up();
                Some(Msg::ProjectListSelectUp)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('j'),
                ..
            }) => {
                self.select_down();
                Some(Msg::ProjectListSelectDown)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => Some(Msg::SelectProject(self.selected)),
            Event::Keyboard(KeyEvent {
                code: Key::Char('n'),
                ..
            }) => Some(Msg::OpenNewProjectDialog),
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
        ComponentId::ProjectList,
        Box::new(ProjectList::new(vec![
            "alpha".to_string(),
            "beta".to_string(),
        ])),
    );

    let output = render_simple_component(&mut app, ComponentId::ProjectList);
    assert!(
        output.contains("alpha"),
        "render should contain first project"
    );
    assert!(
        output.contains("beta"),
        "render should contain second project"
    );
    assert!(
        output.contains("> alpha"),
        "selected row should be prefixed with marker"
    );
}

#[cfg(test)]
#[test]
fn navigation() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::ProjectList,
        Box::new(ProjectList::new(vec![
            "alpha".to_string(),
            "beta".to_string(),
        ])),
    );

    let down_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Down], 1);
    assert_eq!(down_messages, vec![Msg::ProjectListSelectDown]);
    let output = render_simple_component(&mut app, ComponentId::ProjectList);
    assert!(
        output.contains("> beta"),
        "down navigation should move selection to second project"
    );

    let up_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Up], 1);
    assert_eq!(up_messages, vec![Msg::ProjectListSelectUp]);
    let output = render_simple_component(&mut app, ComponentId::ProjectList);
    assert!(
        output.contains("> alpha"),
        "up navigation should move selection to first project"
    );
}

#[cfg(test)]
#[test]
fn selection() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::ProjectList,
        Box::new(ProjectList::new(vec![
            "alpha".to_string(),
            "beta".to_string(),
        ])),
    );

    let enter_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Enter], 1);
    assert_eq!(enter_messages, vec![Msg::SelectProject(0)]);

    let new_project_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Char('n')], 1);
    assert_eq!(new_project_messages, vec![Msg::OpenNewProjectDialog]);
}
