use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::Rect;
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State};

use crate::ui_realm::components::dialog_shell::{DialogButton, DialogShell};
use crate::ui_realm::messages::Msg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteCategoryContext {
    pub category_name: String,
    pub task_count: usize,
}

impl DeleteCategoryContext {
    pub fn new(category_name: impl Into<String>, task_count: usize) -> Self {
        Self {
            category_name: category_name.into(),
            task_count,
        }
    }
}

pub struct DeleteCategoryDialog {
    props: Props,
    context: Option<DeleteCategoryContext>,
    shell: DialogShell,
}

impl DeleteCategoryDialog {
    pub fn new(context: Option<DeleteCategoryContext>) -> Self {
        let shell = Self::build_shell(&context);
        Self {
            props: Props::default(),
            context,
            shell,
        }
    }

    pub fn set_context(&mut self, context: Option<DeleteCategoryContext>) {
        self.context = context;
        self.shell = Self::build_shell(&self.context);
    }

    fn build_shell(context: &Option<DeleteCategoryContext>) -> DialogShell {
        DialogShell::new(
            "Delete Category",
            Self::content_lines(context),
            vec![
                DialogButton::new("delete", "Delete"),
                DialogButton::new("cancel", "Cancel"),
            ],
        )
    }

    fn content_lines(context: &Option<DeleteCategoryContext>) -> Vec<String> {
        match context {
            Some(details) if details.task_count == 0 => vec![
                format!("Delete category '{}' ?", details.category_name),
                "This action cannot be undone.".to_string(),
            ],
            Some(details) => vec![
                format!(
                    "Category '{}' has {} task(s).",
                    details.category_name, details.task_count
                ),
                "Deletion will fail until tasks are moved.".to_string(),
            ],
            None => vec![
                "No category selected.".to_string(),
                "Nothing will be deleted.".to_string(),
            ],
        }
    }

    fn confirm_focused(&self) -> bool {
        self.shell.focused_button_label() == Some("Delete")
    }
}

impl MockComponent for DeleteCategoryDialog {
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

impl Component<Msg, NoUserEvent> for DeleteCategoryDialog {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match self.shell.on(ev) {
            Some(Msg::SubmitDialog) => {
                if self.confirm_focused() && self.context.is_some() {
                    Some(Msg::ConfirmDeleteCategory)
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
        ComponentId::DeleteCategory,
        Box::new(DeleteCategoryDialog::new(Some(DeleteCategoryContext::new(
            "In Progress",
            2,
        )))),
    );

    let output = render_simple_component(&mut app, ComponentId::DeleteCategory);
    assert!(
        output.contains("Delete Category"),
        "dialog title should render"
    );
    assert!(
        output.contains("In Progress"),
        "category name should appear in confirmation text"
    );
    assert!(
        output.contains("Deletion will fail"),
        "impact text should explain why deletion is blocked"
    );
}

#[cfg(test)]
#[test]
fn focus_navigation() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::DeleteCategory,
        Box::new(DeleteCategoryDialog::new(Some(DeleteCategoryContext::new(
            "TODO", 0,
        )))),
    );

    let messages = send_key_to_component(&driver, &mut app, &[KeyCode::Right, KeyCode::Left], 1);
    assert_eq!(
        messages,
        vec![
            Msg::FocusButton("Cancel".to_string()),
            Msg::FocusButton("Delete".to_string()),
        ]
    );
}

#[cfg(test)]
#[test]
fn confirm_emits_msg() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::DeleteCategory,
        Box::new(DeleteCategoryDialog::new(Some(DeleteCategoryContext::new(
            "Done", 0,
        )))),
    );

    let confirm_messages = send_key_to_component(&driver, &mut app, &[KeyCode::Enter], 1);
    assert_eq!(confirm_messages, vec![Msg::ConfirmDeleteCategory]);

    let cancel_driver = EventDriver::default();
    let mut cancel_app = mount_component_for_test(
        &cancel_driver,
        ComponentId::DeleteCategory,
        Box::new(DeleteCategoryDialog::new(Some(DeleteCategoryContext::new(
            "Done", 0,
        )))),
    );
    let focus_cancel_messages =
        send_key_to_component(&cancel_driver, &mut cancel_app, &[KeyCode::Right], 1);
    assert_eq!(
        focus_cancel_messages,
        vec![Msg::FocusButton("Cancel".to_string())]
    );

    let cancel_messages =
        send_key_to_component(&cancel_driver, &mut cancel_app, &[KeyCode::Enter], 1);
    assert_eq!(cancel_messages, vec![Msg::DismissDialog]);

    let missing_context_driver = EventDriver::default();
    let mut missing_context_app = mount_component_for_test(
        &missing_context_driver,
        ComponentId::DeleteCategory,
        Box::new(DeleteCategoryDialog::new(None)),
    );
    let missing_context_messages = send_key_to_component(
        &missing_context_driver,
        &mut missing_context_app,
        &[KeyCode::Enter],
        1,
    );
    assert_eq!(missing_context_messages, vec![Msg::DismissDialog]);
}
