use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub is_remote: bool,
}

pub fn git_detect_default_branch(repo_path: &Path) -> String {
    if let Ok(output) = run_git_output(repo_path, ["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if let Some(branch) = value.strip_prefix("refs/remotes/origin/")
            && !branch.is_empty()
        {
            return branch.to_string();
        }
    }

    for candidate in ["main", "master"] {
        if branch_exists(repo_path, candidate) {
            return candidate.to_string();
        }
    }

    if let Some(first_branch) = git_list_branches(repo_path)
        .into_iter()
        .map(|branch| {
            if branch.is_remote {
                branch
                    .name
                    .split_once('/')
                    .map(|(_, name)| name.to_string())
                    .unwrap_or(branch.name)
            } else {
                branch.name
            }
        })
        .find(|name| !name.is_empty() && name != "HEAD")
    {
        return first_branch;
    }

    "main".to_string()
}

pub fn git_fetch(repo_path: &Path) -> Result<()> {
    run_git(repo_path, ["fetch", "origin"]).context("failed to fetch from origin")
}

pub fn git_list_branches(repo_path: &Path) -> Vec<Branch> {
    let output = match run_git_output(
        repo_path,
        ["branch", "-a", "--format=%(refname:short)|%(refname)"],
    ) {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (name, full_ref) = line.split_once('|')?;
            let name = name.trim();
            let full_ref = full_ref.trim();
            if name.is_empty() || name.contains(" -> ") {
                return None;
            }

            Some(Branch {
                name: name.to_string(),
                is_remote: full_ref.starts_with("refs/remotes/"),
            })
        })
        .collect()
}

