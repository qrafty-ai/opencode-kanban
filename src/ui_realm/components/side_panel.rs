use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::types::{Category, Repo, Task};
use crate::ui_realm::messages::Msg;

pub struct SidePanel {
    props: Props,
    categories: Vec<Category>,
    tasks: Vec<Task>,
    repos: Vec<Repo>,
    current_log_buffer: Option<String>,
    selected_index: usize,
}

#[derive(Clone, Copy)]
struct SidePanelEntry<'a> {
    category: &'a Category,
    task: &'a Task,
}

impl SidePanel {
    pub fn new(
        categories: Vec<Category>,
        tasks: Vec<Task>,
        repos: Vec<Repo>,
        current_log_buffer: Option<String>,
    ) -> Self {
        Self {
            props: Props::default(),
            categories,
            tasks,
            repos,
            current_log_buffer,
            selected_index: 0,
        }
    }

    fn entries(&self) -> Vec<SidePanelEntry<'_>> {
        let mut categories = self.categories.iter().collect::<Vec<_>>();
        categories.sort_by(|left, right| {
            left.position
                .cmp(&right.position)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut entries = Vec::new();
        for category in categories {
            let mut tasks = self
                .tasks
                .iter()
                .filter(|task| task.category_id == category.id)
                .collect::<Vec<_>>();
            tasks.sort_by(|left, right| {
                left.position
                    .cmp(&right.position)
                    .then_with(|| left.title.cmp(&right.title))
                    .then_with(|| left.id.cmp(&right.id))
            });

            entries.extend(
                tasks
                    .into_iter()
                    .map(|task| SidePanelEntry { category, task }),
            );
        }

        entries
    }

    fn clamp_selection(&mut self, len: usize) {
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }

    fn move_selection_by(&mut self, delta: isize) -> bool {
        let entries_len = self.entries().len();
        if entries_len == 0 {
            self.selected_index = 0;
            return false;
        }

        let current = self.selected_index as isize;
        let max_index = (entries_len - 1) as isize;
        let next = (current + delta).clamp(0, max_index) as usize;
        if next == self.selected_index {
            return false;
        }

        self.selected_index = next;
        true
    }

    fn select_home(&mut self) -> bool {
        if self.entries().is_empty() || self.selected_index == 0 {
            return false;
        }
        self.selected_index = 0;
        true
    }

    fn select_end(&mut self) -> bool {
        let entries_len = self.entries().len();
        if entries_len == 0 {
            self.selected_index = 0;
            return false;
        }

        let last_index = entries_len - 1;
        if self.selected_index == last_index {
            return false;
        }

        self.selected_index = last_index;
        true
    }

    fn selected_entry<'a>(&self, entries: &'a [SidePanelEntry<'a>]) -> Option<SidePanelEntry<'a>> {
        entries.get(self.selected_index).copied()
    }

    fn repo_name(&self, repo_id: uuid::Uuid) -> &str {
        self.repos
            .iter()
            .find(|repo| repo.id == repo_id)
            .map(|repo| repo.name.as_str())
            .unwrap_or("unknown")
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
}

impl MockComponent for SidePanel {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Side Panel ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        let entries_len = self.entries().len();
        self.clamp_selection(entries_len);
        let entries = self.entries();

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
            .split(inner);

        let list_block = Block::default().borders(Borders::ALL).title(" Tasks ");
        let list_inner = list_block.inner(chunks[0]);
        frame.render_widget(list_block, chunks[0]);

        if list_inner.height > 0 && list_inner.width > 0 {
            if entries.is_empty() {
                frame.render_widget(
                    Paragraph::new("No tasks available").alignment(Alignment::Center),
                    list_inner,
                );
            } else {
                let visible = entries
                    .iter()
                    .enumerate()
                    .take(list_inner.height as usize)
                    .map(|(index, entry)| {
                        let is_selected = index == self.selected_index;
                        let row_style = if is_selected {
                            Style::default().bg(Color::DarkGray)
                        } else {
                            Style::default()
                        };
                        let marker = if is_selected { "▸" } else { " " };
                        let icon = Self::status_icon(entry.task.tmux_status.as_str());
                        let icon_color = Self::status_color(entry.task.tmux_status.as_str());

                        Line::from(vec![
                            Span::styled(marker, Style::default().fg(Color::Yellow)),
                            Span::styled(icon, Style::default().fg(icon_color)),
                            Span::raw(" "),
                            Span::styled(entry.task.title.as_str(), row_style),
                        ])
                    })
                    .collect::<Vec<_>>();

                frame.render_widget(Paragraph::new(visible), list_inner);
            }
        }

        let details_block = Block::default().borders(Borders::ALL).title(" Details ");
        let details_inner = details_block.inner(chunks[1]);
        frame.render_widget(details_block, chunks[1]);

        if details_inner.height == 0 || details_inner.width == 0 {
            return;
        }

        let details = if let Some(entry) = self.selected_entry(entries.as_slice()) {
            let mut lines = vec![
                Line::from(vec![Span::styled(
                    "Task Details",
                    Style::default().fg(Color::Yellow),
                )]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Title: ", Style::default().fg(Color::Gray)),
                    Span::raw(entry.task.title.as_str()),
                ]),
                Line::from(vec![
                    Span::styled("Category: ", Style::default().fg(Color::Gray)),
                    Span::raw(entry.category.name.as_str()),
                ]),
                Line::from(vec![
                    Span::styled("Repo: ", Style::default().fg(Color::Gray)),
                    Span::raw(self.repo_name(entry.task.repo_id)),
                ]),
                Line::from(vec![
                    Span::styled("Branch: ", Style::default().fg(Color::Gray)),
                    Span::raw(entry.task.branch.as_str()),
                ]),
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(Color::Gray)),
                    Span::raw(entry.task.tmux_status.as_str()),
                ]),
                Line::from(vec![
                    Span::styled("Source: ", Style::default().fg(Color::Gray)),
                    Span::raw(entry.task.status_source.as_str()),
                ]),
            ];

