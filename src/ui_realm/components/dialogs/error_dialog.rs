use tuirealm::command::{Cmd, CmdResult};
use tuirealm::props::{AttrValue, Attribute, Props};
use tuirealm::tui::layout::Rect;
use tuirealm::{Component, Event, Frame, MockComponent, NoUserEvent, State};

use crate::ui_realm::components::dialog_shell::{DialogButton, DialogShell};
use crate::ui_realm::messages::Msg;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorDialogVariant {
    Generic {
        title: String,
        detail: String,
    },
    WorktreeNotFound {
        task_title: String,
    },
    RepoUnavailable {
        task_title: String,
        repo_path: String,
    },
}

pub struct ErrorDialog {
    props: Props,
    variant: ErrorDialogVariant,
    shell: DialogShell,
}

impl ErrorDialog {
    pub fn new(variant: ErrorDialogVariant) -> Self {
        let shell = Self::build_shell(&variant);
        Self {
            props: Props::default(),
            variant,
            shell,
        }
    }

    pub fn set_variant(&mut self, variant: ErrorDialogVariant) {
        self.shell = Self::build_shell(&variant);
        self.variant = variant;
    }

    fn build_shell(variant: &ErrorDialogVariant) -> DialogShell {
        let (title, content_lines, _) = Self::mapped_variant(variant);
        DialogShell::new(
            title,
            content_lines,
            vec![DialogButton::new("dismiss", "Dismiss")],
        )
    }

    fn mapped_variant(variant: &ErrorDialogVariant) -> (String, Vec<String>, Msg) {
        match variant {
            ErrorDialogVariant::Generic { title, detail } => {
                let normalized_title = if title.is_empty() {
                    "Error".to_string()
                } else {
                    title.clone()
                };
                let normalized_detail = if detail.is_empty() {
                    "An unexpected error occurred.".to_string()
                } else {
                    detail.clone()
                };
                (
                    "Error".to_string(),
                    vec![normalized_title, String::new(), normalized_detail],
                    Msg::DismissDialog,
                )
            }
            ErrorDialogVariant::WorktreeNotFound { task_title } => {
                let display_title = if task_title.is_empty() {
                    "selected task".to_string()
                } else {
                    format!("\"{task_title}\"")
                };
                (
                    "Worktree Not Found".to_string(),
                    vec![
                        format!("Worktree not found for {display_title}."),
                        "Task cannot be attached until the worktree is restored.".to_string(),
                    ],
                    Msg::DismissDialog,
                )
            }
            ErrorDialogVariant::RepoUnavailable {
                task_title,
                repo_path,
            } => {
                let display_title = if task_title.is_empty() {
                    "selected task".to_string()
                } else {
                    format!("\"{task_title}\"")
                };
                let display_repo_path = if repo_path.is_empty() {
                    "(missing path)".to_string()
                } else {
                    repo_path.clone()
                };
                (
                    "Repo Unavailable".to_string(),
                    vec![
                        format!("Repo unavailable for {display_title}."),
                        display_repo_path,
                    ],
                    Msg::DismissRepoError,
                )
            }
        }
    }

    fn dismiss_msg(&self) -> Msg {
        let (_, _, dismiss_msg) = Self::mapped_variant(&self.variant);
        dismiss_msg
    }
}

impl MockComponent for ErrorDialog {
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

impl Component<Msg, NoUserEvent> for ErrorDialog {
    fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
        match self.shell.on(ev) {
            Some(Msg::SubmitDialog) | Some(Msg::CancelAction) => Some(self.dismiss_msg()),
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
fn renders_generic() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::Error,
        Box::new(ErrorDialog::new(ErrorDialogVariant::Generic {
            title: "Task failed".to_string(),
            detail: "git worktree add failed".to_string(),
        })),
    );

    let output = render_simple_component(&mut app, ComponentId::Error);
    assert!(
        output.contains("Error"),
        "generic dialog title should render"
    );
    assert!(
        output.contains("Task failed"),
        "generic dialog message title should render"
    );
    assert!(
        output.contains("git worktree add failed"),
        "generic dialog detail should render"
    );
    assert!(
        output.contains("[ Dismiss ]"),
        "dismiss button should render"
    );
}

#[cfg(test)]
#[test]
fn renders_worktree_not_found() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::Error,
        Box::new(ErrorDialog::new(ErrorDialogVariant::WorktreeNotFound {
            task_title: "CI".to_string(),
        })),
    );

    let output = render_simple_component(&mut app, ComponentId::Error);
    assert!(
        output.contains("Worktree Not Found"),
        "worktree variant title should render"
    );
    assert!(
        output.contains("CI"),
        "worktree variant should include task title"
    );
    assert!(
        output.contains("Worktree not found for"),
        "worktree variant message should render"
    );
}

#[cfg(test)]
#[test]
fn renders_repo_unavailable() {
    let driver = EventDriver::default();
    let mut app = mount_component_for_test(
        &driver,
        ComponentId::Error,
        Box::new(ErrorDialog::new(ErrorDialogVariant::RepoUnavailable {
            task_title: "Auth".to_string(),
            repo_path: "/tmp/repo".to_string(),
        })),
    );

    let output = render_simple_component(&mut app, ComponentId::Error);
    assert!(
        output.contains("Repo Unavailable"),
        "repo variant title should render"
    );
    assert!(
        output.contains("Auth"),
        "repo variant should include task title"
    );
    assert!(
        output.contains("/tmp/repo"),
        "repo variant should include repo path"
    );
}

#[cfg(test)]
#[test]
fn dismiss_emits_msg() {
    let driver = EventDriver::default();

    let mut generic_app = mount_component_for_test(
        &driver,
        ComponentId::Error,
        Box::new(ErrorDialog::new(ErrorDialogVariant::Generic {
            title: String::new(),
            detail: String::new(),
        })),
    );
    let generic_messages = send_key_to_component(&driver, &mut generic_app, &[KeyCode::Enter], 1);
    assert_eq!(generic_messages, vec![Msg::DismissDialog]);

    let mut worktree_app = mount_component_for_test(
        &driver,
        ComponentId::Error,
        Box::new(ErrorDialog::new(ErrorDialogVariant::WorktreeNotFound {
            task_title: String::new(),
        })),
    );
    let worktree_messages = send_key_to_component(&driver, &mut worktree_app, &[KeyCode::Esc], 1);
    assert_eq!(worktree_messages, vec![Msg::DismissDialog]);

    let mut repo_app = mount_component_for_test(
        &driver,
        ComponentId::Error,
        Box::new(ErrorDialog::new(ErrorDialogVariant::RepoUnavailable {
            task_title: String::new(),
            repo_path: String::new(),
        })),
    );
    let repo_messages = send_key_to_component(&driver, &mut repo_app, &[KeyCode::Enter], 1);
    assert_eq!(repo_messages, vec![Msg::DismissRepoError]);
}