pub fn git_list_tags(repo_path: &Path) -> Vec<String> {
    let output = match run_git_output(repo_path, ["tag", "-l"]) {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub fn git_create_worktree(
    repo_path: &Path,
    worktree_path: &Path,
    branch_name: &str,
    base_ref: &str,
) -> Result<()> {
    let check_output = run_git_output(repo_path, ["check-ref-format", "--branch", branch_name])
        .with_context(|| format!("failed to validate branch name `{branch_name}`"))?;
    if !check_output.status.success() {
        let stdout = String::from_utf8_lossy(&check_output.stdout)
            .trim()
            .to_string();
        let stderr = String::from_utf8_lossy(&check_output.stderr)
            .trim()
            .to_string();
        bail!("invalid branch name `{branch_name}`\nstdout: {stdout}\nstderr: {stderr}");
    }

    if worktree_path.exists() {
        bail!("worktree path already exists: {}", worktree_path.display());
    }

    let worktree_path_str = worktree_path.to_string_lossy().to_string();
    run_git(
        repo_path,
        [
            "worktree",
            "add",
            "-b",
            branch_name,
            &worktree_path_str,
            base_ref,
        ],
    )
    .with_context(|| {
        format!(
            "failed to create worktree `{}` for branch `{branch_name}` from `{base_ref}`",
            worktree_path.display()
        )
    })
}

pub fn git_remove_worktree(repo_path: &Path, worktree_path: &Path) -> Result<()> {
    let worktree_path_str = worktree_path.to_string_lossy().to_string();
    run_git(
        repo_path,
        ["worktree", "remove", "--force", &worktree_path_str],
    )
    .with_context(|| format!("failed to remove worktree `{}`", worktree_path.display()))
}

pub fn git_delete_branch(repo_path: &Path, branch_name: &str) -> Result<()> {
    run_git(repo_path, ["branch", "-d", branch_name])
        .with_context(|| format!("failed to delete branch `{branch_name}`"))
}

pub fn git_is_valid_repo(path: &Path) -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn git_get_remote_url(repo_path: &Path) -> Option<String> {
    let output = run_git_output(repo_path, ["remote", "get-url", "origin"]).ok()?;
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

pub fn derive_worktree_path(base_dir: &Path, repo_path: &Path, branch_name: &str) -> PathBuf {
    let repo_name = repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo");
    let repo_slug = sanitize_slug(repo_name, "repo");
    let branch_slug = sanitize_slug(branch_name, "branch");

    let repo_dir = base_dir.join(repo_slug);
    let candidate = repo_dir.join(&branch_slug);
    if !candidate.exists() {
        return candidate;
    }

    let mut index = 2;
    loop {
        let with_suffix = repo_dir.join(format!("{branch_slug}-{index}"));
        if !with_suffix.exists() {
            return with_suffix;
        }
        index += 1;
    }
}

fn sanitize_slug(input: &str, fallback: &str) -> String {
    let mut slug = String::new();
    let mut prev_dash = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        fallback.to_string()
    } else {
        slug
    }
}

fn branch_exists(repo_path: &Path, branch_name: &str) -> bool {
    run_git_output(
        repo_path,
        [
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch_name}"),
        ],
    )
    .is_ok()
        || run_git_output(
            repo_path,
            [
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/remotes/origin/{branch_name}"),
            ],
        )
        .is_ok()
}

fn run_git<I, S>(repo_path: &Path, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    run_git_output(repo_path, args).map(|_| ())
}

fn run_git_output<I, S>(repo_path: &Path, args: I) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args_vec: Vec<String> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_string())
        .collect();
    let output = Command::new("git")
        .args(args_vec.iter().map(String::as_str))
        .current_dir(repo_path)
        .output()
        .with_context(|| {
            format!(
                "failed to run git command in {}: git {}",
                repo_path.display(),
                args_vec.join(" ")
            )
        })?;

    if output.status.success() {
        Ok(output)
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git command failed in {}: git {}\nstdout: {}\nstderr: {}",
            repo_path.display(),
            args_vec.join(" "),
            stdout.trim(),
            stderr.trim()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    #[test]
    fn test_is_valid_repo() {
        let repo = TestRepo::new("valid-repo").expect("repo should be created");
        assert!(git_is_valid_repo(repo.path()));

        let not_repo = TempDir::new().expect("temp dir should be created");
        assert!(!git_is_valid_repo(not_repo.path()));
    }

    #[test]
    fn test_detect_default_branch_from_symbolic_ref() {
        let repo = TestRepo::new("default-branch").expect("repo should be created");
        repo.git([
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ])
        .expect("symbolic ref should be set");

        assert_eq!(git_detect_default_branch(repo.path()), "main");
    }

    #[test]
    fn test_detect_default_branch_fallback_main_master_then_first() {
        let repo_main = TestRepo::new("fallback-main").expect("repo should be created");
        assert_eq!(git_detect_default_branch(repo_main.path()), "main");

        let repo_master = TestRepo::new("fallback-master").expect("repo should be created");
        repo_master
            .git(["branch", "-m", "main", "master"])
            .expect("branch should be renamed");
        assert_eq!(git_detect_default_branch(repo_master.path()), "master");

        let repo_first = TestRepo::new("fallback-first").expect("repo should be created");
        repo_first
            .git(["branch", "-m", "main", "trunk"])
            .expect("branch should be renamed");
        assert_eq!(git_detect_default_branch(repo_first.path()), "trunk");
    }

    #[test]
    fn test_create_worktree_from_base() {
        let repo = TestRepo::new_with_origin_main("worktree-base").expect("repo should be created");
        let worktree = repo.temp.path().join("wt-main");

        git_create_worktree(repo.path(), &worktree, "feature/test", "origin/main")
            .expect("worktree should be created");

        assert!(worktree.exists());
        let branches = repo
            .git_stdout(["branch", "--format=%(refname:short)"])
            .expect("branches should list");
        assert!(branches.lines().any(|line| line.trim() == "feature/test"));
        let worktrees = repo
            .git_stdout(["worktree", "list"])
            .expect("worktree list should work");
        assert!(worktrees.contains(worktree.to_string_lossy().as_ref()));
    }

    #[test]
    fn test_remove_worktree_and_delete_branch() {
        let repo =
            TestRepo::new_with_origin_main("remove-worktree").expect("repo should be created");
        let worktree = repo.temp.path().join("wt-remove");

        git_create_worktree(repo.path(), &worktree, "feature/remove", "origin/main")
            .expect("worktree should be created");
        git_remove_worktree(repo.path(), &worktree).expect("worktree should be removed");

        assert!(!worktree.exists());
        git_delete_branch(repo.path(), "feature/remove").expect("branch should be deleted safely");

        let branches = repo
            .git_stdout(["branch", "--format=%(refname:short)"])
            .expect("branches should list");
        assert!(!branches.lines().any(|line| line.trim() == "feature/remove"));
    }

    #[test]
    fn test_invalid_branch_name() {
        let repo =
            TestRepo::new_with_origin_main("invalid-branch").expect("repo should be created");
        let worktree = repo.temp.path().join("wt-invalid");

        for invalid in ["bad name", "bad..name", "bad~name"] {
            let result = git_create_worktree(repo.path(), &worktree, invalid, "origin/main");
            assert!(result.is_err(), "branch should be rejected: {invalid}");
        }
    }

    #[test]
    fn test_list_branches_and_tags() {
        let repo = TestRepo::new_with_origin_main("branches-tags").expect("repo should be created");
        repo.git(["branch", "feature/local", "main"])
            .expect("local branch should be created");
        repo.git(["tag", "v1.0.0"]).expect("tag should be created");

        let branches = git_list_branches(repo.path());
        assert!(branches.iter().any(|b| b.name == "main" && !b.is_remote));
        assert!(
            branches
                .iter()
                .any(|b| b.name == "origin/main" && b.is_remote)
        );
        assert!(
            branches
                .iter()
                .any(|b| b.name == "feature/local" && !b.is_remote)
        );

        let tags = git_list_tags(repo.path());
        assert_eq!(tags, vec!["v1.0.0".to_string()]);
    }

    #[test]
    fn test_get_remote_url() {
        let repo = TestRepo::new_with_origin_main("remote-url").expect("repo should be created");
        let remote = git_get_remote_url(repo.path());
        assert_eq!(
            remote.as_deref(),
            Some("https://example.com/remote-url.git")
        );
    }

    #[test]
    fn test_derive_worktree_path_slug_and_collision() {
        let base = TempDir::new().expect("temp dir should be created");
        let repo_path = base.path().join("my.repo name");
        fs::create_dir_all(&repo_path).expect("repo path should be created");

        let p1 = derive_worktree_path(base.path(), &repo_path, "feature/add api");
        assert!(p1.ends_with("my-repo-name/feature-add-api"));

        fs::create_dir_all(&p1).expect("first worktree should be created");
        let p2 = derive_worktree_path(base.path(), &repo_path, "feature/add api");
        assert!(p2.ends_with("my-repo-name/feature-add-api-2"));
    }

    #[test]
    fn test_worktree_path_no_collision_between_repos() {
        let base = TempDir::new().expect("temp dir should be created");
        let repo_one = base.path().join("repo-one");
        let repo_two = base.path().join("repo_two");
        fs::create_dir_all(&repo_one).expect("repo one should exist");
        fs::create_dir_all(&repo_two).expect("repo two should exist");

        let path_one = derive_worktree_path(base.path(), &repo_one, "feature/shared");
        let path_two = derive_worktree_path(base.path(), &repo_two, "feature/shared");

        assert_ne!(path_one, path_two);
    }

    #[test]
    fn test_non_ascii_branch_name_slug_and_create() {
        let base = TempDir::new().expect("temp dir should be created");
        let repo_path = base.path().join("repo");
        fs::create_dir_all(&repo_path).expect("repo path should exist");
        let slugged = derive_worktree_path(base.path(), &repo_path, "feat/日本語");
        assert!(slugged.ends_with("repo/feat"));

        let repo =
            TestRepo::new_with_origin_main("non-ascii-branch").expect("repo should be created");
        let worktree = repo.temp.path().join("wt-non-ascii");
        git_create_worktree(repo.path(), &worktree, "feat/日本語", "origin/main")
            .expect("non-ascii branch name should be accepted by git");
        assert!(worktree.exists());
    }

    #[test]
    fn test_spaces_in_worktree_path() {
        let repo = TestRepo::new_with_origin_main("spaces-path").expect("repo should be created");
        let worktree = repo.temp.path().join("folder with spaces").join("new wt");

        git_create_worktree(repo.path(), &worktree, "feature/spaces", "origin/main")
            .expect("worktree should be created with spaces path");
        assert!(worktree.exists());
    }

    struct TestRepo {
        temp: TempDir,
        repo: PathBuf,
    }

    impl TestRepo {
        fn new(name: &str) -> Result<Self> {
            let temp = TempDir::new().context("failed to create temp dir")?;
            let repo = temp.path().join("repo");
            fs::create_dir_all(&repo).context("failed to create repo dir")?;

            let test_repo = Self { temp, repo };
            test_repo.git(["init", "-b", "main"])?;
            test_repo.git(["config", "user.name", "Test User"])?;
            test_repo.git(["config", "user.email", "test@example.com"])?;
            test_repo
                .git(["commit", "--allow-empty", "-m", "init"])
                .context("failed to create initial commit")?;
            test_repo
                .git([
                    "remote",
                    "add",
                    "origin",
                    &format!("https://example.com/{name}.git"),
                ])
                .context("failed to add origin remote")?;
            Ok(test_repo)
        }

        fn new_with_origin_main(name: &str) -> Result<Self> {
            let temp = TempDir::new().context("failed to create temp dir")?;
            let bare = temp.path().join("origin.git");
            let seed = temp.path().join("seed");
            fs::create_dir_all(&seed).context("failed to create seed dir")?;

            run_git_in(&temp.path().to_path_buf(), ["init", "--bare", "origin.git"])?;

            let seed_path = seed.to_string_lossy().to_string();
            run_git_in(
                &temp.path().to_path_buf(),
                ["init", "-b", "main", &seed_path],
            )?;
            run_git_in(&seed, ["config", "user.name", "Test User"])?;
            run_git_in(&seed, ["config", "user.email", "test@example.com"])?;
            run_git_in(&seed, ["commit", "--allow-empty", "-m", "init"])?;
            run_git_in(
                &seed,
                ["remote", "add", "origin", bare.to_string_lossy().as_ref()],
            )?;
            run_git_in(&seed, ["push", "-u", "origin", "main"])?;

            let repo = temp.path().join("repo");
            let repo_path = repo.to_string_lossy().to_string();
            run_git_in(
                &temp.path().to_path_buf(),
                ["clone", bare.to_string_lossy().as_ref(), &repo_path],
            )?;
            run_git_in(&repo, ["config", "user.name", "Test User"])?;
            run_git_in(&repo, ["config", "user.email", "test@example.com"])?;
            run_git_in(
                &repo,
                [
                    "remote",
                    "set-url",
                    "origin",
                    &format!("https://example.com/{name}.git"),
                ],
            )?;

            Ok(Self { temp, repo })
        }

        fn path(&self) -> &Path {
            &self.repo
        }

        fn git<I, S>(&self, args: I) -> Result<()>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            run_git_in(&self.repo, args)
        }

        fn git_stdout<I, S>(&self, args: I) -> Result<String>
        where
            I: IntoIterator<Item = S>,
            S: AsRef<str>,
        {
            let args_vec: Vec<String> = args
                .into_iter()
                .map(|arg| arg.as_ref().to_string())
                .collect();
            let output = Command::new("git")
                .args(args_vec.iter().map(String::as_str))
                .current_dir(&self.repo)
                .output()
                .with_context(|| format!("failed to run git {}", args_vec.join(" ")))?;

            if !output.status.success() {
                bail!(
                    "git command failed: git {}\nstdout: {}\nstderr: {}",
                    args_vec.join(" "),
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim()
                );
            }

            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }
    }

    fn run_git_in<I, S>(dir: &Path, args: I) -> Result<()>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let args_vec: Vec<String> = args
            .into_iter()
            .map(|arg| arg.as_ref().to_string())
            .collect();
        let output = Command::new("git")
            .args(args_vec.iter().map(String::as_str))
            .current_dir(dir)
            .output()
            .with_context(|| format!("failed to run git {}", args_vec.join(" ")))?;

        if output.status.success() {
            Ok(())
        } else {
            bail!(
                "git command failed in {}: git {}\nstdout: {}\nstderr: {}",
                dir.display(),
                args_vec.join(" "),
                String::from_utf8_lossy(&output.stdout).trim(),
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
    }
}
