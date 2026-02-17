use std::collections::HashMap;

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, BorderType, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};
use uuid::Uuid;

use crate::types::{Category, Task};
use crate::ui_realm::messages::Msg;

pub struct KanbanColumn {
    props: Props,
    column_index: usize,
    category: Category,
    tasks: Vec<Task>,
    repo_name_by_id: HashMap<Uuid, String>,
    selected_index: usize,
    scroll_offset: usize,
    viewport_tasks: usize,
}

impl KanbanColumn {
    pub fn new(
        column_index: usize,
        category: Category,
        mut tasks: Vec<Task>,
        repo_name_by_id: HashMap<Uuid, String>,
    ) -> Self {
        tasks.sort_by_key(|task| task.position);
        let mut component = Self {
            props: Props::default(),
            column_index,
            category,
            tasks,
            repo_name_by_id,
            selected_index: 0,
            scroll_offset: 0,
            viewport_tasks: 1,
        };
        component.clamp_selection();
        component.clamp_scroll_offset();
        component
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    fn status_icon(status: &str) -> &'static str {
        match status {
            "running" => "●",
            "idle" => "○",
            "waiting" => "◐",
            "dead" => "✕",
            "repo_unavailable" => "!",
            "broken" => "!",
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

    fn set_viewport_tasks(&mut self, tasks: usize) {
        self.viewport_tasks = tasks.max(1);
        self.clamp_scroll_offset();
        self.ensure_selected_visible();
    }

    fn clamp_selection(&mut self) {
        if self.tasks.is_empty() {
            self.selected_index = 0;
            return;
        }
        self.selected_index = self.selected_index.min(self.tasks.len() - 1);
    }

    fn max_scroll_offset(&self) -> usize {
        self.tasks.len().saturating_sub(self.viewport_tasks)
    }

    fn clamp_scroll_offset(&mut self) {
        self.scroll_offset = self.scroll_offset.min(self.max_scroll_offset());
    }

    fn ensure_selected_visible(&mut self) {
        if self.tasks.is_empty() {
            self.scroll_offset = 0;
            return;
        }

        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else {
            let visible_end = self.scroll_offset + self.viewport_tasks;
            if self.selected_index >= visible_end {
                self.scroll_offset = self.selected_index + 1 - self.viewport_tasks;
            }
        }
        self.clamp_scroll_offset();
    }

    fn move_selection_by(&mut self, delta: isize) -> bool {
        if self.tasks.is_empty() {
            return false;
        }

        let current = self.selected_index as isize;
        let max_index = (self.tasks.len() - 1) as isize;
        let next = (current + delta).clamp(0, max_index) as usize;
        if next == self.selected_index {
            return false;
        }

        self.selected_index = next;
        self.ensure_selected_visible();
        true
    }

    fn page_down(&mut self) -> bool {
        self.move_selection_by(self.viewport_tasks as isize)
    }

    fn page_up(&mut self) -> bool {
        self.move_selection_by(-(self.viewport_tasks as isize))
    }

    fn emit_selection_msg(&self) -> Option<Msg> {
        if self.tasks.is_empty() {
            None
        } else {
            Some(Msg::SelectTask {
                column: self.column_index,
                task: self.selected_index,
            })
        }
    }

    fn is_focused(&self) -> bool {
        self.props
            .get(Attribute::Focus)
            .and_then(|value| match value {
                AttrValue::Flag(flag) => Some(flag),
                _ => None,
            })
            .unwrap_or(false)
    }

    fn repo_branch_label(&self, task: &Task) -> String {
        let repo = self
            .repo_name_by_id
            .get(&task.repo_id)
            .map(String::as_str)
            .unwrap_or("unknown");
        let branch = task.branch.trim();
        if branch.is_empty() {
            repo.to_string()
        } else {
            format!("{repo}:{branch}")
        }
    }

    fn task_primary_text(&self, task: &Task) -> String {
        let title = task.title.trim();
        if title.is_empty() {
            self.repo_branch_label(task)
        } else {
            title.to_string()
        }
    }
}

impl MockComponent for KanbanColumn {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let is_focused = self.is_focused();
        let border_type = if is_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        };
        let border_color = if is_focused { Color::Cyan } else { Color::Gray };
        let title = format!(" {} ({}) ", self.category.name, self.tasks.len());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(border_type)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(title, Style::default().fg(border_color)))
            .title_alignment(Alignment::Center);
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        self.set_viewport_tasks((inner.height as usize / 3).max(1));

