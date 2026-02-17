use std::collections::HashMap;

use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tuirealm::tui::style::{Color, Modifier, Style};
use tuirealm::tui::text::{Line, Span};
use tuirealm::tui::widgets::{Block, Borders, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::command_palette::{CommandPaletteState, all_commands};
use crate::types::CommandFrequency;
use crate::ui_realm::messages::Msg;

pub struct CommandPalette {
    props: Props,
    state: CommandPaletteState,
}

impl CommandPalette {
    pub fn new(frequencies: HashMap<String, CommandFrequency>) -> Self {
        Self {
            props: Props::default(),
            state: CommandPaletteState::new(frequencies),
        }
    }

    fn select_relative(&mut self, delta: isize) -> Option<Msg> {
        if self.state.filtered.is_empty() {
            return None;
        }
        self.state.move_selection(delta);
        Some(Msg::SelectCommandPaletteItem(self.state.selected_index))
    }

    fn apply_query_char(&mut self, ch: char) {
        self.state.query.push(ch);
        self.state.update_query();
    }

    fn apply_backspace(&mut self) {
        self.state.query.pop();
        self.state.update_query();
    }

    fn render_command_name(&self, name: &str, matched_indices: &[usize]) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        for (index, ch) in name.chars().enumerate() {
            let style = if matched_indices.contains(&index) {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            spans.push(Span::styled(ch.to_string(), style));
        }
        spans
    }
}

impl MockComponent for CommandPalette {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let search = Paragraph::new(self.state.query.as_str()).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search ")
                .border_style(Style::default().fg(Color::Yellow)),
        );
        frame.render_widget(search, layout[0]);

        if layout[1].height == 0 {
            return;
        }

        if self.state.filtered.is_empty() {
            frame.render_widget(
                Paragraph::new("No matching commands")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray)),
                layout[1],
            );
            return;
        }

        let all_cmds = all_commands();
        let list_height = layout[1].height as usize;
        let total_items = self.state.filtered.len();
        let scroll_offset = if total_items <= list_height {
            0
        } else {
            let half_height = list_height / 2;
            if self.state.selected_index > half_height {
                (self.state.selected_index - half_height).min(total_items - list_height)
            } else {
                0
            }
        };

        for (row_index, ranked) in self
            .state
            .filtered
            .iter()
            .skip(scroll_offset)
            .take(list_height)
            .enumerate()
        {
            let command = &all_cmds[ranked.command_idx];
            let absolute_index = scroll_offset + row_index;
            let is_selected = absolute_index == self.state.selected_index;

            let mut row_spans = Vec::new();
            row_spans.push(Span::styled(
                if is_selected { "▸ " } else { "  " },
                Style::default().fg(Color::Yellow),
            ));
            row_spans
                .extend(self.render_command_name(command.display_name, &ranked.matched_indices));

            let row_area = Rect {
                x: layout[1].x,
                y: layout[1].y + row_index as u16,
                width: layout[1].width,
                height: 1,
            };

            frame.render_widget(
                Paragraph::new(Line::from(row_spans)).style(if is_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                }),
                row_area,
            );

            if !command.keybinding.is_empty() {
                frame.render_widget(
                    Paragraph::new(command.keybinding)
                        .alignment(Alignment::Right)
                        .style(Style::default().fg(Color::DarkGray)),
                    row_area,
                );
            }
        }
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.state.selected_index as u16))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for CommandPalette {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::DismissDialog),
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => self.state.selected_command_id().map(Msg::ExecuteCommand),
            Event::Keyboard(KeyEvent { code: Key::Up, .. }) => self.select_relative(-1),
            Event::Keyboard(KeyEvent {
                code: Key::Down, ..
            }) => self.select_relative(1),
            Event::Keyboard(KeyEvent {
                code: Key::Char('k'),
                modifiers,
            }) if modifiers.contains(KeyModifiers::CONTROL) => self.select_relative(-1),
            Event::Keyboard(KeyEvent {
                code: Key::Char('j'),
                modifiers,
            }) if modifiers.contains(KeyModifiers::CONTROL) => self.select_relative(1),
            Event::Keyboard(KeyEvent {
                code: Key::Backspace,
                ..
            }) => {
                if self.state.query.is_empty() {
                    Some(Msg::DismissDialog)
                } else {
                    self.apply_backspace();
                    None
                }
            }
            Event::Keyboard(KeyEvent {
                code: Key::Char(ch),
                modifiers,
            }) if !modifiers.contains(KeyModifiers::CONTROL)
                && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.apply_query_char(ch);
                None
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod command_palette_component {
    use std::collections::HashMap;

    use crossterm::event::KeyCode;

    use super::CommandPalette;
    use crate::ui_realm::ComponentId;
    use crate::ui_realm::messages::Msg;
    use crate::ui_realm::tests::harness::{EventDriver, MockTerminal};
    use crate::ui_realm::tests::helpers::{
        mount_component_for_test, render_component, send_key_to_component,
    };

    #[test]
    fn renders() {
        let driver = EventDriver::default();
        let mut app = mount_component_for_test(
            &driver,
            ComponentId::CommandPalette,
            Box::new(CommandPalette::new(HashMap::new())),
        );

        let mut terminal = MockTerminal::new(60, 12);
        let output = render_component(&mut app, ComponentId::CommandPalette, &mut terminal);

        assert!(output.contains("Search"), "search input should render");
        assert!(
            output.contains("Switch Project"),
            "default command list should render"
        );
    }

    #[test]
    fn filters() {
        let driver = EventDriver::default();
        let mut app = mount_component_for_test(
            &driver,
            ComponentId::CommandPalette,
            Box::new(CommandPalette::new(HashMap::new())),
        );

        let messages = send_key_to_component(
            &driver,
            &mut app,
            &[KeyCode::Char('z'), KeyCode::Char('z')],
            1,
        );
        assert!(messages.is_empty(), "typing query should not emit messages");

        let mut terminal = MockTerminal::new(60, 12);
        let output = render_component(&mut app, ComponentId::CommandPalette, &mut terminal);

        assert!(
            output.contains("No matching commands"),
            "unmatched query should render empty-state label"
        );
    }

    #[test]
    fn selection_emits_msg() {
        let driver = EventDriver::default();
        let mut app = mount_component_for_test(
            &driver,
            ComponentId::CommandPalette,
            Box::new(CommandPalette::new(HashMap::new())),
        );

        let move_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Down], 1);
        assert_eq!(move_messages, vec![Msg::SelectCommandPaletteItem(1)]);

        let execute_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Enter], 1);
        assert_eq!(
            execute_messages,
            vec![Msg::ExecuteCommand("new_task".to_string())]
        );
    }
}
