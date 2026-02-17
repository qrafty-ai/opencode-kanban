use std::collections::HashMap;

use tuirealm::application::ApplicationResult;
use tuirealm::listener::EventListenerCfg;
use tuirealm::{Application, Frame, NoUserEvent, PollStrategy, Update, tui::layout::Rect};
use uuid::Uuid;

use super::ComponentId;
use super::components::{
    CategoryInputDialog, CategoryInputMode, CommandPalette, ConfirmQuitDialog, ContextMenu,
    ContextMenuEntry, DeleteCategoryContext, DeleteCategoryDialog, DeleteTaskContext,
    DeleteTaskDialog, DialogButton, DialogShell, ErrorDialog, ErrorDialogVariant, Footer,
    HelpDialog, KanbanColumn, NewProjectDialog, NewTaskDialog, ProjectList, SidePanel, TaskCard,
};
use super::messages::Msg;
use super::model::Model;
use crate::types::{Category, Repo, Task};

pub struct TuiApplication {
    app: Application<ComponentId, Msg, NoUserEvent>,
    last_viewport: Option<(u16, u16)>,
    active_modal: Option<ComponentId>,
    previous_non_modal_focus: Option<ComponentId>,
}

impl TuiApplication {
    pub fn new() -> Self {
        Self::with_listener(EventListenerCfg::default())
    }

    pub fn with_listener(listener_cfg: EventListenerCfg<NoUserEvent>) -> Self {
        Self {
            app: Application::init(listener_cfg),
            last_viewport: None,
            active_modal: None,
            previous_non_modal_focus: None,
        }
    }

    pub fn tick(&mut self, strategy: PollStrategy) -> ApplicationResult<Vec<Msg>> {
        self.app.tick(strategy)
    }

    pub fn tick_and_update<M>(
        &mut self,
        model: &mut M,
        strategy: PollStrategy,
    ) -> ApplicationResult<()>
    where
        M: Update<Msg>,
    {
        let messages = self.tick(strategy)?;
        for msg in messages {
            let mut next = Some(msg);
            while next.is_some() {
                next = model.update(next);
            }
        }
        Ok(())
    }

