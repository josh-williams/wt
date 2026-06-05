use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

use crate::repo;

pub const CONFIG_FILENAME: &str = ".wt.toml";

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub worktrees_dir: String,
    pub default_branch: String,
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Step {
    Copy(CopyStep),
    Command(CommandStep),
    Script(ScriptStep),
}

#[derive(Debug, Clone, Deserialize)]
pub struct CopyStep {
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandStep {
    pub run: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScriptStep {
    pub path: String,
    #[serde(default)]
    pub optional: bool,
}

/// Loaded configuration plus the absolute paths to the repo root and the
/// resolved worktrees directory (relative paths in config are resolved against
/// `repo_root`).
#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: Config,
    pub repo_root: PathBuf,
    pub worktrees_dir: PathBuf,
}

impl LoadedConfig {
    /// Locate the main git checkout via `git rev-parse --show-toplevel`,
    /// then read `.wt.toml` from there.
    pub fn load() -> Result<Self> {
        let repo_root = repo::repo_root()?;
        Self::load_from_repo_root(repo_root)
    }

    pub fn load_from_repo_root(repo_root: PathBuf) -> Result<Self> {
        let config_path = repo_root.join(CONFIG_FILENAME);
        if !config_path.exists() {
            return Err(anyhow!(
                "no {} found at repo root ({}). create one to configure wt for this repo.",
                CONFIG_FILENAME,
                repo_root.display()
            ));
        }

        let raw = std::fs::read_to_string(&config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("parsing {}", config_path.display()))?;

        if config.default_branch.trim().is_empty() {
            return Err(anyhow!(
                "{}: `default_branch` must be a non-empty branch name",
                config_path.display()
            ));
        }
        if config.worktrees_dir.trim().is_empty() {
            return Err(anyhow!(
                "{}: `worktrees_dir` must be a non-empty path (relative to repo root)",
                config_path.display()
            ));
        }

        let worktrees_dir = resolve_path(&repo_root, &config.worktrees_dir);

        Ok(Self {
            config,
            repo_root,
            worktrees_dir,
        })
    }

    /// Convert a branch name to a worktree directory name (slashes become hyphens).
    pub fn branch_to_dir_name(branch: &str) -> String {
        branch.replace('/', "-")
    }

    /// Absolute path to the worktree dir for the given branch.
    pub fn worktree_path_for_branch(&self, branch: &str) -> PathBuf {
        self.worktrees_dir.join(Self::branch_to_dir_name(branch))
    }

    /// Absolute path to the worktree dir for the given on-disk name.
    pub fn worktree_path_for_name(&self, name: &str) -> PathBuf {
        self.worktrees_dir.join(name)
    }

    /// List the names of subdirectories of `worktrees_dir`. Returns an empty
    /// Vec if the dir doesn't exist yet (matches `wt clean`'s old behavior of
    /// "no worktrees found").
    pub fn list_worktree_names(&self) -> Result<Vec<String>> {
        if !self.worktrees_dir.exists() {
            return Ok(Vec::new());
        }
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.worktrees_dir).with_context(|| {
            format!("reading worktrees dir {}", self.worktrees_dir.display())
        })? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(s) = entry.file_name().to_str()
            {
                names.push(s.to_string());
            }
        }
        names.sort();
        Ok(names)
    }
}

/// Resolve a config path. Absolute paths are kept as-is; relative paths are
/// joined to `repo_root`.
fn resolve_path(repo_root: &Path, p: &str) -> PathBuf {
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        repo_root.join(path)
    }
}
