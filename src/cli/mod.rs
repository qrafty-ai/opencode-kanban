use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::{Args, Subcommand};
use serde_json::{Value, json};
use tracing::{error, warn};
use uuid::Uuid;

use crate::{
    app::runtime::{
        CreateTaskRuntime, RealCreateTaskRuntime, next_available_session_name_by,
        worktrees_root_for_repo,
    },
    db::Database,
    git::derive_worktree_path,
    opencode::{Status, opencode_attach_command},
    projects,
    types::{Category, Repo, Task},
};

const SCHEMA_VERSION: &str = "cli.v1";

#[derive(Debug, Clone, Subcommand)]
pub enum RootCommand {
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    Category {
        #[command(subcommand)]
        command: CategoryCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TaskCommand {
    List(TaskListArgs),
    Create(TaskCreateArgs),
    Move(TaskMoveArgs),
    Archive(TaskArchiveArgs),
    Show(TaskShowArgs),
}

#[derive(Debug, Clone, Subcommand)]
pub enum CategoryCommand {
    List,
    Create(CategoryCreateArgs),
    Update(CategoryUpdateArgs),
    Delete(CategoryDeleteArgs),
}

#[derive(Debug, Clone, Args)]
#[group(id = "category_selector", multiple = false)]
pub struct OptionalCategorySelectorArgs {
    #[arg(long, value_name = "UUID", group = "category_selector")]
    pub category_id: Option<Uuid>,

    #[arg(long, value_name = "SLUG", group = "category_selector")]
    pub category_slug: Option<String>,
}

#[derive(Debug, Clone, Args)]
#[group(id = "category_selector", required = true, multiple = false)]
pub struct RequiredCategorySelectorArgs {
    #[arg(long, value_name = "UUID", group = "category_selector")]
    pub category_id: Option<Uuid>,

    #[arg(long, value_name = "SLUG", group = "category_selector")]
    pub category_slug: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct TaskListArgs {
    #[command(flatten)]
    pub selector: OptionalCategorySelectorArgs,

    #[arg(long)]
    pub archived: bool,

    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct TaskCreateArgs {
    #[arg(long, value_name = "TEXT")]
    pub title: String,

    #[arg(long, value_name = "BRANCH")]
    pub branch: String,

    #[arg(long, value_name = "REPO")]
    pub repo: Option<String>,

    #[command(flatten)]
    pub selector: OptionalCategorySelectorArgs,
}

#[derive(Debug, Clone, Args)]
pub struct TaskMoveArgs {
    #[arg(long, value_name = "TASK_ID")]
    pub id: String,

    #[command(flatten)]
    pub selector: RequiredCategorySelectorArgs,
}

#[derive(Debug, Clone, Args)]
pub struct TaskArchiveArgs {
    #[arg(long, value_name = "TASK_ID")]
    pub id: String,
}

#[derive(Debug, Clone, Args)]
pub struct TaskShowArgs {
    #[arg(long, value_name = "TASK_ID")]
    pub id: String,
}

#[derive(Debug, Clone, Args)]
pub struct CategoryCreateArgs {
    #[arg(long, value_name = "TEXT")]
    pub name: String,

    #[arg(long, value_name = "SLUG")]
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Args)]
pub struct CategoryUpdateArgs {
    #[arg(long, value_name = "CATEGORY_ID")]
    pub id: Uuid,

    #[arg(long, value_name = "TEXT")]
    pub name: Option<String>,

    #[arg(long, value_name = "SLUG")]
    pub slug: Option<String>,

    #[arg(long, value_name = "N")]
    pub position: Option<i64>,
}

#[derive(Debug, Clone, Args)]
pub struct CategoryDeleteArgs {
    #[arg(long, value_name = "CATEGORY_ID")]
    pub id: Uuid,
}

pub fn run(project_name: &str, command: RootCommand, json_output: bool, quiet: bool) -> i32 {
    match execute(project_name, command) {
        Ok(output) => {
            print_success(output, json_output, quiet);
            0
        }
        Err(err) => {
            print_error(&err, json_output);
            err.exit_code
        }
    }
}

struct CommandOutput {
    command: &'static str,
    project: String,
    data: Value,
    text: String,
}

#[derive(Debug)]
struct CliError {
    exit_code: i32,
    code: &'static str,
    message: String,
    details: Option<Value>,
}

type CliResult<T> = Result<T, CliError>;

fn execute(project_name: &str, command: RootCommand) -> CliResult<CommandOutput> {
    let project = project_name.to_string();
    let db_path = resolve_existing_project_db_path(&project)?;
    let db = Database::open(&db_path).map_err(runtime_error)?;

    match command {
        RootCommand::Task { command } => execute_task_command(&db, &project, command),
        RootCommand::Category { command } => execute_category_command(&db, &project, command),
    }
}

fn resolve_existing_project_db_path(project: &str) -> CliResult<PathBuf> {
    let db_path = projects::get_project_path(project);
    if !db_path.exists() {
        return Err(not_found_error(
            "PROJECT_NOT_FOUND",
            format!("project '{}' not found", project),
        ));
    }
    Ok(db_path)
}

fn execute_task_command(
    db: &Database,
    project: &str,
    command: TaskCommand,
) -> CliResult<CommandOutput> {
    match command {
        TaskCommand::List(args) => task_list(db, project, args),
        TaskCommand::Create(args) => task_create(db, project, args),
        TaskCommand::Move(args) => task_move(db, project, args),
        TaskCommand::Archive(args) => task_archive(db, project, args),
        TaskCommand::Show(args) => task_show(db, project, args),
    }
}

fn execute_category_command(
    db: &Database,
    project: &str,
    command: CategoryCommand,
) -> CliResult<CommandOutput> {
    match command {
        CategoryCommand::List => category_list(db, project),
        CategoryCommand::Create(args) => category_create(db, project, args),
        CategoryCommand::Update(args) => category_update(db, project, args),
        CategoryCommand::Delete(args) => category_delete(db, project, args),
    }
}

fn category_list(db: &Database, project: &str) -> CliResult<CommandOutput> {
    let categories = db.list_categories().map_err(runtime_error)?;
    let data = json!({
        "categories": categories.iter().map(category_json).collect::<Vec<_>>()
    });
    let text = render_category_list_text(&categories);

    Ok(CommandOutput {
        command: "category list",
        project: project.to_string(),
        data,
        text,
    })
}

fn render_category_list_text(categories: &[Category]) -> String {
    if categories.is_empty() {
        return "No categories found.".to_string();
    }

    let headers = ["ID", "Slug", "Name", "Pos", "Color"];
    let rows = categories
        .iter()
        .map(|category| {
            let id = category.id.to_string();
            let short_id = id.chars().take(8).collect::<String>();
            let name = category.name.replace('\n', " ");
            let color = category.color.clone().unwrap_or_else(|| "-".to_string());

            vec![
                short_id,
                category.slug.clone(),
                name,
                category.position.to_string(),
                color,
            ]
        })
        .collect::<Vec<_>>();

    render_text_table(&headers, &rows)
}

fn category_create(
    db: &Database,
    project: &str,
    args: CategoryCreateArgs,
) -> CliResult<CommandOutput> {
    let position = db
        .list_categories()
        .map_err(runtime_error)?
        .into_iter()
        .map(|category| category.position)
        .max()
        .unwrap_or(-1)
        + 1;

    let created = db
        .add_category_with_slug(&args.name, args.slug.as_deref(), position, None)
        .map_err(classify_db_error)?;
    let data = json!({ "category": category_json(&created) });

    Ok(CommandOutput {
        command: "category create",
        project: project.to_string(),
        data,
        text: format!("created category {} ({})", created.slug, created.id),
    })
}

fn category_update(
    db: &Database,
    project: &str,
    args: CategoryUpdateArgs,
) -> CliResult<CommandOutput> {
    if args.name.is_none() && args.slug.is_none() && args.position.is_none() {
        return Err(usage_error(
            "CATEGORY_UPDATE_EMPTY",
            "provide at least one of --name, --slug, or --position",
        ));
    }

    let categories = db.list_categories().map_err(runtime_error)?;
    if !categories.iter().any(|category| category.id == args.id) {
        return Err(not_found_error(
            "CATEGORY_NOT_FOUND",
            format!("category {} not found", args.id),
        ));
    }

    if let Some(name) = args.name.as_deref() {
        db.rename_category(args.id, name)
            .map_err(classify_db_error)?;
    }
    if let Some(slug) = args.slug.as_deref() {
        db.update_category_slug(args.id, slug)
            .map_err(classify_db_error)?;
    }
    if let Some(position) = args.position {
        db.update_category_position(args.id, position)
            .map_err(classify_db_error)?;
    }

    let updated = db
        .list_categories()
        .map_err(runtime_error)?
        .into_iter()
        .find(|category| category.id == args.id)
        .ok_or_else(|| {
            runtime_error(anyhow::anyhow!(
                "category {} disappeared after update",
                args.id
            ))
        })?;

    let data = json!({ "category": category_json(&updated) });
    Ok(CommandOutput {
        command: "category update",
        project: project.to_string(),
        data,
        text: format!("updated category {} ({})", updated.slug, updated.id),
    })
}

fn category_delete(
    db: &Database,
    project: &str,
    args: CategoryDeleteArgs,
) -> CliResult<CommandOutput> {
    db.delete_category(args.id).map_err(classify_db_error)?;
    let data = json!({ "deleted": true, "category_id": args.id });

    Ok(CommandOutput {
        command: "category delete",
        project: project.to_string(),
        data,
        text: format!("deleted category {}", args.id),
    })
}

fn task_list(db: &Database, project: &str, args: TaskListArgs) -> CliResult<CommandOutput> {
    let categories = db.list_categories().map_err(runtime_error)?;
    let category_by_id: HashMap<Uuid, Category> = categories
        .into_iter()
        .map(|category| (category.id, category))
        .collect();

    let repos = db.list_repos().map_err(runtime_error)?;
    let repo_by_id: HashMap<Uuid, Repo> = repos.into_iter().map(|repo| (repo.id, repo)).collect();
    let repo_filter_id = resolve_repo_filter_id(&repo_by_id, args.repo.as_deref())?;
    let category_filter_id = resolve_optional_category_selector(
        db,
        args.selector.category_id,
        args.selector.category_slug.as_deref(),
    )?;

    let tasks = if args.archived {
        db.list_archived_tasks().map_err(runtime_error)?
    } else {
        db.list_tasks().map_err(runtime_error)?
    };

    let filtered: Vec<Task> = tasks
        .into_iter()
        .filter(|task| {
            repo_filter_id.is_none_or(|repo_id| task.repo_id == repo_id)
                && category_filter_id.is_none_or(|category_id| task.category_id == category_id)
        })
        .collect();

    let data = json!({
        "tasks": filtered
            .iter()
            .map(|task| task_json(task, &category_by_id, &repo_by_id))
            .collect::<Vec<_>>()
    });

    let text = render_task_list_text(&filtered, &category_by_id, &repo_by_id);

    Ok(CommandOutput {
        command: "task list",
        project: project.to_string(),
        data,
        text,
    })
}

fn render_task_list_text(
    tasks: &[Task],
    category_by_id: &HashMap<Uuid, Category>,
    repo_by_id: &HashMap<Uuid, Repo>,
) -> String {
    if tasks.is_empty() {
        return "No tasks found.".to_string();
    }

    let headers = ["ID", "Category", "Repo:Branch", "Title"];
    let rows = tasks
        .iter()
        .map(|task| {
            let category_label = category_by_id
                .get(&task.category_id)
                .map(|category| category.slug.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let repo_name = repo_by_id
                .get(&task.repo_id)
                .map(|repo| repo.name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let id = task.id.to_string();
            let short_id = id.chars().take(8).collect::<String>();
            let repo_branch = format!("{}:{}", repo_name, task.branch);
            let title = task.title.replace('\n', " ");

            vec![short_id, category_label, repo_branch, title]
        })
        .collect::<Vec<_>>();

    render_text_table(&headers, &rows)
}

fn render_text_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths = headers
        .iter()
        .map(|header| header.chars().count())
        .collect::<Vec<_>>();

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            let width = cell.chars().count();
            if width > widths[index] {
                widths[index] = width;
            }
        }
    }

    let border = format!(
        "+{}+",
        widths
            .iter()
            .map(|width| "-".repeat(*width + 2))
            .collect::<Vec<_>>()
            .join("+")
    );

    let mut lines = Vec::new();
    lines.push(border.clone());
    lines.push(format!(
        "| {} |",
        headers
            .iter()
            .enumerate()
            .map(|(index, header)| format!("{header:<width$}", width = widths[index]))
            .collect::<Vec<_>>()
            .join(" | ")
    ));
    lines.push(border.clone());

    for row in rows {
        lines.push(format!(
            "| {} |",
            row.iter()
                .enumerate()
                .map(|(index, cell)| format!("{cell:<width$}", width = widths[index]))
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }

    lines.push(border);
    lines.join("\n")
}

fn task_create(db: &Database, project: &str, args: TaskCreateArgs) -> CliResult<CommandOutput> {
    let repos = db.list_repos().map_err(runtime_error)?;
    let repo = resolve_repo_for_create(&repos, args.repo.as_deref())?;

    let category_id = match resolve_optional_category_selector(
        db,
        args.selector.category_id,
        args.selector.category_slug.as_deref(),
    )? {
        Some(value) => value,
        None => resolve_default_category_id(db)?,
    };

    let branch = args.branch.trim();
    if branch.is_empty() {
        return Err(usage_error("BRANCH_REQUIRED", "branch cannot be empty"));
    }

    let runtime = RealCreateTaskRuntime;
    let repo_path = PathBuf::from(&repo.path);

    CreateTaskRuntime::git_validate_branch(&runtime, &repo_path, branch)
        .context("branch validation failed")
        .map_err(classify_db_error)?;

    let base_ref = repo
        .default_base
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| CreateTaskRuntime::git_detect_default_branch(&runtime, &repo_path));

    if let Err(err) = CreateTaskRuntime::git_fetch(&runtime, &repo_path) {
        warn!(
            repo = %repo.path,
            error = %err,
            "fetch from origin failed, continuing offline"
        );
    }

    CreateTaskRuntime::git_check_branch_up_to_date(&runtime, &repo_path, &base_ref)
        .context("base branch check failed")
        .map_err(classify_db_error)?;

    let worktrees_root = worktrees_root_for_repo(&repo_path);
    fs::create_dir_all(&worktrees_root).map_err(runtime_error)?;
    let worktree_path = derive_worktree_path(&worktrees_root, &repo_path, branch);

    CreateTaskRuntime::git_create_worktree(&runtime, &repo_path, &worktree_path, branch, &base_ref)
        .context("worktree creation failed")
        .map_err(classify_db_error)?;

    let project_slug = if project == projects::DEFAULT_PROJECT {
        None
    } else {
        Some(project)
    };
    let session_name =
        next_available_session_name_by(None, project_slug, &repo.name, branch, |name| {
            CreateTaskRuntime::tmux_session_exists(&runtime, name)
        });

    let worktree_path_string = worktree_path.display().to_string();
    let command = opencode_attach_command(None, Some(&worktree_path_string));

    let mut created_task_id: Option<Uuid> = None;
    let mut tmux_created = false;
    let create_result = (|| -> anyhow::Result<Task> {
        CreateTaskRuntime::tmux_create_session(
            &runtime,
            &session_name,
            &worktree_path,
            Some(&command),
        )
        .context("tmux session creation failed")?;
        tmux_created = true;

        let task = db
            .add_task(repo.id, branch, &args.title, category_id)
            .context("failed to save task")?;
        created_task_id = Some(task.id);

        db.update_task_tmux(
            task.id,
            Some(session_name.clone()),
            Some(worktree_path.display().to_string()),
        )
        .context("failed to save task runtime metadata")?;
        db.update_task_status(task.id, Status::Idle.as_str())
            .context("failed to save task runtime status")?;

        Ok(task)
    })();

    let created = match create_result {
        Ok(task) => task,
        Err(err) => {
            if let Some(task_id) = created_task_id {
                let _ = db.delete_task(task_id);
            }
            if tmux_created {
                let _ = CreateTaskRuntime::tmux_kill_session(&runtime, &session_name);
            }
            let _ = CreateTaskRuntime::git_remove_worktree(&runtime, &repo_path, &worktree_path);
            return Err(classify_db_error(err));
        }
    };

    let created = db
        .get_task(created.id)
        .map_err(|err| runtime_error(anyhow::anyhow!(err.to_string())))?;

    let categories = db.list_categories().map_err(runtime_error)?;
    let category_by_id: HashMap<Uuid, Category> = categories
        .into_iter()
        .map(|category| (category.id, category))
        .collect();
    let repo_by_id = HashMap::from([(repo.id, repo.clone())]);

    let data = json!({ "task": task_json(&created, &category_by_id, &repo_by_id) });
    Ok(CommandOutput {
        command: "task create",
        project: project.to_string(),
        data,
        text: format!("created task {} ({})", created.title, created.id),
    })
}

fn task_move(db: &Database, project: &str, args: TaskMoveArgs) -> CliResult<CommandOutput> {
    let task_id = resolve_task_id_selector(db, &args.id)?;
    let target_category_id = resolve_required_category_selector(
        db,
        args.selector.category_id,
        args.selector.category_slug.as_deref(),
    )?;

    let task = db
        .get_task(task_id)
        .map_err(|err| task_lookup_error(task_id, err.to_string()))?;
    db.update_task_category(task.id, target_category_id, 0)
        .map_err(classify_db_error)?;

    let updated = db
        .get_task(task.id)
        .map_err(|err| runtime_error(anyhow::anyhow!(err.to_string())))?;
    let categories = db.list_categories().map_err(runtime_error)?;
    let category_by_id: HashMap<Uuid, Category> = categories
        .into_iter()
        .map(|category| (category.id, category))
        .collect();
    let repos = db.list_repos().map_err(runtime_error)?;
    let repo_by_id: HashMap<Uuid, Repo> = repos.into_iter().map(|repo| (repo.id, repo)).collect();

    let data = json!({ "task": task_json(&updated, &category_by_id, &repo_by_id) });
    Ok(CommandOutput {
        command: "task move",
        project: project.to_string(),
        data,
        text: format!("moved task {} to {}", updated.id, updated.category_id),
    })
}

fn task_archive(db: &Database, project: &str, args: TaskArchiveArgs) -> CliResult<CommandOutput> {
    let task_id = resolve_task_id_selector(db, &args.id)?;
    let existing = db
        .get_task(task_id)
        .map_err(|err| task_lookup_error(task_id, err.to_string()))?;

    if !existing.archived {
        db.archive_task(task_id).map_err(classify_db_error)?;
    }

    let archived = db
        .get_task(task_id)
        .map_err(|err| runtime_error(anyhow::anyhow!(err.to_string())))?;
    let categories = db.list_categories().map_err(runtime_error)?;
    let category_by_id: HashMap<Uuid, Category> = categories
        .into_iter()
        .map(|category| (category.id, category))
        .collect();
    let repos = db.list_repos().map_err(runtime_error)?;
    let repo_by_id: HashMap<Uuid, Repo> = repos.into_iter().map(|repo| (repo.id, repo)).collect();

    let data = json!({ "task": task_json(&archived, &category_by_id, &repo_by_id) });
    Ok(CommandOutput {
        command: "task archive",
        project: project.to_string(),
        data,
        text: format!("archived task {}", archived.id),
    })
}

fn task_show(db: &Database, project: &str, args: TaskShowArgs) -> CliResult<CommandOutput> {
    let task_id = resolve_task_id_selector(db, &args.id)?;
    let task = db
        .get_task(task_id)
        .map_err(|err| task_lookup_error(task_id, err.to_string()))?;
    let categories = db.list_categories().map_err(runtime_error)?;
    let category_by_id: HashMap<Uuid, Category> = categories
        .into_iter()
        .map(|category| (category.id, category))
        .collect();
    let repos = db.list_repos().map_err(runtime_error)?;
    let repo_by_id: HashMap<Uuid, Repo> = repos.into_iter().map(|repo| (repo.id, repo)).collect();

    let data = json!({ "task": task_json(&task, &category_by_id, &repo_by_id) });
    Ok(CommandOutput {
        command: "task show",
        project: project.to_string(),
        data,
        text: format!("{} {}", task.id, task.title),
    })
}

fn resolve_repo_filter_id(
    repo_by_id: &HashMap<Uuid, Repo>,
    name: Option<&str>,
) -> CliResult<Option<Uuid>> {
    let Some(repo_selector) = name else {
        return Ok(None);
    };

    repo_by_id
        .iter()
        .find(|(_, repo)| repo_matches_selector(repo, repo_selector))
        .map(|(id, _)| Some(*id))
        .ok_or_else(|| {
            not_found_error(
                "REPO_NOT_FOUND",
                format!("repo '{}' not found", repo_selector),
            )
        })
}

fn resolve_repo_for_create<'a>(repos: &'a [Repo], name: Option<&str>) -> CliResult<&'a Repo> {
    if let Some(repo_selector) = name {
        return repos
            .iter()
            .find(|repo| repo_matches_selector(repo, repo_selector))
            .ok_or_else(|| {
                not_found_error(
                    "REPO_NOT_FOUND",
                    format!("repo '{}' not found", repo_selector),
                )
            });
    }

    match repos {
        [only] => Ok(only),
        [] => Err(not_found_error(
            "REPO_NOT_FOUND",
            "no repositories are configured; provide --repo after adding one".to_string(),
        )),
        _ => Err(conflict_error(
            "REPO_SELECTOR_REQUIRED",
            "multiple repositories found; provide --repo".to_string(),
            None,
        )),
    }
}

fn repo_matches_selector(repo: &Repo, selector: &str) -> bool {
    if repo.name == selector || repo.path == selector {
        return true;
    }

    let repo_path = canonical_path_best_effort(&repo.path);
    let selector_path = canonical_path_best_effort(selector);
    repo_path.is_some() && repo_path == selector_path
}

fn canonical_path_best_effort(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    std::fs::canonicalize(Path::new(trimmed)).ok()
}

fn resolve_optional_category_selector(
    db: &Database,
    category_id: Option<Uuid>,
    category_slug: Option<&str>,
) -> CliResult<Option<Uuid>> {
    match (category_id, category_slug) {
        (Some(id), None) => {
            let categories = db.list_categories().map_err(runtime_error)?;
            if categories.iter().any(|category| category.id == id) {
                Ok(Some(id))
            } else {
                Err(not_found_error(
                    "CATEGORY_NOT_FOUND",
                    format!("category {} not found", id),
                ))
            }
        }
        (None, Some(slug)) => {
            let category = db
                .get_category_by_slug(slug)
                .map_err(runtime_error)?
                .ok_or_else(|| {
                    not_found_error(
                        "CATEGORY_NOT_FOUND",
                        format!("category '{}' not found", slug),
                    )
                })?;
            Ok(Some(category.id))
        }
        (None, None) => Ok(None),
        (Some(_), Some(_)) => Err(conflict_error(
            "CATEGORY_SELECTOR_CONFLICT",
            "provide exactly one of category_id or category_slug".to_string(),
            None,
        )),
    }
}

fn resolve_required_category_selector(
    db: &Database,
    category_id: Option<Uuid>,
    category_slug: Option<&str>,
) -> CliResult<Uuid> {
    resolve_optional_category_selector(db, category_id, category_slug)?.ok_or_else(|| {
        usage_error(
            "CATEGORY_SELECTOR_REQUIRED",
            "provide one of --category-id or --category-slug",
        )
    })
}

fn resolve_task_id_selector(db: &Database, selector: &str) -> CliResult<Uuid> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Err(usage_error("TASK_ID_REQUIRED", "task id cannot be empty"));
    }

    if let Ok(parsed) = Uuid::parse_str(trimmed) {
        return Ok(parsed);
    }

    let needle = trimmed.to_ascii_lowercase();
    let mut tasks = db.list_tasks().map_err(runtime_error)?;
    let mut archived = db.list_archived_tasks().map_err(runtime_error)?;
    tasks.append(&mut archived);

    let mut unique_matches = Vec::new();
    let mut seen = HashSet::new();
    for task in tasks {
        let full = task.id.to_string().to_ascii_lowercase();
        let simple = task.id.as_simple().to_string();
        if (full.starts_with(&needle) || simple.starts_with(&needle)) && seen.insert(task.id) {
            unique_matches.push(task.id);
        }
    }

    match unique_matches.as_slice() {
        [single] => Ok(*single),
        [] => Err(not_found_error(
            "TASK_NOT_FOUND",
            format!("task '{}' not found", selector),
        )),
        many => Err(conflict_error(
            "TASK_ID_AMBIGUOUS",
            format!(
                "task id prefix '{}' matches {} tasks; use a longer id",
                selector,
                many.len()
            ),
            Some(json!({
                "matches": many.iter().map(|id| id.to_string()).collect::<Vec<_>>()
            })),
        )),
    }
}