    pub fn app(&self) -> &Application<ComponentId, Msg, NoUserEvent> {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut Application<ComponentId, Msg, NoUserEvent> {
        &mut self.app
    }

    pub fn view(&mut self, id: &ComponentId, frame: &mut Frame<'_>, area: Rect) {
        self.app.view(id, frame, area);
    }

    pub fn mount_all_components(&mut self) -> ApplicationResult<()> {
        let category = placeholder_category();
        let repo = placeholder_repo();
        let task = placeholder_task(repo.id, category.id);
        let categories = vec![category.clone()];
        let repos = vec![repo.clone()];
        let tasks = vec![task.clone()];
        let repo_lookup: HashMap<Uuid, Repo> = vec![(repo.id, repo.clone())].into_iter().collect();
        let repo_name_by_id: HashMap<Uuid, String> = repos
            .iter()
            .map(|repo| (repo.id, repo.name.clone()))
            .collect();

        self.app.remount(
            ComponentId::ProjectList,
            Box::new(ProjectList::new(vec![repo.name.clone()])),
            vec![],
        )?;
        self.app.remount(
            ComponentId::KanbanColumn(0),
            Box::new(KanbanColumn::new(
                0,
                category,
                vec![task.clone()],
                repo_name_by_id.clone(),
            )),
            vec![],
        )?;
        self.app.remount(
            ComponentId::TaskCard(0),
            Box::new(TaskCard::new(0, Some(task.clone()), Some(repo), true)),
            vec![],
        )?;
        self.app.remount(
            ComponentId::SidePanel,
            Box::new(SidePanel::new(
                categories.clone(),
                tasks.clone(),
                repos.clone(),
                None,
            )),
            vec![],
        )?;
        self.app.remount(
            ComponentId::ContextMenu,
            Box::new(ContextMenu::new(
                "Task Actions",
                vec![
                    ContextMenuEntry::new("Attach", Msg::AttachTask),
                    ContextMenuEntry::new("Delete", Msg::OpenDeleteTaskDialog),
                    ContextMenuEntry::new("Move Right", Msg::MoveTaskRight),
                ],
            )),
            vec![],
        )?;
        self.app
            .remount(ComponentId::Footer, Box::new(Footer::new()), vec![])?;
        self.app.remount(
            ComponentId::CommandPalette,
            Box::new(CommandPalette::new(HashMap::new())),
            vec![],
        )?;

        self.app
            .remount(ComponentId::NewTask, Box::new(NewTaskDialog::new()), vec![])?;
        self.app.remount(
            ComponentId::DeleteTask,
            Box::new(DeleteTaskDialog::new(delete_task_context(
                &tasks,
                &repo_lookup,
            ))),
            vec![],
        )?;
        self.app.remount(
            ComponentId::CategoryInput,
            Box::new(CategoryInputDialog::new(CategoryInputMode::New, "")),
            vec![],
        )?;
        self.app.remount(
            ComponentId::DeleteCategory,
            Box::new(DeleteCategoryDialog::new(delete_category_context(
                &categories,
                &tasks,
            ))),
            vec![],
        )?;
        self.app.remount(
            ComponentId::NewProject,
            Box::new(NewProjectDialog::new()),
            vec![],
        )?;
        self.app.remount(
            ComponentId::ConfirmQuit,
            Box::new(ConfirmQuitDialog::new(0)),
            vec![],
        )?;
        self.app
            .remount(ComponentId::Help, Box::new(HelpDialog::new()), vec![])?;
        self.app.remount(
            ComponentId::WorktreeNotFound,
            Box::new(ErrorDialog::new(ErrorDialogVariant::WorktreeNotFound {
                task_title: task.title.clone(),
            })),
            vec![],
        )?;
        self.app.remount(
            ComponentId::RepoUnavailable,
            Box::new(ErrorDialog::new(ErrorDialogVariant::RepoUnavailable {
                task_title: task.title,
                repo_path: repos
                    .first()
                    .map(|repo| repo.path.clone())
                    .unwrap_or_else(|| "/missing/repository".to_string()),
            })),
            vec![],
        )?;
        self.app.remount(
            ComponentId::Error,
            Box::new(ErrorDialog::new(ErrorDialogVariant::Generic {
                title: "Error".to_string(),
                detail: "Unexpected error".to_string(),
            })),
            vec![],
        )?;
        self.app.remount(
            ComponentId::MoveTask,
            Box::new(DialogShell::new(
                "Move Task",
                vec![
                    "Move task dialog is not implemented yet.".to_string(),
                    "Use H/L to move selected task between categories.".to_string(),
                ],
                vec![DialogButton::new("close", "Close")],
            )),
            vec![],
        )?;

        Ok(())
    }

    pub fn handle_resize(&mut self, model: &Model, msg: &Msg) -> ApplicationResult<bool> {
        let (width, height) = match *msg {
            Msg::Resize { width, height } => (width, height),
            _ => return Ok(false),
        };

        let next_viewport = (width, height);
        if self.last_viewport == Some(next_viewport) {
            return Ok(false);
        }

        let previous_focus = self.app.focus().copied();
        self.wire_components(model)?;

        let focus_target = self
            .active_modal
            .filter(|component_id| self.app.mounted(component_id))
            .or_else(|| {
                previous_focus
                    .filter(|component_id| component_exists_in_layout(*component_id, model))
                    .filter(|component_id| self.app.mounted(component_id))
            })
            .unwrap_or(ComponentId::ProjectList);
        self.app.active(&focus_target)?;

        self.last_viewport = Some(next_viewport);
        Ok(true)
    }

    pub fn route_modal_focus(&mut self, model: &Model, msg: &Msg) -> ApplicationResult<bool> {
        if let Some(dialog_component) = dialog_component_for_message(msg) {
            if !self.app.mounted(&dialog_component) {
                return Ok(false);
            }

            if self.active_modal.is_none() {
                self.previous_non_modal_focus = self
                    .app
                    .focus()
                    .copied()
                    .filter(|component_id| !is_modal_component(*component_id));
            }

            self.active_modal = Some(dialog_component);

            if self.app.focus().copied() != Some(dialog_component) {
                self.app.active(&dialog_component)?;
                return Ok(true);
            }

            return Ok(false);
        }

        if closes_modal(msg) {
            if self.active_modal.take().is_none() {
                return Ok(false);
            }

            let focus_target = self
                .previous_non_modal_focus
                .take()
                .filter(|component_id| component_exists_in_layout(*component_id, model))
                .filter(|component_id| self.app.mounted(component_id))
                .unwrap_or(ComponentId::ProjectList);

            if self.app.focus().copied() != Some(focus_target) {
                self.app.active(&focus_target)?;
            }

            return Ok(true);
        }

        if let Some(dialog_component) = self
            .active_modal
            .filter(|component_id| self.app.mounted(component_id))
            && self.app.focus().copied() != Some(dialog_component)
        {
            self.app.active(&dialog_component)?;
            return Ok(true);
        }

        Ok(false)
    }

    pub fn wire_components(&mut self, model: &Model) -> ApplicationResult<()> {
        self.mount_all_components()?;

        let categories = sorted_categories(&model.categories);
        let repos = sorted_repos(&model.repos);
        let ordered_tasks = sorted_tasks_for_mount(&model.tasks, &categories);
        let repo_lookup: HashMap<Uuid, Repo> =
            repos.iter().cloned().map(|repo| (repo.id, repo)).collect();
        let repo_name_by_id: HashMap<Uuid, String> = repos
            .iter()
            .map(|repo| (repo.id, repo.name.clone()))
            .collect();

        self.app.remount(
            ComponentId::ProjectList,
            Box::new(ProjectList::new(
                repos.iter().map(|repo| repo.name.clone()).collect(),
            )),
            vec![],
        )?;

        if categories.is_empty() {
            self.app.remount(
                ComponentId::KanbanColumn(0),
                Box::new(KanbanColumn::new(
                    0,
                    placeholder_category(),
                    Vec::new(),
                    repo_name_by_id.clone(),
                )),
                vec![],
            )?;
        } else {
            for (column_index, category) in categories.iter().cloned().enumerate() {
                let column_tasks = sorted_tasks_for_category(category.id, &model.tasks);
                self.app.remount(
                    ComponentId::KanbanColumn(column_index),
                    Box::new(KanbanColumn::new(
                        column_index,
                        category,
                        column_tasks,
                        repo_name_by_id.clone(),
                    )),
                    vec![],
                )?;
            }
        }

        if ordered_tasks.is_empty() {
            self.app.remount(
                ComponentId::TaskCard(0),
                Box::new(TaskCard::new(0, None, None, false)),
                vec![],
            )?;
        } else {
            for (card_index, task) in ordered_tasks.iter().cloned().enumerate() {
                self.app.remount(
                    ComponentId::TaskCard(card_index),
                    Box::new(TaskCard::new(
                        card_index,
                        Some(task.clone()),
                        repo_lookup.get(&task.repo_id).cloned(),
                        card_index == 0,
                    )),
                    vec![],
                )?;
            }
        }

        self.app.remount(
            ComponentId::SidePanel,
            Box::new(SidePanel::new(
                categories.clone(),
                ordered_tasks.clone(),
                repos.clone(),
                None,
            )),
            vec![],
        )?;

        self.app.remount(
            ComponentId::ContextMenu,
            Box::new(ContextMenu::new(
                "Task Actions",
                vec![
                    ContextMenuEntry::new("Attach", Msg::AttachTask),
                    ContextMenuEntry::new("Delete", Msg::OpenDeleteTaskDialog),
                    ContextMenuEntry::new("Move Right", Msg::MoveTaskRight),
                ],
            )),
            vec![],
        )?;
        self.app
            .remount(ComponentId::Footer, Box::new(Footer::new()), vec![])?;
        self.app.remount(
            ComponentId::CommandPalette,
            Box::new(CommandPalette::new(HashMap::new())),
            vec![],
        )?;

        self.app
            .remount(ComponentId::NewTask, Box::new(NewTaskDialog::new()), vec![])?;
        self.app.remount(
            ComponentId::DeleteTask,
            Box::new(DeleteTaskDialog::new(delete_task_context(
                &ordered_tasks,
                &repo_lookup,
            ))),
            vec![],
        )?;
        self.app.remount(
            ComponentId::CategoryInput,
            Box::new(CategoryInputDialog::new(CategoryInputMode::New, "")),
            vec![],
        )?;
        self.app.remount(
            ComponentId::DeleteCategory,
            Box::new(DeleteCategoryDialog::new(delete_category_context(
                &categories,
                &model.tasks,
            ))),
            vec![],
        )?;
        self.app.remount(
            ComponentId::NewProject,
            Box::new(NewProjectDialog::new()),
            vec![],
        )?;
        self.app.remount(
            ComponentId::ConfirmQuit,
            Box::new(ConfirmQuitDialog::new(0)),
            vec![],
        )?;
        self.app
            .remount(ComponentId::Help, Box::new(HelpDialog::new()), vec![])?;
        self.app.remount(
            ComponentId::WorktreeNotFound,
            Box::new(ErrorDialog::new(ErrorDialogVariant::WorktreeNotFound {
                task_title: ordered_tasks
                    .first()
                    .map(|task| task.title.clone())
                    .unwrap_or_default(),
            })),
            vec![],
        )?;
        self.app.remount(
            ComponentId::RepoUnavailable,
            Box::new(ErrorDialog::new(ErrorDialogVariant::RepoUnavailable {
                task_title: ordered_tasks
                    .first()
                    .map(|task| task.title.clone())
                    .unwrap_or_default(),
                repo_path: repos
                    .first()
                    .map(|repo| repo.path.clone())
                    .unwrap_or_else(|| "/missing/repository".to_string()),
            })),
            vec![],
        )?;
        self.app.remount(
            ComponentId::Error,
            Box::new(ErrorDialog::new(ErrorDialogVariant::Generic {
                title: "Error".to_string(),
                detail: "Unexpected error".to_string(),
            })),
            vec![],
        )?;
        self.app.remount(
            ComponentId::MoveTask,
            Box::new(DialogShell::new(
                "Move Task",
                vec![
                    "Move task dialog is not implemented yet.".to_string(),
                    "Use H/L to move selected task between categories.".to_string(),
                ],
                vec![DialogButton::new("close", "Close")],
            )),
            vec![],
        )?;

        Ok(())
    }
}

fn sorted_categories(categories: &[Category]) -> Vec<Category> {
    let mut sorted = categories.to_vec();
    sorted.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });
    sorted
}

