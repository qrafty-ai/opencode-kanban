use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::Rect;
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State};

use crate::ui_realm::components::dialog_shell::{DialogButton, DialogShell};
use crate::ui_realm::messages::Msg;

pub struct HelpDialog {
    props: Props,
    shell: DialogShell,
}

impl HelpDialog {
    pub fn new() -> Self {
        Self {
            props: Props::default(),
            shell: DialogShell::new(
                "Help",
                Self::content_lines(),
                vec![DialogButton::new("close", "Close")],
            ),
        }
    }

    fn content_lines() -> Vec<String> {
        vec![
            "Navigation".to_string(),
            "  h/l or arrows: switch columns".to_string(),
            "  j/k or arrows: select task".to_string(),
            "Task Actions".to_string(),
            "  n: new task".to_string(),
            "  Enter: attach selected task".to_string(),
            "  J/K: move selected task in column".to_string(),
            "Category Management".to_string(),
            "  c: add category".to_string(),
            "  r: rename category".to_string(),
            "  x: delete category".to_string(),
            "  H/L: move focused category".to_string(),
            "General".to_string(),
            "  Ctrl+P: open command palette".to_string(),
            "  ?: toggle help".to_string(),
            "  Esc: dismiss".to_string(),
            "  q: quit (asks confirmation if sessions are active)".to_string(),
        ]
    }
}

impl Default for HelpDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl MockComponent for HelpDialog {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.shell.view(frame, area);
    }

    fn query(&self, attr: Attribute) -> Option<AttrValue> {
        self.props.get(attr)
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        self.props.set(attr, value);
    }

    fn state(&self) -> State {
        self.shell.state()
    }

    fn perform(&mut self, cmd: Cmd) -> CmdResult {
        self.shell.perform(cmd)
    }
}

impl Component<Msg, NoUserEvent> for HelpDialog {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match self.shell.on(ev) {
            Some(Msg::SubmitDialog) | Some(Msg::CancelAction) => Some(Msg::DismissDialog),
            msg => msg,
        }
    }
}

#[cfg(test)]
use crate::ui_realm::ComponentId;
#[cfg(test)]
use crate::ui_realm::tests::harness::{EventDriver, MockTerminal};
#[cfg(test)]
use crate::ui_realm::tests::helpers::{
    mount_component_for_test, render_component, render_simple_component, send_key_to_component,
};
#[cfg(test)]
use crossterm::event::KeyCode;

#[cfg(test)]
#[test]
fn renders() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(&driver, ComponentId::Help, Box::new(HelpDialog::new()));

    let output = render_simple_component(&mut app, ComponentId::Help);
    assert!(output.contains("Help"), "dialog title should render");
    assert!(output.contains("Navigation"), "help content should render");
    assert!(output.contains("[ Close ]"), "close button should render");
}

#[cfg(test)]
#[test]
fn close_emits_msg() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(&driver, ComponentId::Help, Box::new(HelpDialog::new()));

    let enter_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Enter], 1);
    assert_eq!(enter_messages, vec![Msg::DismissDialog]);

    let esc_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Esc], 1);
    assert_eq!(esc_messages, vec![Msg::DismissDialog]);
}

#[cfg(test)]
#[test]
fn content_contains_key_hints() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(&driver, ComponentId::Help, Box::new(HelpDialog::new()));

    let mut terminal = MockTerminal::new(120, 32);
    let output = render_component(&mut app, ComponentId::Help, &mut terminal);
    assert!(
        output.contains("n: new task"),
        "new task hint should render"
    );
    assert!(
        output.contains("Enter: attach selected task"),
        "attach hint should render"
    );
    assert!(
        output.contains("Ctrl+P: open command palette"),
        "command palette hint should render"
    );
    assert!(
        output.contains("?: toggle help"),
        "toggle help hint should render"
    );
    assert!(
        output.contains("Esc: dismiss"),
        "dismiss hint should render"
    );
}
