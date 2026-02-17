use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::types::{Repo, Task};
use crate::ui_realm::messages::Msg;

pub struct TaskCard {
    props: Props,
    card_index: usize,
    task: Option<Task>,
    repo: Option<Repo>,
    selected: bool,
}

impl TaskCard {
    pub fn new(card_index: usize, task: Option<Task>, repo: Option<Repo>, selected: bool) -> Self {
        Self {
            props: Props::default(),
            card_index,
            task,
            repo,
            selected,
        }
    }

    fn status_icon(status: &str) -> &'static str {
        match status {
            "running" => "●",
            "idle" => "○",
            "waiting" => "◐",
            "dead" => "✕",
            "repo_unavailable" | "broken" => "!",
            _ => "?",
        }
    }

    fn status_color(status: &str) -> Color {
        match status {
            "running" => Color::Yellow,
            "idle" => Color::White,
            "waiting" | "dead" | "repo_unavailable" | "broken" => Color::Gray,
            _ => Color::DarkGray,
        }
    }

    fn title_text(task: Option<&Task>) -> &str {
        match task {
            Some(task) if !task.title.trim().is_empty() => task.title.as_str(),
            _ => "(untitled task)",
        }
    }

    fn repo_context(&self) -> String {
        let repo_name = self
            .repo
            .as_ref()
            .map(|repo| repo.name.trim())
            .filter(|name| !name.is_empty())
            .unwrap_or("unknown");
        let branch = self
            .task
            .as_ref()
            .map(|task| task.branch.trim())
            .filter(|branch| !branch.is_empty())
            .unwrap_or("unknown");
        format!("{repo_name}/{branch}")
    }
}

impl MockComponent for TaskCard {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let border_style = if self.selected {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let Some(task) = self.task.as_ref() else {
            frame.render_widget(
                Paragraph::new("No task data").alignment(Alignment::Center),
                inner,
            );
            return;
        };

        let icon = Self::status_icon(task.tmux_status.as_str());
        let icon_color = Self::status_color(task.tmux_status.as_str());
        let title_color = if self.selected {
            Color::Yellow
        } else {
            Color::White
        };

        let line1 = Line::from(vec![
            Span::styled(icon, Style::default().fg(icon_color)),
            Span::raw(" "),
            Span::styled(
                Self::title_text(self.task.as_ref()),
                Style::default().fg(title_color),
            ),
        ]);
        let line2 = Line::from(vec![
            Span::raw("  "),
            Span::styled(self.repo_context(), Style::default().fg(Color::Gray)),
        ]);

        if inner.height == 1 {
            frame.render_widget(Paragraph::new(line1), inner);
            return;
        }

        frame.render_widget(Paragraph::new(vec![line1, line2]), inner);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.card_index as u16))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for TaskCard {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char(' '),
                ..
            }) => Some(Msg::SelectTaskInSidePanel(self.card_index)),
            Event::Keyboard(KeyEvent {
                code: Key::Char('a'),
                ..
            }) => Some(Msg::AttachTask),
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
use uuid::Uuid;

#[cfg(test)]
#[test]
fn renders() {
    let driver = EventDriver::default();
    let component = Box::new(TaskCard::new(
        0,
        Some(sample_task("Ship task card", "running")),
        Some(sample_repo("kanban")),
        true,
    ));
    let mut app = mount_component_for_test(&driver, ComponentId::TaskCard(0), component);

    let output = render_simple_component(&mut app, ComponentId::TaskCard(0));
    assert!(
        output.contains("Ship task card"),
        "card title should render"
    );
    assert!(output.contains("●"), "status icon should render");
}

#[cfg(test)]
#[test]
fn shows_repo_info() {
    let driver = EventDriver::default();
    let component = Box::new(TaskCard::new(
        1,
        Some(sample_task("Repo details", "idle")),
        Some(sample_repo("alpha-repo")),
        false,
    ));
    let mut app = mount_component_for_test(&driver, ComponentId::TaskCard(1), component);

    let output = render_simple_component(&mut app, ComponentId::TaskCard(1));
    assert!(
        output.contains("alpha-repo/feature/task-card"),
        "repo context should render as repo/branch"
    );
}

#[cfg(test)]
#[test]
fn selection_emits_msg() {
    let driver = EventDriver::default();
    let component = Box::new(TaskCard::new(
        4,
        Some(sample_task("Selectable", "waiting")),
        None,
        false,
    ));
    let mut app = mount_component_for_test(&driver, ComponentId::TaskCard(4), component);

    let select_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Enter], 1);
    assert_eq!(select_messages, vec![Msg::SelectTaskInSidePanel(4)]);

    let attach_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Char('a')], 1);
    assert_eq!(attach_messages, vec![Msg::AttachTask]);
}

#[cfg(test)]
fn sample_task(title: &str, status: &str) -> Task {
    Task {
        id: Uuid::new_v4(),
        title: title.to_string(),
        repo_id: Uuid::new_v4(),
        branch: "feature/task-card".to_string(),
        category_id: Uuid::new_v4(),
        position: 0,
        tmux_session_name: None,
        worktree_path: None,
        tmux_status: status.to_string(),
        status_source: "server".to_string(),
        status_fetched_at: None,
        status_error: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

#[cfg(test)]
fn sample_repo(name: &str) -> Repo {
    Repo {
        id: Uuid::new_v4(),
        path: "/tmp/repo".to_string(),
        name: name.to_string(),
        default_base: Some("main".to_string()),
        remote_url: Some("git@example.com:repo.git".to_string()),
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}
