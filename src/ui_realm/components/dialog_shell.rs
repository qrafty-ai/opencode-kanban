use tuirealm::command::{Cmd, CmdResult};
use tuirealm::event::{Key, KeyEvent, KeyModifiers};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::Line;
use tuirealm::tui::widgets::{Block, Borders, Clear, Paragraph};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State, StateValue};

use crate::ui_realm::messages::Msg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogButton {
    pub id: String,
    pub label: String,
}

impl DialogButton {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }
}

pub struct DialogShell {
    props: Props,
    title: String,
    content_lines: Vec<String>,
    buttons: Vec<DialogButton>,
    focused_button: usize,
}

impl DialogShell {
    pub fn new(
        title: impl Into<String>,
        content_lines: Vec<String>,
        buttons: Vec<DialogButton>,
    ) -> Self {
        Self {
            props: Props::default(),
            title: title.into(),
            content_lines,
            buttons,
            focused_button: 0,
        }
    }

    pub fn focused_button_label(&self) -> Option<&str> {
        self.buttons
            .get(self.focused_button)
            .map(|button| button.label.as_str())
    }

    fn focus_next_button(&mut self) -> Option<String> {
        if self.buttons.is_empty() {
            return None;
        }
        self.focused_button = (self.focused_button + 1) % self.buttons.len();
        self.buttons
            .get(self.focused_button)
            .map(|button| button.label.clone())
    }

    fn focus_previous_button(&mut self) -> Option<String> {
        if self.buttons.is_empty() {
            return None;
        }
        self.focused_button = if self.focused_button == 0 {
            self.buttons.len() - 1
        } else {
            self.focused_button - 1
        };
        self.buttons
            .get(self.focused_button)
            .map(|button| button.label.clone())
    }
}

impl MockComponent for DialogShell {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        frame.render_widget(Clear, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", self.title));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 {
            return;
        }

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(3)])
            .split(inner);

        let content = self
            .content_lines
            .iter()
            .map(|line| Line::from(line.as_str()))
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(content).alignment(Alignment::Center),
            rows[0],
        );

        if self.buttons.is_empty() || rows[1].width == 0 {
            return;
        }

        let constraints = vec![Constraint::Ratio(1, self.buttons.len() as u32); self.buttons.len()];
        let button_areas = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(rows[1]);

        for (index, button) in self.buttons.iter().enumerate() {
            let is_focused = index == self.focused_button;
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
                Paragraph::new(format!("[ {} ]", button.label))
                    .alignment(Alignment::Center)
                    .style(style)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_style(Style::default().fg(border)),
                    ),
                button_areas[index],
            );
        }
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::One(StateValue::U16(self.focused_button as u16))
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for DialogShell {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match ev {
            Event::Keyboard(KeyEvent {
                code: Key::Tab,
                modifiers: KeyModifiers::NONE,
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Right,
                modifiers: KeyModifiers::NONE,
            }) => self.focus_next_button().map(Msg::FocusButton),
            Event::Keyboard(KeyEvent {
                code: Key::BackTab,
                modifiers: KeyModifiers::SHIFT,
            })
            | Event::Keyboard(KeyEvent {
                code: Key::Left,
                modifiers: KeyModifiers::NONE,
            }) => self.focus_previous_button().map(Msg::FocusButton),
            Event::Keyboard(KeyEvent {
                code: Key::Enter, ..
            }) => Some(Msg::SubmitDialog),
            Event::Keyboard(KeyEvent { code: Key::Esc, .. }) => Some(Msg::CancelAction),
            _ => None,
        }
    }
}

#[cfg(test)]
mod dialog_shell {
    use crossterm::event::KeyCode;
    use tuirealm::{Application, NoUserEvent, PollStrategy};

    use super::{DialogButton, DialogShell};
    use crate::ui_realm::ComponentId;
    use crate::ui_realm::messages::Msg;
    use crate::ui_realm::tests::harness::{
        EventDriver, MockTerminal, assert_buffer_contains, send_keys,
    };

    #[test]
    fn renders_with_title() {
        let driver = EventDriver::default();
        let mut app: Application<ComponentId, Msg, NoUserEvent> =
            Application::init(driver.listener_cfg());

        let shell = DialogShell::new(
            "Test Dialog",
            vec!["Dialog body content".to_string()],
            vec![
                DialogButton::new("ok", "OK"),
                DialogButton::new("cancel", "Cancel"),
            ],
        );

        app.mount(ComponentId::Help, Box::new(shell), vec![])
            .expect("dialog shell should mount");
        app.active(&ComponentId::Help)
            .expect("dialog shell should become active");

        let mut terminal = MockTerminal::new(60, 12);
        terminal.draw(|frame| {
            app.view(&ComponentId::Help, frame, frame.size());
        });

        assert_buffer_contains(&terminal, "Test Dialog");
        assert_buffer_contains(&terminal, "Dialog body content");
        assert_buffer_contains(&terminal, "OK");
        assert_buffer_contains(&terminal, "Cancel");
    }

    #[test]
    fn focus_cycles_buttons() {
        let driver = EventDriver::default();
        let mut app: Application<ComponentId, Msg, NoUserEvent> =
            Application::init(driver.listener_cfg());

        let shell = DialogShell::new(
            "Focus Dialog",
            vec!["content".to_string()],
            vec![
                DialogButton::new("ok", "OK"),
                DialogButton::new("cancel", "Cancel"),
            ],
        );

        app.mount(ComponentId::Help, Box::new(shell), vec![])
            .expect("dialog shell should mount");
        app.active(&ComponentId::Help)
            .expect("dialog shell should become active");

        send_keys(&driver, &[KeyCode::Tab, KeyCode::Tab, KeyCode::Tab]);
        let messages = app
            .tick(PollStrategy::UpTo(8))
            .expect("dialog shell tick should succeed");

        assert_eq!(
            messages,
            vec![
                Msg::FocusButton("Cancel".to_string()),
                Msg::FocusButton("OK".to_string()),
                Msg::FocusButton("Cancel".to_string()),
            ]
        );
    }
}