fn resolve_default_category_id(db: &Database) -> CliResult<Uuid> {
    let categories = db.list_categories().map_err(runtime_error)?;
    categories
        .iter()
        .find(|category| category.slug == "todo")
        .or_else(|| categories.first())
        .map(|category| category.id)
        .ok_or_else(|| runtime_error(anyhow::anyhow!("no category available for task creation")))
}

fn task_json(
    task: &Task,
    categories: &HashMap<Uuid, Category>,
    repos: &HashMap<Uuid, Repo>,
) -> Value {
    let category = categories.get(&task.category_id);
    let repo = repos.get(&task.repo_id);

    json!({
        "id": task.id,
        "title": task.title,
        "repo_id": task.repo_id,
        "repo_name": repo.map(|value| value.name.clone()),
        "branch": task.branch,
        "category_id": task.category_id,
        "category": category.map(category_json),
        "position": task.position,
        "archived": task.archived,
        "archived_at": task.archived_at,
        "tmux_session_name": task.tmux_session_name,
        "worktree_path": task.worktree_path,
        "tmux_status": task.tmux_status,
        "status_source": task.status_source,
        "status_fetched_at": task.status_fetched_at,
        "status_error": task.status_error,
        "opencode_session_id": task.opencode_session_id,
        "created_at": task.created_at,
        "updated_at": task.updated_at
    })
}