fn sorted_repos(repos: &[Repo]) -> Vec<Repo> {
    let mut sorted = repos.to_vec();
    sorted.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.id.cmp(&right.id))
    });
    sorted
}

fn sorted_tasks_for_mount(tasks: &[Task], categories: &[Category]) -> Vec<Task> {
    let category_order: HashMap<Uuid, usize> = categories
        .iter()
        .enumerate()
        .map(|(index, category)| (category.id, index))
        .collect();

    let mut sorted = tasks.to_vec();
    sorted.sort_by(|left, right| {
        category_order
            .get(&left.category_id)
            .copied()
            .unwrap_or(usize::MAX)
            .cmp(
                &category_order
                    .get(&right.category_id)
                    .copied()
                    .unwrap_or(usize::MAX),
            )
            .then_with(|| left.position.cmp(&right.position))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });

    sorted
}

fn sorted_tasks_for_category(category_id: Uuid, tasks: &[Task]) -> Vec<Task> {
    let mut category_tasks = tasks
        .iter()
        .filter(|task| task.category_id == category_id)
        .cloned()
        .collect::<Vec<_>>();
    category_tasks.sort_by(|left, right| {
        left.position
            .cmp(&right.position)
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    category_tasks
}

fn component_exists_in_layout(component_id: ComponentId, model: &Model) -> bool {
    match component_id {
        ComponentId::KanbanColumn(index) => {
            let category_count = model.categories.len();
            if category_count == 0 {
                index == 0
            } else {
                index < category_count
            }
        }
        ComponentId::TaskCard(index) => {
            let categories = sorted_categories(&model.categories);
            let task_count = sorted_tasks_for_mount(&model.tasks, &categories).len();
            if task_count == 0 {
                index == 0
            } else {
                index < task_count
            }
        }
        _ => true,
    }
}

fn dialog_component_for_message(msg: &Msg) -> Option<ComponentId> {
    match msg {
        Msg::OpenNewTaskDialog => Some(ComponentId::NewTask),
        Msg::OpenDeleteTaskDialog => Some(ComponentId::DeleteTask),
        Msg::OpenAddCategoryDialog | Msg::OpenRenameCategoryDialog => {
            Some(ComponentId::CategoryInput)
        }
        Msg::OpenDeleteCategoryDialog => Some(ComponentId::DeleteCategory),
        Msg::OpenNewProjectDialog => Some(ComponentId::NewProject),
        Msg::OpenQuitDialog => Some(ComponentId::ConfirmQuit),
        Msg::OpenCommandPalette => Some(ComponentId::CommandPalette),
        Msg::ShowError(_) => Some(ComponentId::Error),
        Msg::ExecuteCommand(command) if command == "help" => Some(ComponentId::Help),
        _ => None,
    }
}

fn closes_modal(msg: &Msg) -> bool {
    matches!(
        msg,
        Msg::DismissDialog | Msg::CancelQuit | Msg::DismissRepoError
    )
}

fn is_modal_component(component_id: ComponentId) -> bool {
    matches!(
        component_id,
        ComponentId::CommandPalette
            | ComponentId::NewTask
            | ComponentId::DeleteTask
            | ComponentId::CategoryInput
            | ComponentId::DeleteCategory
            | ComponentId::NewProject
            | ComponentId::ConfirmQuit
            | ComponentId::Help
            | ComponentId::WorktreeNotFound
            | ComponentId::RepoUnavailable
            | ComponentId::Error
            | ComponentId::MoveTask
    )
}

fn delete_task_context(tasks: &[Task], repos: &HashMap<Uuid, Repo>) -> Option<DeleteTaskContext> {
    tasks.first().map(|task| DeleteTaskContext {
        title: task.title.clone(),
        repo_name: repos
            .get(&task.repo_id)
            .map(|repo| repo.name.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        branch: task.branch.clone(),
    })
}

fn delete_category_context(
    categories: &[Category],
    tasks: &[Task],
) -> Option<DeleteCategoryContext> {
    categories.first().map(|category| {
        let task_count = tasks
            .iter()
            .filter(|task| task.category_id == category.id)
            .count();
        DeleteCategoryContext::new(category.name.clone(), task_count)
    })
}

fn placeholder_category() -> Category {
    Category {
        id: Uuid::from_u128(0xCA7E_0000_0000_0000_0000_0000_0000_0001),
        name: "TODO".to_string(),
        position: 0,
        color: None,
        created_at: "1970-01-01T00:00:00Z".to_string(),
    }
}

fn placeholder_repo() -> Repo {
    Repo {
        id: Uuid::from_u128(0xCA7E_0000_0000_0000_0000_0000_0000_0010),
        path: "/missing/repository".to_string(),
        name: "placeholder-repo".to_string(),
        default_base: Some("main".to_string()),
        remote_url: None,
        created_at: "1970-01-01T00:00:00Z".to_string(),
        updated_at: "1970-01-01T00:00:00Z".to_string(),
    }
}

fn placeholder_task(repo_id: Uuid, category_id: Uuid) -> Task {
    Task {
        id: Uuid::from_u128(0xCA7E_0000_0000_0000_0000_0000_0000_0100),
        title: "Placeholder Task".to_string(),
        repo_id,
        branch: "placeholder/task".to_string(),
        category_id,
        position: 0,
        tmux_session_name: None,
        worktree_path: None,
        tmux_status: "idle".to_string(),
        status_source: "placeholder".to_string(),
        status_fetched_at: None,
        status_error: None,
        created_at: "1970-01-01T00:00:00Z".to_string(),
        updated_at: "1970-01-01T00:00:00Z".to_string(),
    }
}

impl Default for TuiApplication {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod application {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::super::ComponentId;
    use super::super::messages::Msg;
    use super::super::model::Model;
    use super::TuiApplication;
    use crate::db::Database;
    use crate::types::{Category, Repo, Task};
    use crate::ui_realm::tests::harness::MockTerminal;
    use tuirealm::command::{Cmd, CmdResult};
    use tuirealm::listener::{EventListenerCfg, ListenerResult, Poll};
    use tuirealm::{
        AttrValue, Attribute, Component, Event, MockComponent, NoUserEvent, PollStrategy, Props,
        State,
    };
    use uuid::Uuid;

    #[derive(Clone, Default)]
    struct MessagePoll {
        queue: Arc<Mutex<VecDeque<Event<NoUserEvent>>>>,
    }

    impl MessagePoll {
        fn listener_cfg(&self) -> EventListenerCfg<NoUserEvent> {
            EventListenerCfg::default()
                .poll_timeout(Duration::from_millis(5))
                .port(Box::new(self.clone()), Duration::from_millis(1))
        }

        fn send_tick(&self) {
            self.queue
                .lock()
                .expect("message poll queue lock should not be poisoned")
                .push_back(Event::Tick);
        }
    }

    impl Poll<NoUserEvent> for MessagePoll {
        fn poll(&mut self) -> ListenerResult<Option<Event<NoUserEvent>>> {
            Ok(self
                .queue
                .lock()
                .expect("message poll queue lock should not be poisoned")
                .pop_front())
        }
    }

    #[derive(Default)]
    struct TickComponent {
        props: Props,
    }

    impl MockComponent for TickComponent {
        fn view(&mut self, _frame: &mut tuirealm::Frame<'_>, _area: tuirealm::tui::layout::Rect) {}

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

    impl Component<Msg, NoUserEvent> for TickComponent {
        fn on(&mut self, ev: Event<NoUserEvent>) -> Option<Msg> {
            match ev {
                Event::Tick => Some(Msg::Tick),
                _ => None,
            }
        }
    }

    #[test]
    fn new_creates() {
        let app = TuiApplication::new();
        assert!(app.app().focus().is_none());
    }

    #[test]
    fn tick_returns_messages() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());

        app.app_mut()
            .mount(
                ComponentId::Footer,
                Box::new(TickComponent::default()),
                vec![],
            )
            .expect("tick component should mount");
        app.app_mut()
            .active(&ComponentId::Footer)
            .expect("tick component should become active");

        poll.send_tick();

        let messages = app
            .tick(PollStrategy::UpTo(4))
            .expect("tick should return queued messages");
        assert_eq!(messages, vec![Msg::Tick]);
    }

    #[test]
    fn mounts_all_component_ids() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());

        app.mount_all_components()
            .expect("mount_all_components should mount all component ids");

        for id in all_component_ids() {
            assert!(
                app.app().mounted(&id),
                "component {id:?} should be mounted by mount_all_components"
            );
        }
    }

    #[test]
    fn mount_all_is_idempotent() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());

        app.mount_all_components()
            .expect("initial mount_all_components should succeed");
        app.mount_all_components()
            .expect("repeated mount_all_components should not fail");

        for id in all_component_ids() {
            assert!(
                app.app().mounted(&id),
                "component {id:?} should remain mounted after repeated mount_all_components"
            );
        }
    }

    #[test]
    fn resize_recalculation_on_change() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let model = model_for_resize_tests();

        let did_rewire = app
            .handle_resize(
                &model,
                &Msg::Resize {
                    width: 120,
                    height: 40,
                },
            )
            .expect("resize handling should succeed");

        assert!(did_rewire, "initial resize should trigger rewiring");
        assert!(
            app.app().mounted(&ComponentId::ProjectList),
            "resize rewiring should mount wired components"
        );

        let did_rewire = app
            .handle_resize(
                &model,
                &Msg::Resize {
                    width: 140,
                    height: 42,
                },
            )
            .expect("changed resize handling should succeed");
        assert!(did_rewire, "viewport changes should trigger rewiring");
    }

    #[test]
    fn resize_no_rewire_on_same_size() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let model = model_for_resize_tests();

        app.handle_resize(
            &model,
            &Msg::Resize {
                width: 100,
                height: 30,
            },
        )
        .expect("initial resize handling should succeed");

        app.app_mut()
            .active(&ComponentId::Footer)
            .expect("footer should become active before no-op resize");

        let did_rewire = app
            .handle_resize(
                &model,
                &Msg::Resize {
                    width: 100,
                    height: 30,
                },
            )
            .expect("same-size resize handling should succeed");

        assert!(
            !did_rewire,
            "same-size viewport should not trigger rewiring"
        );
        assert_eq!(app.app().focus(), Some(&ComponentId::Footer));
    }

    #[test]
    fn resize_preserves_focus() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let mut model = model_for_resize_tests();

        model.categories.push(test_category("Doing", 1));
        model.categories.push(test_category("Done", 2));

        app.wire_components(&model)
            .expect("initial wiring should succeed");
        app.app_mut()
            .active(&ComponentId::KanbanColumn(2))
            .expect("third kanban column should become active");

        app.handle_resize(
            &model,
            &Msg::Resize {
                width: 110,
                height: 35,
            },
        )
        .expect("resize rewiring should succeed");
        assert_eq!(app.app().focus(), Some(&ComponentId::KanbanColumn(2)));

        model.categories.truncate(1);

        app.handle_resize(
            &model,
            &Msg::Resize {
                width: 120,
                height: 36,
            },
        )
        .expect("resize fallback rewiring should succeed");
        assert_eq!(app.app().focus(), Some(&ComponentId::ProjectList));
    }

    #[test]
    fn resize_storm_does_not_panic() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let mut model = model_for_resize_tests();

        model.categories.push(test_category("Doing", 1));
        model.categories.push(test_category("Done", 2));

        app.wire_components(&model)
            .expect("initial wiring should succeed");
        app.app_mut()
            .active(&ComponentId::KanbanColumn(2))
            .expect("third kanban column should become active before resize storm");

        let resize_storm = [
            (120, 40),
            (120, 40),
            (40, 20),
            (160, 48),
            (200, 60),
            (80, 24),
            (1, 1),
            (220, 70),
            (95, 28),
            (95, 28),
            (140, 42),
            (72, 18),
            (180, 54),
            (100, 30),
            (132, 38),
        ];

        for (index, (width, height)) in resize_storm.into_iter().enumerate() {
            if index == 5 {
                model.categories.truncate(1);
            }
            if index == 10 {
                model.categories.push(test_category("Doing", 1));
                model.categories.push(test_category("Done", 2));
            }

            app.handle_resize(&model, &Msg::Resize { width, height })
                .expect("resize storm event should not panic");

            assert_focus_is_valid(&app, &model);
        }
    }

    #[test]
    fn modal_focus_opens_dialog() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let model = model_for_resize_tests();

        app.wire_components(&model)
            .expect("initial wiring should succeed");
        app.app_mut()
            .active(&ComponentId::Footer)
            .expect("footer should become active before opening dialog");

        let routed = app
            .route_modal_focus(&model, &Msg::OpenNewTaskDialog)
            .expect("modal focus routing should succeed");

        assert!(routed, "dialog-open message should route focus to dialog");
        assert_eq!(app.app().focus(), Some(&ComponentId::NewTask));
    }

    #[test]
    fn modal_focus_blocks_background() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let model = model_for_resize_tests();

        app.wire_components(&model)
            .expect("initial wiring should succeed");
        app.app_mut()
            .active(&ComponentId::ProjectList)
            .expect("project list should become active before opening dialog");
        app.route_modal_focus(&model, &Msg::OpenDeleteTaskDialog)
            .expect("modal open should route focus");

        app.app_mut()
            .active(&ComponentId::Footer)
            .expect("background component should become active for test setup");
        assert_eq!(app.app().focus(), Some(&ComponentId::Footer));

        let rerouted = app
            .route_modal_focus(&model, &Msg::Tick)
            .expect("modal enforcement should succeed");

        assert!(
            rerouted,
            "active modal should reclaim focus from background"
        );
        assert_eq!(app.app().focus(), Some(&ComponentId::DeleteTask));
    }

    #[test]
    fn modal_focus_restores_previous() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let model = model_for_resize_tests();

        app.wire_components(&model)
            .expect("initial wiring should succeed");
        app.app_mut()
            .active(&ComponentId::KanbanColumn(0))
            .expect("kanban column should become active before opening dialog");

        app.route_modal_focus(&model, &Msg::OpenAddCategoryDialog)
            .expect("modal open should route focus");
        assert_eq!(app.app().focus(), Some(&ComponentId::CategoryInput));

        let restored = app
            .route_modal_focus(&model, &Msg::DismissDialog)
            .expect("modal dismiss should restore focus");

        assert!(restored, "modal dismiss should restore background focus");
        assert_eq!(app.app().focus(), Some(&ComponentId::KanbanColumn(0)));
    }

    #[test]
    fn modal_focus_fallback_to_project_list() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let mut model = model_for_resize_tests();

        model.categories.push(test_category("Doing", 1));
        model.categories.push(test_category("Done", 2));

        app.wire_components(&model)
            .expect("initial wiring should succeed");
        app.app_mut()
            .active(&ComponentId::KanbanColumn(2))
            .expect("third kanban column should become active before opening dialog");

        app.route_modal_focus(&model, &Msg::OpenNewProjectDialog)
            .expect("modal open should route focus");
        assert_eq!(app.app().focus(), Some(&ComponentId::NewProject));

        model.categories.truncate(1);

        let restored = app
            .route_modal_focus(&model, &Msg::DismissDialog)
            .expect("modal dismiss should restore or fallback focus");

        assert!(restored, "modal dismiss should trigger focus routing");
        assert_eq!(app.app().focus(), Some(&ComponentId::ProjectList));
    }

    #[test]
    fn modal_focus_under_event_storm() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let mut model = model_for_resize_tests();

        model.categories.push(test_category("Doing", 1));
        model.categories.push(test_category("Done", 2));

        app.wire_components(&model)
            .expect("initial wiring should succeed");
        app.app_mut()
            .active(&ComponentId::KanbanColumn(1))
            .expect("second kanban column should become active before opening dialog");

        let opened = app
            .route_modal_focus(&model, &Msg::OpenDeleteTaskDialog)
            .expect("modal open should route focus");
        assert!(opened, "opening a modal should route focus");
        assert_eq!(app.app().focus(), Some(&ComponentId::DeleteTask));

        let storm_resizes = [
            (120, 40),
            (96, 28),
            (140, 42),
            (80, 24),
            (1, 1),
            (132, 38),
            (160, 48),
            (100, 30),
        ];

        for (index, (width, height)) in storm_resizes.into_iter().enumerate() {
            let background_focus = if index % 2 == 0 {
                ComponentId::Footer
            } else {
                ComponentId::ProjectList
            };

            app.app_mut()
                .active(&background_focus)
                .expect("test setup should be able to steal focus");

            let rerouted = app
                .route_modal_focus(&model, &Msg::Tick)
                .expect("modal should reclaim focus during event storm");
            assert!(rerouted, "modal should reclaim focus from background");
            assert_eq!(app.app().focus(), Some(&ComponentId::DeleteTask));

            app.handle_resize(&model, &Msg::Resize { width, height })
                .expect("resize handling should not panic while modal is open");
            assert_eq!(app.app().focus(), Some(&ComponentId::DeleteTask));
        }

        let restored = app
            .route_modal_focus(&model, &Msg::DismissDialog)
            .expect("modal dismiss should restore previous focus");
        assert!(restored, "dismissing modal should route focus");
        assert_eq!(app.app().focus(), Some(&ComponentId::KanbanColumn(1)));

        app.route_modal_focus(&model, &Msg::OpenCommandPalette)
            .expect("command palette open should route focus");
        assert_eq!(app.app().focus(), Some(&ComponentId::CommandPalette));

        model.categories.truncate(1);

        let fallback = app
            .route_modal_focus(&model, &Msg::DismissDialog)
            .expect("modal dismiss should fallback when previous focus is invalid");
        assert!(fallback, "closing modal should restore or fallback focus");
        assert_eq!(app.app().focus(), Some(&ComponentId::ProjectList));
    }

    #[test]
    fn long_content_renders_safely_and_is_clipped() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let mut model = model_for_resize_tests();

        let category_id = Uuid::from_u128(0xCA7E_0000_0000_0000_0000_0000_0000_2000);
        let repo_id = Uuid::from_u128(0xCA7E_0000_0000_0000_0000_0000_0000_3000);
        let long_category = format!("TODO-{}", "category".repeat(32));
        let long_title = format!("task-{}", "x".repeat(320));
        let long_repo_name = format!("repo-{}", "r".repeat(256));
        let long_branch = format!("branch-{}", "b".repeat(320));

        model.categories = vec![Category {
            id: category_id,
            name: long_category,
            position: 0,
            color: None,
            created_at: "1970-01-01T00:00:00Z".to_string(),
        }];
        model.repos = vec![Repo {
            id: repo_id,
            path: "/tmp/very/long/path/for/repo".to_string(),
            name: long_repo_name,
            default_base: Some("main".to_string()),
            remote_url: None,
            created_at: "1970-01-01T00:00:00Z".to_string(),
            updated_at: "1970-01-01T00:00:00Z".to_string(),
        }];
        model.tasks = vec![Task {
            id: Uuid::from_u128(0xCA7E_0000_0000_0000_0000_0000_0000_4000),
            title: long_title.clone(),
            repo_id,
            branch: long_branch.clone(),
            category_id,
            position: 0,
            tmux_session_name: None,
            worktree_path: None,
            tmux_status: "running".to_string(),
            status_source: "manual".to_string(),
            status_fetched_at: None,
            status_error: None,
            created_at: "1970-01-01T00:00:00Z".to_string(),
            updated_at: "1970-01-01T00:00:00Z".to_string(),
        }];

        app.wire_components(&model)
            .expect("wiring should succeed with long content");

        let rendered_column = render_component(&mut app, ComponentId::KanbanColumn(0), 24, 6);
        assert!(
            rendered_column.contains("task-"),
            "visible prefix of long task title should render"
        );
        assert!(
            !rendered_column.contains(&long_title),
            "kanban column should clip long title to viewport"
        );

        let rendered_card = render_component(&mut app, ComponentId::TaskCard(0), 28, 6);
        assert!(
            rendered_card.contains("task-"),
            "task card should render a visible title prefix"
        );
        assert!(
            !rendered_card.contains(&long_title),
            "task card should clip long title to viewport"
        );
        assert!(
            !rendered_card.contains(&long_branch),
            "task card should clip long repo context to viewport"
        );
    }

    #[test]
    fn empty_states_for_major_views_render_safely() {
        let poll = MessagePoll::default();
        let mut app = TuiApplication::with_listener(poll.listener_cfg());
        let mut model = model_for_resize_tests();

        model.categories.clear();
        model.repos.clear();
        model.tasks.clear();

        app.wire_components(&model)
            .expect("wiring should succeed for empty model");

        let project_list = render_component(&mut app, ComponentId::ProjectList, 42, 8);
        assert!(
            project_list.contains("No projects available"),
            "project list should render empty-state text"
        );

        let kanban_column = render_component(&mut app, ComponentId::KanbanColumn(0), 42, 8);
        assert!(
            kanban_column.contains("No tasks in this category"),
            "kanban column should render empty-state text"
        );

        let task_card = render_component(&mut app, ComponentId::TaskCard(0), 42, 8);
        assert!(
            task_card.contains("No task data"),
            "task card should render empty-state text"
        );

        let side_panel = render_component(&mut app, ComponentId::SidePanel, 84, 16);
        assert!(
            side_panel.contains("No tasks available"),
            "side panel list should render empty-state text"
        );
        assert!(
            side_panel.contains("No task selected"),
            "side panel details should render empty-state text"
        );
    }

    fn model_for_resize_tests() -> Model {
        let db = Database::open(":memory:").expect("in-memory database should open");
        Model::new(db).expect("model should initialize from in-memory database")
    }

    fn test_category(name: &str, position: i64) -> Category {
        Category {
            id: Uuid::new_v4(),
            name: name.to_string(),
            position,
            color: None,
            created_at: "1970-01-01T00:00:00Z".to_string(),
        }
    }

    fn assert_focus_is_valid(app: &TuiApplication, model: &Model) {
        let focus = app
            .app()
            .focus()
            .copied()
            .expect("application should always have focus during resize storm");

        assert!(
            app.app().mounted(&focus),
            "focused component {focus:?} should remain mounted"
        );
        assert!(
            super::component_exists_in_layout(focus, model),
            "focused component {focus:?} should remain valid in current layout"
        );
    }

    fn render_component(
        app: &mut TuiApplication,
        id: ComponentId,
        width: u16,
        height: u16,
    ) -> String {
        let mut terminal = MockTerminal::new(width, height);
        terminal.draw(|frame| {
            app.view(&id, frame, frame.size());
        });
        terminal.buffer_as_string()
    }

    fn all_component_ids() -> [ComponentId; 18] {
        [
            ComponentId::ProjectList,
            ComponentId::KanbanColumn(0),
            ComponentId::TaskCard(0),
            ComponentId::SidePanel,
            ComponentId::ContextMenu,
            ComponentId::Footer,
            ComponentId::CommandPalette,
            ComponentId::NewTask,
            ComponentId::DeleteTask,
            ComponentId::CategoryInput,
            ComponentId::DeleteCategory,
            ComponentId::NewProject,
            ComponentId::ConfirmQuit,
            ComponentId::Help,
            ComponentId::WorktreeNotFound,
            ComponentId::RepoUnavailable,
            ComponentId::Error,
            ComponentId::MoveTask,
        ]
    }
}
