use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::Rect;
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State};

use crate::ui_realm::components::dialog_shell::{DialogButton, DialogShell};
use crate::ui_realm::messages::Msg;

pub struct ConfirmQuitDialog {
    props: Props,
    shell: DialogShell,
}

impl ConfirmQuitDialog {
    pub fn new(active_session_count: usize) -> Self {
        Self {
            props: Props::default(),
            shell: Self::build_shell(active_session_count),
        }
    }

    pub fn set_active_session_count(&mut self, active_session_count: usize) {
        self.shell = Self::build_shell(active_session_count);
    }

    fn build_shell(active_session_count: usize) -> DialogShell {
        DialogShell::new(
            "Confirm Quit",
            vec![format!(
                "{} active tmux session(s) still running.\nQuit anyway?",
                active_session_count
            )],
            vec![
                DialogButton::new("quit", "Quit"),
                DialogButton::new("cancel", "Cancel"),
            ],
        )
    }

    fn confirm_focused(&self) -> bool {
        self.shell.focused_button_label() == Some("Quit")
    }
}

impl MockComponent for ConfirmQuitDialog {
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

impl Component<Msg, NoUserEvent> for ConfirmQuitDialog {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match self.shell.on(ev) {
            Some(Msg::SubmitDialog) => {
                if self.confirm_focused() {
                    Some(Msg::ConfirmQuit)
                } else {
                    Some(Msg::DismissDialog)
                }
            }
            Some(Msg::CancelAction) => Some(Msg::DismissDialog),
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
    mount_component_for_test, render_component, send_key_to_component,
};
#[cfg(test)]
use crossterm::event::KeyCode;

#[cfg(test)]
#[test]
fn renders() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::ConfirmQuit,
        Box::new(ConfirmQuitDialog::new(3)),
    );

    let mut terminal = MockTerminal::new(70, 12);
    let output = render_component(&mut app, ComponentId::ConfirmQuit, &mut terminal);
    assert!(
        output.contains("Confirm Quit"),
        "dialog title should render"
    );
    assert!(
        output.contains("3 active tmux session"),
        "active session count should render"
    );
    assert!(
        output.contains("Quit anyway?"),
        "quit confirmation question should render"
    );
    assert!(output.contains("Quit"), "quit action should render");
    assert!(output.contains("Cancel"), "cancel action should render");
}

#[cfg(test)]
#[test]
fn focus_navigation() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::ConfirmQuit,
        Box::new(ConfirmQuitDialog::new(1)),
    );

    let messages = send_key_to_component(
        &driver,
        &mut app,
        &[KeyCode::Right, KeyCode::Left, KeyCode::Tab],
        1,
    );
    assert_eq!(
        messages,
        vec![
            Msg::FocusButton("Cancel".to_string()),
            Msg::FocusButton("Quit".to_string()),
            Msg::FocusButton("Cancel".to_string()),
        ]
    );
}

#[cfg(test)]
#[test]
fn confirm_emits_msg() {
    let driver = EventDriver::default();

    let mut confirm_app = mount_component_for_test(
        &driver,
        ComponentId::ConfirmQuit,
        Box::new(ConfirmQuitDialog::new(2)),
    );
    let confirm_messages = send_key_to_component(&driver, &mut confirm_app, &[KeyCode::Enter], 1);
    assert_eq!(confirm_messages, vec![Msg::ConfirmQuit]);

    let mut cancel_app = mount_component_for_test(
        &driver,
        ComponentId::ConfirmQuit,
        Box::new(ConfirmQuitDialog::new(2)),
    );
    let focus_cancel = send_key_to_component(&driver, &mut cancel_app, &[KeyCode::Right], 1);
    assert_eq!(focus_cancel, vec![Msg::FocusButton("Cancel".to_string())]);

    let cancel_messages = send_key_to_component(&driver, &mut cancel_app, &[KeyCode::Enter], 1);
    assert_eq!(cancel_messages, vec![Msg::DismissDialog]);

    let mut esc_app = mount_component_for_test(
        &driver,
        ComponentId::ConfirmQuit,
        Box::new(ConfirmQuitDialog::new(2)),
    );
    let esc_messages = send_key_to_component(&driver, &mut esc_app, &[KeyCode::Esc], 1);
    assert_eq!(esc_messages, vec![Msg::DismissDialog]);

    let mut esc_cancel_app = mount_component_for_test(
        &driver,
        ComponentId::ConfirmQuit,
        Box::new(ConfirmQuitDialog::new(2)),
    );
    let focused_cancel = send_key_to_component(&driver, &mut esc_cancel_app, &[KeyCode::Right], 1);
    assert_eq!(focused_cancel, vec![Msg::FocusButton("Cancel".to_string())]);
    let esc_after_focus = send_key_to_component(&driver, &mut esc_cancel_app, &[KeyCode::Esc], 1);
    assert_eq!(esc_after_focus, vec![Msg::DismissDialog]);
}