fn category_json(category: &Category) -> Value {
    json!({
        "id": category.id,
        "slug": category.slug,
        "name": category.name,
        "position": category.position,
        "color": category.color,
        "created_at": category.created_at
    })
}

fn usage_error(code: &'static str, message: impl Into<String>) -> CliError {
    CliError {
        exit_code: 2,
        code,
        message: message.into(),
        details: None,
    }
}

fn not_found_error(code: &'static str, message: impl Into<String>) -> CliError {
    CliError {
        exit_code: 3,
        code,
        message: message.into(),
        details: None,
    }
}

fn conflict_error(
    code: &'static str,
    message: impl Into<String>,
    details: Option<Value>,
) -> CliError {
    CliError {
        exit_code: 4,
        code,
        message: message.into(),
        details,
    }
}

fn runtime_error(err: impl std::fmt::Display) -> CliError {
    CliError {
        exit_code: 5,
        code: "RUNTIME_ERROR",
        message: err.to_string(),
        details: None,
    }
}

fn task_lookup_error(task_id: Uuid, message: String) -> CliError {
    if message.contains("not found") {
        return not_found_error("TASK_NOT_FOUND", format!("task {} not found", task_id));
    }
    runtime_error(message)
}

fn classify_db_error(err: anyhow::Error) -> CliError {
    let top_message = err.to_string();

    if let Some(detail) = find_constraint_detail(&err, "UNIQUE constraint failed") {
        let message = if top_message.contains(&detail) {
            top_message
        } else {
            format!("{top_message}: {detail}")
        };
        return conflict_error("UNIQUE_CONSTRAINT", message, None);
    }

    if let Some(detail) = find_constraint_detail(&err, "FOREIGN KEY constraint failed") {
        let message = if top_message.contains(&detail) {
            top_message
        } else {
            format!("{top_message}: {detail}")
        };
        return conflict_error("FOREIGN_KEY_CONSTRAINT", message, None);
    }

    let message = format_anyhow_error_chain(&err);
    runtime_error(message)
}