            if let Some(fetched_at) = &entry.task.status_fetched_at {
                lines.push(Line::from(vec![
                    Span::styled("Fetched: ", Style::default().fg(Color::Gray)),
                    Span::raw(fetched_at.as_str()),
                ]));
            }

            if let Some(status_error) = &entry.task.status_error {
                lines.push(Line::from(vec![
                    Span::styled("Status Error: ", Style::default().fg(Color::Gray)),
                    Span::raw(status_error.as_str()),
                ]));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "Logs",
                Style::default().fg(Color::Yellow),
            )]));

            match self.current_log_buffer.as_deref() {
                Some(logs) if !logs.trim().is_empty() => {
                    for line in logs.lines().take(20) {
                        lines.push(Line::from(line));
                    }
                }
                _ if entry.task.tmux_status == "running" => {
                    lines.push(Line::from("No logs yet."));
                }
                _ => {
                    lines.push(Line::from("Task is not running."));
                }
            }

            Paragraph::new(lines)
        } else {
            Paragraph::new("No task selected")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::Gray))
        };

        frame.render_widget(details, details_inner);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.selected_index as u16))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for SidePanel {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('j'),
                ..
            }) => {
                if self.move_selection_by(1) {
                    Some(Msg::SelectTaskInSidePanel(self.selected_index))
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent { code: Key::Up, .. })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('k'),
                ..
            }) => {
                if self.move_selection_by(-1) {
                    Some(Msg::SelectTaskInSidePanel(self.selected_index))
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Home, ..
            }) => {
                if self.select_home() {
                    Some(Msg::SelectTaskInSidePanel(self.selected_index))
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent { code: Key::End, .. }) => {
                if self.select_end() {
                    Some(Msg::SelectTaskInSidePanel(self.selected_index))
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => {
                if self.entries().is_empty() {
                    None
                } else {
                    Some(Msg::AttachTask)
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod side_panel {
    use crossterm::event::KeyCode;
    use uuid::Uuid;

    use super::SidePanel;
    use crate::types::{Category, Repo, Task};
    use crate::ui_realm::ComponentId;
    use crate::ui_realm::messages::Msg;
    use crate::ui_realm::tests::harness::{EventDriver, MockTerminal};
    use crate::ui_realm::tests::helpers::{
        mount_component_for_test, render_component, send_key_to_component,
    };

    #[test]
    fn renders_details() {
        let driver = EventDriver::default();
        let category = sample_category("TODO", 0);
        let repo = sample_repo("kanban");
        let task = sample_task("Implement side panel", "running", 0, category.id, repo.id);

        let component = Box::new(SidePanel::new(
            vec![category],
            vec![task],
            vec![repo],
            Some("log line 1\nlog line 2".to_string()),
        ));

        let mut app = mount_component_for_test(&driver, ComponentId::SidePanel, component);
        let mut terminal = MockTerminal::new(100, 20);
        let rendered = render_component(&mut app, ComponentId::SidePanel, &mut terminal);

        assert!(
            rendered.contains("Task Details"),
            "details heading should render"
        );
        assert!(
            rendered.contains("Implement side panel"),
            "task title should render"
        );
        assert!(
            rendered.contains("Status: running"),
            "status metadata should render"
        );
        assert!(
            rendered.contains("Logs"),
            "logs section heading should render"
        );
        assert!(
            rendered.contains("log line 1"),
            "task logs should render in details"
        );
    }

    #[test]
    fn empty_state() {
        let driver = EventDriver::default();
        let component = Box::new(SidePanel::new(vec![], vec![], vec![], None));

        let mut app = mount_component_for_test(&driver, ComponentId::SidePanel, component);
        let mut terminal = MockTerminal::new(80, 14);
        let rendered = render_component(&mut app, ComponentId::SidePanel, &mut terminal);

        assert!(
            rendered.contains("No tasks available"),
            "list pane should show empty-state text"
        );
        assert!(
            rendered.contains("No task selected"),
            "details pane should show empty-state text"
        );
    }

    #[test]
    fn navigation_emits_msg() {
        let driver = EventDriver::default();
        let category = sample_category("TODO", 0);
        let repo = sample_repo("workspace");
        let tasks = vec![
            sample_task("task-1", "idle", 0, category.id, repo.id),
            sample_task("task-2", "running", 1, category.id, repo.id),
        ];

        let component = Box::new(SidePanel::new(vec![category], tasks, vec![repo], None));
        let mut app = mount_component_for_test(&driver, ComponentId::SidePanel, component);

        let messages = send_key_to_component(&driver, &mut app, &[KeyCode::Down], 1);
        assert_eq!(messages, vec![Msg::SelectTaskInSidePanel(1)]);
    }

    fn sample_category(name: &str, position: i64) -> Category {
        Category {
            id: Uuid::new_v4(),
            name: name.to_string(),
            position,
            color: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn sample_repo(name: &str) -> Repo {
        Repo {
            id: Uuid::new_v4(),
            path: "/tmp/repo".to_string(),
            name: name.to_string(),
            default_base: Some("main".to_string()),
            remote_url: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn sample_task(
        title: &str,
        status: &str,
        position: i64,
        category_id: Uuid,
        repo_id: Uuid,
    ) -> Task {
        Task {
            id: Uuid::new_v4(),
            title: title.to_string(),
            repo_id,
            branch: "main".to_string(),
            category_id,
            position,
            tmux_session_name: None,
            worktree_path: None,
            tmux_status: status.to_string(),
            status_source: "server".to_string(),
            status_fetched_at: Some("2026-01-01T00:00:00Z".to_string()),
            status_error: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }
}
