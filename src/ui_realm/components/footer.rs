use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::{Alignment, Rect};
use tuirealm::tui::style::{Color, Style};
use tuirealm::tui::text::Span;
use tuirealm::tui::widgets::{Block, Borders};
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State};

use crate::ui_realm::messages::Msg;

const DEFAULT_HINTS: &str = " n: new task  Enter: attach  Ctrl+P: command palette  c/r/x: category  H/L: move task left/right  J/K: reorder task  tmux Prefix+K: previous session ";

pub struct Footer {
    props: Props,
}

impl Footer {
    pub fn new() -> Self {
        Self {
            props: Props::default(),
        }
    }

    fn notice(&self) -> String {
        self.props
            .get(Attribute::Text)
            .map(|v| match v {
                AttrValue::String(s) if !s.is_empty() => s.clone(),
                _ => DEFAULT_HINTS.to_string(),
            })
            .unwrap_or_else(|| DEFAULT_HINTS.to_string())
    }

    pub fn set_notice(&mut self, notice: Option<String>) {
        if let Some(text) = notice {
            self.props.set(Attribute::Text, AttrValue::String(text));
        }
    }

    pub fn notice_opt(&self) -> Option<String> {
        self.props.get(Attribute::Text).and_then(|v| match v {
            AttrValue::String(s) => Some(s.clone()),
            _ => None,
        })
    }
}

impl Default for Footer {
    fn default() -> Self {
        Self::new()
    }
}

impl MockComponent for Footer {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let notice = self.notice();
        let block = Block::default()
            .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
            .border_style(Style::default().fg(Color::Blue))
            .title(Span::styled(
                format!(" {notice} "),
                Style::default().fg(Color::Blue),
            ))
            .title_alignment(Alignment::Center);
        frame.render_widget(block, area);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::None
    }
}

impl Component<Msg, NoUserEvent> for Footer {
    fn on(&mut self, _ev: Event<NoUserEvent>) -> Option<Msg> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FooterMsg {
    NoticeCleared,
}

#[cfg(test)]
use crate::ui_realm::ComponentId;
#[cfg(test)]
use crate::ui_realm::tests::harness::{EventDriver, MockTerminal};
#[cfg(test)]
use crate::ui_realm::tests::helpers::{mount_component_for_test, render_component};

#[cfg(test)]
#[test]
fn renders() {
    let driver = EventDriver::default();
    let component = Box::new(Footer::new());
    let mut app = mount_component_for_test(&driver, ComponentId::Footer, component);
    let mut terminal = MockTerminal::new(80, 10);
    let rendered = render_component(&mut app, ComponentId::Footer, &mut terminal);

    assert!(
        rendered.contains("n: new task"),
        "default hints should be rendered"
    );
    assert!(
        rendered.contains("Enter: attach"),
        "default hints should include attach hint"
    );
}

#[cfg(test)]
#[test]
fn updates_notice() {
    let driver = EventDriver::default();
    let mut component = Footer::new();
    component.set_notice(Some("Warning: tmux mouse mode disabled".to_string()));

    let mut app = mount_component_for_test(&driver, ComponentId::Footer, Box::new(component));
    let mut terminal = MockTerminal::new(60, 10);
    let rendered = render_component(&mut app, ComponentId::Footer, &mut terminal);

    assert!(
        rendered.contains("Warning: tmux mouse mode disabled"),
        "custom notice should be rendered"
    );
}

#[cfg(test)]
#[test]
fn empty_notice() {
    let driver = EventDriver::default();
    let mut component = Footer::new();
    component.set_notice(Some(String::new()));

    let mut app = mount_component_for_test(&driver, ComponentId::Footer, Box::new(component));
    let mut terminal = MockTerminal::new(80, 10);
    let rendered = render_component(&mut app, ComponentId::Footer, &mut terminal);

    assert!(
        rendered.contains("n: new task"),
        "empty notice should show default hints"
    );
}
