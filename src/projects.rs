use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::db::Database;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectInfo {
    pub name: String,
    pub path: PathBuf,
}

pub const DEFAULT_PROJECT: &str = "opencode-kanban";

pub fn get_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opencode-kanban")
}

pub fn get_project_path(name: &str) -> PathBuf {
    let sanitized = sanitize_project_name(name);
    get_data_dir().join(format!("{}.sqlite", sanitized))
}

fn sanitize_project_name(name: &str) -> String {
    if name == DEFAULT_PROJECT {
        return name.to_string();
    }

    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();

    sanitized.trim_matches(|c| c == '-' || c == '_').to_string()
}

fn validate_project_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("project name cannot be empty");
    }

    if name.contains('/') || name.contains('\\') {
        bail!("project name cannot contain path separators");
    }

    if name == "." || name == ".." {
        bail!("project name cannot be . or ..");
    }

    let sanitized = sanitize_project_name(name);
    if sanitized.is_empty() {
        bail!("project name contains only invalid characters");
    }

    Ok(())
}

pub fn list_projects() -> Result<Vec<ProjectInfo>> {
    let data_dir = get_data_dir();

    if !data_dir.exists() {
        return Ok(vec![]);
    }

    let mut projects = Vec::new();

    for entry in fs::read_dir(&data_dir).context("failed to read data directory")? {
        let entry = entry.context("failed to read directory entry")?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("sqlite")
            && let Some(name) = path.file_stem().and_then(|s| s.to_str())
        {
            projects.push(ProjectInfo {
                name: name.to_string(),
                path,
            });
        }
    }

    projects.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(projects)
}

pub fn create_project(name: &str) -> Result<PathBuf> {
    validate_project_name(name)?;

    let sanitized = sanitize_project_name(name);
    let path = get_data_dir().join(format!("{}.sqlite", sanitized));

    if path.exists() {
        bail!("project '{}' already exists", sanitized);
    }

    let _db = Database::open(&path).context("failed to create project database")?;

    Ok(path)
}

pub fn rename_project(old_path: &Path, new_name: &str) -> Result<PathBuf> {
    validate_project_name(new_name)?;

    let sanitized = sanitize_project_name(new_name);
    let new_path = get_data_dir().join(format!("{}.sqlite", sanitized));

    if new_path.exists() {
        bail!("project '{}' already exists", sanitized);
    }

    fs::rename(old_path, &new_path).with_context(|| {
        format!(
            "failed to rename project '{}' to '{}'",
            old_path.display(),
            new_path.display()
        )
    })?;

    Ok(new_path)
}

pub fn delete_project(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("project file does not exist: {}", path.display());
    }

    fs::remove_file(path)
        .with_context(|| format!("failed to delete project file '{}'", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_project_name() {
        assert_eq!(sanitize_project_name("my-project"), "my-project");
        assert_eq!(sanitize_project_name("my_project"), "my_project");
        assert_eq!(sanitize_project_name("my.project"), "my_project");
        assert_eq!(sanitize_project_name("my@project#1"), "my_project_1");
        assert_eq!(sanitize_project_name("  my-project  "), "my-project");
        assert_eq!(sanitize_project_name("-my-project-"), "my-project");
        assert_eq!(sanitize_project_name(DEFAULT_PROJECT), DEFAULT_PROJECT);
    }

    #[test]
    fn test_validate_project_name_valid() {
        assert!(validate_project_name("my-project").is_ok());
        assert!(validate_project_name("my_project").is_ok());
        assert!(validate_project_name("project123").is_ok());
    }

    #[test]
    fn test_validate_project_name_invalid() {
        assert!(validate_project_name("").is_err());
        assert!(validate_project_name("my/project").is_err());
        assert!(validate_project_name("my\\project").is_err());
        assert!(validate_project_name("..").is_err());
        assert!(validate_project_name(".").is_err());
        assert!(validate_project_name("@@@").is_err());
    }

    #[test]
    fn test_get_project_path() {
        let path = get_project_path("my-project");
        assert!(path.to_string_lossy().contains("my-project.sqlite"));
    }

    #[test]
    fn test_list_projects_empty_dir() {
        let projects = list_projects();
        assert!(projects.is_ok());
    }

    #[test]
    fn test_create_project_in_memory() {
        let db = Database::open(":memory:").unwrap();
        let categories = db.list_categories().unwrap();
        assert_eq!(categories.len(), 3);
    }
}