fn print_success(output: CommandOutput, json_output: bool, quiet: bool) {
    if json_output {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "command": output.command,
            "project": output.project,
            "data": output.data
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(value) => println!("{value}"),
            Err(_) => println!("{}", payload),
        }
        return;
    }

    if quiet {
        return;
    }

    if output.text.is_empty() {
        println!("ok");
    } else {
        println!("{}", output.text);
    }
}

fn print_error(err: &CliError, json_output: bool) {
    error!(
        code = err.code,
        message = %err.message,
        details = ?err.details,
        "cli command failed"
    );

    if json_output {
        let payload = json!({
            "schema_version": SCHEMA_VERSION,
            "error": {
                "code": err.code,
                "message": err.message,
                "details": err.details
            }
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(value) => eprintln!("{value}"),
            Err(_) => eprintln!("{}", payload),
        }
        return;
    }

    eprintln!("error[{}]: {}", err.code, err.message);
}

fn format_anyhow_error_chain(err: &anyhow::Error) -> String {
    let mut seen = HashSet::new();
    let mut parts = Vec::new();
    for cause in err.chain() {
        let text = cause.to_string();
        if seen.contains(&text) {
            continue;
        }
        seen.insert(text.clone());
        parts.push(text);
    }

    parts.join(": ")
}

fn find_constraint_detail(err: &anyhow::Error, needle: &str) -> Option<String> {
    let mut best: Option<String> = None;
    for cause in err.chain() {
        let message = cause.to_string();
        if !message.contains(needle) {
            continue;
        }

        best = match best {
            Some(existing) if existing.len() <= message.len() => Some(existing),
            _ => Some(message),
        };
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::TempDir;

    fn fake_repo(name: &str, path: &str) -> Repo {
        Repo {
            id: Uuid::new_v4(),
            path: path.to_string(),
            name: name.to_string(),
            default_base: None,
            remote_url: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn resolve_selector_by_slug_returns_category_id() {
        let db = Database::open(":memory:").expect("db should open");
        let todo = db
            .get_category_by_slug("todo")
            .expect("lookup should succeed")
            .expect("todo category should exist");

        let resolved = resolve_optional_category_selector(&db, None, Some("todo"))
            .expect("selector should resolve")
            .expect("selector should return category id");

        assert_eq!(resolved, todo.id);
    }

    #[test]
    fn resolve_selector_with_conflicting_inputs_returns_conflict() {
        let db = Database::open(":memory:").expect("db should open");
        let todo = db
            .get_category_by_slug("todo")
            .expect("lookup should succeed")
            .expect("todo category should exist");

        let err = resolve_optional_category_selector(&db, Some(todo.id), Some("todo"))
            .expect_err("conflicting selector should fail");

        assert_eq!(err.exit_code, 4);
        assert_eq!(err.code, "CATEGORY_SELECTOR_CONFLICT");
    }

    #[test]
    fn repo_selector_matches_repo_name() {
        let repo = fake_repo("test", "/tmp/test");
        assert!(repo_matches_selector(&repo, "test"));
    }

    #[test]
    fn repo_selector_matches_repo_path() {
        let repo = fake_repo("test", "/tmp/test");
        assert!(repo_matches_selector(&repo, "/tmp/test"));
    }

    #[test]
    fn format_anyhow_error_chain_includes_context_and_root_cause() {
        let err = anyhow::anyhow!("UNIQUE constraint failed: tasks.repo_id, tasks.branch")
            .context("failed to insert task");
        let message = format_anyhow_error_chain(&err);

        assert!(message.contains("failed to insert task"));
        assert!(message.contains("UNIQUE constraint failed: tasks.repo_id, tasks.branch"));
    }

    #[test]
    fn classify_db_error_uses_compact_unique_constraint_message() {
        let err = anyhow::anyhow!(
            "error returned from database: (code: 2067) UNIQUE constraint failed: tasks.repo_id, tasks.branch"
        )
        .context("failed to insert task");

        let classified = classify_db_error(err);
        assert_eq!(classified.code, "UNIQUE_CONSTRAINT");
        assert_eq!(classified.exit_code, 4);
        assert_eq!(
            classified.message,
            "failed to insert task: error returned from database: (code: 2067) UNIQUE constraint failed: tasks.repo_id, tasks.branch"
        );
    }

    #[test]
    fn task_list_text_renders_table_with_repo_branch_column() {
        let category_id = Uuid::new_v4();
        let repo_id = Uuid::new_v4();

        let category = Category {
            id: category_id,
            slug: "todo".to_string(),
            name: "TODO".to_string(),
            position: 0,
            color: None,
            created_at: "now".to_string(),
        };

        let repo = Repo {
            id: repo_id,
            path: "/tmp/test-repo".to_string(),
            name: "test-repo".to_string(),
            default_base: None,
            remote_url: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };

        let task = Task {
            id: Uuid::new_v4(),
            title: "Ship table output".to_string(),
            repo_id,
            branch: "feature/table-output".to_string(),
            category_id,
            position: 0,
            tmux_session_name: None,
            worktree_path: None,
            tmux_status: "unknown".to_string(),
            status_source: "none".to_string(),
            status_fetched_at: None,
            status_error: None,
            opencode_session_id: None,
            archived: false,
            archived_at: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };

        let category_by_id = HashMap::from([(category_id, category)]);
        let repo_by_id = HashMap::from([(repo_id, repo)]);
        let output = render_task_list_text(&[task], &category_by_id, &repo_by_id);

        assert!(output.contains("Repo:Branch"));
        assert!(output.contains("test-repo:feature/table-output"));
        assert!(output.contains("|"));
    }

    #[test]
    fn category_list_text_renders_table() {
        let categories = vec![
            Category {
                id: Uuid::new_v4(),
                slug: "todo".to_string(),
                name: "TODO".to_string(),
                position: 0,
                color: None,
                created_at: "now".to_string(),
            },
            Category {
                id: Uuid::new_v4(),
                slug: "review".to_string(),
                name: "Review".to_string(),
                position: 1,
                color: Some("blue".to_string()),
                created_at: "now".to_string(),
            },
        ];

        let output = render_category_list_text(&categories);
        assert!(output.contains("| ID"));
        assert!(output.contains("Slug"));
        assert!(output.contains("review"));
        assert!(output.contains("blue"));
    }

    #[test]
    fn resolve_task_id_selector_accepts_short_prefix() {
        let repo_dir = TempDir::new().expect("temp repo dir");
        let db = Database::open(":memory:").expect("db should open");
        let repo = db.add_repo(repo_dir.path()).expect("repo should save");
        let category = db
            .get_category_by_slug("todo")
            .expect("lookup should succeed")
            .expect("todo category should exist");

        let task = db
            .add_task(repo.id, "feature/short-id", "short id task", category.id)
            .expect("task should save");
        let short = task.id.to_string().chars().take(8).collect::<String>();

        let resolved = resolve_task_id_selector(&db, &short).expect("short id should resolve");
        assert_eq!(resolved, task.id);
    }
}