        if self.tasks.is_empty() {
            frame.render_widget(
                Paragraph::new("No tasks in this category")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::Gray)),
                inner,
            );
            return;
        }

        let visible_tasks = self
            .tasks
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(self.viewport_tasks)
            .collect::<Vec<_>>();
        let mut lines = Vec::with_capacity(visible_tasks.len() * 3);
        for (line_index, (index, task)) in visible_tasks.into_iter().enumerate() {
            let is_selected = is_focused && index == self.selected_index;
            let prefix = if is_selected { "▸" } else { " " };
            let icon = Self::status_icon(task.tmux_status.as_str());
            let icon_color = Self::status_color(task.tmux_status.as_str());
            let title_color = if is_selected {
                Color::Yellow
            } else {
                Color::White
            };
            let detail = self.repo_branch_label(task);

            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::Yellow)),
                Span::styled(icon, Style::default().fg(icon_color)),
                Span::raw(" "),
                Span::styled(
                    self.task_primary_text(task),
                    Style::default().fg(title_color),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(detail, Style::default().fg(Color::Gray)),
            ]));

            if line_index + 1 < self.viewport_tasks && index + 1 < self.tasks.len() {
                lines.push(Line::raw(""));
            }
        }

        frame.render_widget(Paragraph::new(lines), inner);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.scroll_offset as u16))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for KanbanColumn {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Left, ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('h'),
                ..
            }) => Some(Msg::FocusColumn(self.column_index.saturating_sub(1))),
            Event::Keyboard(KeyEvent {
                code: Key::Right, ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('l'),
                ..
            }) => Some(Msg::FocusColumn(self.column_index.saturating_add(1))),
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => {
                if self.move_selection_by(1) {
                    self.emit_selection_msg()
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('j'),
                ..
            }) => {
                if self.move_selection_by(1) {
                    self.emit_selection_msg()
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent { code: Key::Up, .. }) => {
                if self.move_selection_by(-1) {
                    self.emit_selection_msg()
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('k'),
                ..
            }) => {
                if self.move_selection_by(-1) {
                    self.emit_selection_msg()
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::PageDown,
                ..
            }) => {
                if self.page_down() {
                    self.emit_selection_msg()
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::PageUp, ..
            }) => {
                if self.page_up() {
                    self.emit_selection_msg()
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => {
                if self.tasks.is_empty() {
                    None
                } else {
                    Some(Msg::AttachTask)
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('e'),
                modifiers,
            }) if modifiers.contains(tuirealm::event::KeyModifiers::CONTROL) => {
                if self.tasks.is_empty() {
                    None
                } else {
                    Some(Msg::AttachTask)
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('n'),
                ..
            }) => Some(Msg::OpenNewTaskDialog),
            Event::Keyboard(KeyEvent {
                code: Key::Char('d'),
                ..
            }) => Some(Msg::OpenDeleteTaskDialog),
            Event::Keyboard(KeyEvent {
                code: Key::Char('c'),
                ..
            }) => Some(Msg::OpenAddCategoryDialog),
            Event::Keyboard(KeyEvent {
                code: Key::Char('r'),
                ..
            }) => Some(Msg::OpenRenameCategoryDialog),
            Event::Keyboard(KeyEvent {
                code: Key::Char('x'),
                ..
            }) => Some(Msg::OpenDeleteCategoryDialog),
            Event::Keyboard(KeyEvent {
                code: Key::Char('H'),
                ..
            }) => Some(Msg::MoveTaskLeft),
            Event::Keyboard(KeyEvent {
                code: Key::Char('L'),
                ..
            }) => Some(Msg::MoveTaskRight),
            Event::Keyboard(KeyEvent {
                code: Key::Char('J'),
                ..
            }) => Some(Msg::MoveTaskDown),
            Event::Keyboard(KeyEvent {
                code: Key::Char('K'),
                ..
            }) => Some(Msg::MoveTaskUp),
            Event::Keyboard(KeyEvent {
                code: Key::Char('p'),
                modifiers,
            }) if modifiers.contains(tuirealm::event::KeyModifiers::CONTROL) => {
                Some(Msg::OpenCommandPalette)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('o'),
                modifiers,
            }) if modifiers.contains(tuirealm::event::KeyModifiers::CONTROL) => {
                Some(Msg::OpenProjectList)
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char('?'),
                ..
            }) => Some(Msg::ExecuteCommand("help".to_string())),
            _ => None,
        }
    }
}

#[cfg(test)]
use crossterm::event::KeyCode;

