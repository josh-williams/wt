use anyhow::{anyhow, Result};

use crate::cli::NewArgs;
use crate::config::LoadedConfig;
use crate::ide;
use crate::repo;
use crate::steps::{self, StepContext};

pub fn run(args: NewArgs) -> Result<()> {
    let loaded = LoadedConfig::load()?;
    let branch = args.branch.trim();
    if branch.is_empty() {
        return Err(anyhow!("branch name must not be empty"));
    }

    let dir_name = LoadedConfig::branch_to_dir_name(branch);
    let worktree_path = loaded.worktree_path_for_branch(branch);
    let repo_root = &loaded.repo_root;

    // If the worktree dir already exists and is on the same branch, just open it.
    if worktree_path.exists() {
        // Require an actual worktree marker (`.git` file/dir). Otherwise git
        // will walk up to the parent repo and report a misleading branch.
        if !worktree_path.join(".git").exists() {
            return Err(anyhow!(
                "{} already exists but is not a git worktree. remove or rename it first.",
                worktree_path.display()
            ));
        }
        match repo::current_branch(&worktree_path) {
            Some(existing) if existing == branch => {
                println!(
                    "Worktree already exists at {} on branch {}; skipping setup.",
                    worktree_path.display(),
                    branch
                );
                if !args.no_open {
                    ide::open_in_cursor(&worktree_path)?;
                }
                return Ok(());
            }
            Some(other) => {
                return Err(anyhow!(
                    "{} already exists but is on branch {} (wanted {}). remove it with `wt rm {}` first.",
                    worktree_path.display(),
                    other,
                    branch,
                    dir_name
                ));
            }
            None => {
                return Err(anyhow!(
                    "{} already exists but is not a recognizable git worktree. remove it manually.",
                    worktree_path.display()
                ));
            }
        }
    }

    // Make sure the parent worktrees dir exists so `git worktree add` doesn't blow up.
    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Always fetch (matches old script behavior; failure is non-fatal/quiet).
    repo::fetch_origin_quiet(repo_root, &loaded.config.default_branch);

    // Branch resolution: local exists -> remote exists -> create new from origin/<default> --no-track.
    if repo::local_branch_exists(repo_root, branch) {
        println!("Branch '{}' exists locally, creating worktree...", branch);
        repo::worktree_add_existing(repo_root, &worktree_path, branch)?;
    } else if repo::remote_branch_exists(repo_root, branch) {
        println!("Branch '{}' exists remotely, creating worktree...", branch);
        repo::worktree_add_existing(repo_root, &worktree_path, branch)?;
    } else {
        println!(
            "Branch '{}' doesn't exist, creating new branch from origin/{}...",
            branch, loaded.config.default_branch
        );
        repo::worktree_add_new_no_track(
            repo_root,
            &worktree_path,
            branch,
            &loaded.config.default_branch,
        )?;
    }

    let ctx = StepContext {
        repo_root,
        worktree_path: &worktree_path,
        worktree_name: &dir_name,
        branch,
        default_branch: &loaded.config.default_branch,
    };
    steps::run_all(&ctx, &loaded.config.steps)?;

    if !args.no_open {
        ide::open_in_cursor(&worktree_path)?;
    } else {
        println!("Worktree created at: {}", worktree_path.display());
    }

    Ok(())
}
