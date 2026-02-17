use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::ui_realm::messages::Msg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenuEntry {
    pub label: String,
    pub msg: Msg,
}

impl ContextMenuEntry {
    pub fn new(label: impl Into<String>, msg: Msg) -> Self {
        Self {
            label: label.into(),
            msg,
        }
    }
}

pub struct ContextMenu {
    props: Props,
    title: String,
    items: Vec<ContextMenuEntry>,
    selected_index: usize,
    scroll_offset: usize,
    viewport_rows: usize,
}

impl ContextMenu {
    pub fn new(title: impl Into<String>, items: Vec<ContextMenuEntry>) -> Self {
        let mut component = Self {
            props: Props::default(),
            title: title.into(),
            items,
            selected_index: 0,
            scroll_offset: 0,
            viewport_rows: 1,
        };
        component.clamp_selection();
        component.clamp_scroll_offset();
        component
    }

    fn set_viewport_rows(&mut self, rows: usize) {
        self.viewport_rows = rows.max(1);
        self.clamp_scroll_offset();
        self.ensure_selected_visible();
    }

    fn clamp_selection(&mut self) {
        if self.items.is_empty() {
            self.selected_index = 0;
            return;
        }
        self.selected_index = self.selected_index.min(self.items.len() - 1);
    }

    fn max_scroll_offset(&self) -> usize {
        self.items.len().saturating_sub(self.viewport_rows)
    }

    fn clamp_scroll_offset(&mut self) {
        self.scroll_offset = self.scroll_offset.min(self.max_scroll_offset());
    }

    fn ensure_selected_visible(&mut self) {
        if self.items.is_empty() {
            self.scroll_offset = 0;
            return;
        }

        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else {
            let visible_end = self.scroll_offset + self.viewport_rows;
            if self.selected_index >= visible_end {
                self.scroll_offset = self.selected_index + 1 - self.viewport_rows;
            }
        }
        self.clamp_scroll_offset();
    }

    fn select_next(&mut self) -> bool {
        if self.items.is_empty() {
            return false;
        }
        let next = (self.selected_index + 1).min(self.items.len() - 1);
        if next == self.selected_index {
            return false;
        }
        self.selected_index = next;
        self.ensure_selected_visible();
        true
    }

    fn select_previous(&mut self) -> bool {
        if self.items.is_empty() {
            return false;
        }
        if self.selected_index == 0 {
            return false;
        }
        self.selected_index -= 1;
        self.ensure_selected_visible();
        true
    }

    fn selected_msg(&self) -> Option<Msg> {
        self.items
            .get(self.selected_index)
            .map(|item| item.msg.clone())
    }
}

impl MockComponent for ContextMenu {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", self.title));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        self.set_viewport_rows(inner.height as usize);

        if self.items.is_empty() {
            frame.render_widget(
                Paragraph::new("No actions").alignment(Alignment::Center),
                inner,
            );
            return;
        }

        let lines = self
            .items
            .iter()
            .enumerate()
            .skip(self.scroll_offset)
            .take(self.viewport_rows)
            .map(|(index, item)| {
                let is_selected = index == self.selected_index;
                let style = if is_selected {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else {
                    Style::default()
                };
                let prefix = if is_selected { "> " } else { "  " };
                Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(item.label.as_str(), style),
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
        State::One(StateValue::U16(self.selected_index as u16))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for ContextMenu {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('j'),
                ..
            }) => {
                if self.select_next() {
                    Some(Msg::SelectDown)
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent { code: Key::Up, .. })
            | Event::Keyboard(KeyEvent {
                code: Key::Char('k'),
                ..
            }) => {
                if self.select_previous() {
                    Some(Msg::SelectUp)
                } else {
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => self.selected_msg(),
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
        ComponentId::ContextMenu,
        Box::new(ContextMenu::new("Actions", sample_items())),
    );

    let output = render_simple_component(&mut app, ComponentId::ContextMenu);
    assert!(output.contains("Actions"), "menu title should render");
    assert!(output.contains("Attach"), "first action should render");
    assert!(output.contains("Delete"), "second action should render");
    assert!(output.contains("Move"), "third action should render");
    assert!(
        output.contains("> Attach"),
        "first entry should be selected by default"
    );
}

#[cfg(test)]
#[test]
fn navigation() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::ContextMenu,
        Box::new(ContextMenu::new("Actions", sample_items())),
    );

    let down_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Down], 1);
    assert_eq!(down_messages, vec![Msg::SelectDown]);
    let output = render_simple_component(&mut app, ComponentId::ContextMenu);
    assert!(
        output.contains("> Delete"),
        "down navigation should move selection to second item"
    );

    let up_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Up], 1);
    assert_eq!(up_messages, vec![Msg::SelectUp]);
    let output = render_simple_component(&mut app, ComponentId::ContextMenu);
    assert!(
        output.contains("> Attach"),
        "up navigation should return selection to first item"
    );
}

#[cfg(test)]
#[test]
fn selection_emits_msg() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::ContextMenu,
        Box::new(ContextMenu::new("Actions", sample_items())),
    );

    let enter_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Enter], 1);
    assert_eq!(enter_messages, vec![Msg::AttachTask]);

    let empty_messages = send_key_to_component(
        &driver,
        &mut mount_component_for_test(
            &driver,
            ComponentId::ContextMenu,
            Box::new(ContextMenu::new("Actions", Vec::new())),
        ),
        &[KeyCode::Enter],
        1,
    );
    assert!(
        empty_messages.is_empty(),
        "empty menu enter should not panic or emit msg"
    );
}

#[cfg(test)]
fn sample_items() -> Vec<ContextMenuEntry> {
    vec![
        ContextMenuEntry::new("Attach", Msg::AttachTask),
        ContextMenuEntry::new("Delete", Msg::OpenDeleteTaskDialog),
        ContextMenuEntry::new("Move", Msg::MoveTaskRight),
    ]
}