#[cfg(test)]
use crate::ui_realm::ComponentId;
#[cfg(test)]
use crate::ui_realm::tests::harness::{EventDriver, MockTerminal};
#[cfg(test)]
use crate::ui_realm::tests::helpers::{
    mount_component_for_test, render_component, send_key_to_component,
};

#[cfg(test)]
#[test]
fn renders_header() {
    let driver = EventDriver::default();
    let category = sample_category("TODO");
    let tasks = vec![sample_task("Task A", "idle", 0)];
    let component = Box::new(KanbanColumn::new(0, category, tasks, HashMap::new()));

    let mut app = mount_component_for_test(&driver, ComponentId::KanbanColumn(0), component);
    let mut terminal = MockTerminal::new(40, 10);
    let rendered = render_component(&mut app, ComponentId::KanbanColumn(0), &mut terminal);

    assert!(
        rendered.contains("TODO"),
        "category title should render in column header"
    );
}

#[cfg(test)]
#[test]
fn renders_tasks_with_icons() {
    let driver = EventDriver::default();
    let category = sample_category("Status");
    let tasks = vec![
        sample_task("Running", "running", 0),
        sample_task("Idle", "idle", 1),
        sample_task("Waiting", "waiting", 2),
        sample_task("Dead", "dead", 3),
        sample_task("Repo Missing", "repo_unavailable", 4),
        sample_task("Broken", "broken", 5),
        sample_task("Unknown", "mystery", 6),
    ];
    let component = Box::new(KanbanColumn::new(0, category, tasks, HashMap::new()));

    let mut app = mount_component_for_test(&driver, ComponentId::KanbanColumn(0), component);
    let mut terminal = MockTerminal::new(60, 24);
    let rendered = render_component(&mut app, ComponentId::KanbanColumn(0), &mut terminal);

    assert!(rendered.contains("●"), "running icon should be rendered");
    assert!(rendered.contains("○"), "idle icon should be rendered");
    assert!(rendered.contains("◐"), "waiting icon should be rendered");
    assert!(rendered.contains("✕"), "dead icon should be rendered");
    assert!(
        rendered.contains("!"),
        "repo/broken icon should be rendered"
    );
    assert!(
        rendered.contains("?"),
        "unknown status icon should be rendered"
    );
}

#[cfg(test)]
#[test]
fn scrolling() {
    let driver = EventDriver::default();
    let category = sample_category("TODO");
    let tasks = (0..20)
        .map(|index| sample_task(&format!("task-{index:02}"), "idle", index as i64))
        .collect::<Vec<_>>();
    let component = Box::new(KanbanColumn::new(0, category, tasks, HashMap::new()));

    let mut app = mount_component_for_test(&driver, ComponentId::KanbanColumn(0), component);
    let mut terminal = MockTerminal::new(36, 8);
    let before = render_component(&mut app, ComponentId::KanbanColumn(0), &mut terminal);
    assert!(
        before.contains("task-00"),
        "first page should include task-00"
    );

    let messages = send_key_to_component(&driver, &mut app, &[KeyCode::PageDown], 1);
    assert_eq!(
        messages,
        vec![Msg::SelectTask { column: 0, task: 2 }],
        "paged navigation should emit semantic selection msg"
    );

    let after = render_component(&mut app, ComponentId::KanbanColumn(0), &mut terminal);
    assert!(
        after.contains("task-02"),
        "page down should move visible window"
    );
    assert!(
        !after.contains("task-00"),
        "scrolling should advance past initial top task"
    );
}

#[cfg(test)]
#[test]
fn selection_emits_msg() {
    let driver = EventDriver::default();
    let category = sample_category("TODO");
    let tasks = vec![
        sample_task("task-0", "idle", 0),
        sample_task("task-1", "running", 1),
        sample_task("task-2", "waiting", 2),
    ];
    let component = Box::new(KanbanColumn::new(2, category, tasks, HashMap::new()));

    let mut app = mount_component_for_test(&driver, ComponentId::KanbanColumn(2), component);
    let messages = send_key_to_component(&driver, &mut app, &[KeyCode::Down], 1);

    assert_eq!(messages, vec![Msg::SelectTask { column: 2, task: 1 }]);
}

#[cfg(test)]
fn sample_category(name: &str) -> Category {
    Category {
        id: Uuid::new_v4(),
        name: name.to_string(),
        position: 0,
        color: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

#[cfg(test)]
fn sample_task(title: &str, status: &str, position: i64) -> Task {
    Task {
        id: Uuid::new_v4(),
        title: title.to_string(),
        repo_id: Uuid::new_v4(),
        branch: "main".to_string(),
        category_id: Uuid::new_v4(),
        position,
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
