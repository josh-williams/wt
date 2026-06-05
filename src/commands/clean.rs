use anyhow::{anyhow, Result};
use dialoguer::{theme::ColorfulTheme, Confirm};
use owo_colors::OwoColorize;

use crate::cli::CleanArgs;
use crate::config::LoadedConfig;
use crate::gh;
use crate::repo::{self, RemoveOutcome};

struct Candidate {
    dir_name: String,
    branch: String,
}

pub fn run(args: CleanArgs) -> Result<()> {
    let loaded = LoadedConfig::load()?;

    println!("Checking for worktrees with merged branches...");
    println!();

    let names = loaded.list_worktree_names()?;
    if names.is_empty() {
        println!("No worktrees found in {}", loaded.worktrees_dir.display());
        return Ok(());
    }

    println!("Fetching latest from origin...");
    repo::fetch_origin_quiet(&loaded.repo_root, &loaded.config.default_branch);
    println!();

    let mut candidates: Vec<Candidate> = Vec::new();

    for dir_name in &names {
        let worktree_path = loaded.worktree_path_for_name(dir_name);
        let branch = match repo::current_branch(&worktree_path) {
            Some(b) => b,
            None => {
                println!(
                    "{}  Skipping {}: cannot determine branch",
                    "warn:".yellow(),
                    dir_name
                );
                continue;
            }
        };

        println!("Checking {} (branch: {})...", dir_name, branch);

        if branch_has_qualifying_pr(&branch, args.include_closed)? {
            println!("  {} branch {} qualifies for cleanup", "OK".green(), branch);
            candidates.push(Candidate {
                dir_name: dir_name.clone(),
                branch,
            });
        } else {
            println!(
                "  {} branch {} does NOT qualify - keeping",
                "skip:".dimmed(),
                branch
            );
        }
    }

    println!();

    if candidates.is_empty() {
        println!("No qualifying worktrees found. Nothing to clean up.");
        return Ok(());
    }

    println!("==========================================");
    println!(
        "Found {} worktree(s) to clean up:",
        candidates.len()
    );
    println!("==========================================");
    for (i, c) in candidates.iter().enumerate() {
        let path = loaded.worktree_path_for_name(&c.dir_name);
        println!();
        println!("[{}] {}", i + 1, c.dir_name);
        println!("    Branch: {}", c.branch);
        println!("    Path:   {}", path.display());
    }
    println!();

    if args.dry_run {
        println!("--dry-run: not removing anything.");
        return Ok(());
    }

    if !args.yes {
        println!("==========================================");
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Proceed with cleanup?")
            .default(false)
            .interact()
            .map_err(|e| anyhow!("prompt failed: {}", e))?;
        if !confirmed {
            println!("Cleanup cancelled.");
            return Ok(());
        }
    }

    println!();
    println!("Starting cleanup...");
    println!();

    let mut had_errors = false;
    for c in &candidates {
        let path = loaded.worktree_path_for_name(&c.dir_name);
        println!("Cleaning up {} (branch: {})...", c.dir_name, c.branch);

        // Force-remove since we know the PR is merged/closed -- whatever's
        // uncommitted here is post-merge cruft we don't care about.
        match repo::worktree_remove(&loaded.repo_root, &path, true)? {
            RemoveOutcome::Removed => {}
            RemoveOutcome::NeedsForce(msg)
            | RemoveOutcome::Orphan(msg)
            | RemoveOutcome::Other(msg) => {
                eprintln!("  failed to remove worktree: {}", msg.trim());
                had_errors = true;
                println!();
                continue;
            }
        }

        match repo::delete_local_branch(&loaded.repo_root, &c.branch) {
            Ok(()) => println!("  deleted local branch {}", c.branch),
            Err(e) => {
                eprintln!("  warning: could not delete local branch {}: {}", c.branch, e);
            }
        }

        println!();
    }

    println!("Cleanup complete!");
    if had_errors {
        return Err(anyhow!("one or more worktrees failed to clean up"));
    }
    Ok(())
}

fn branch_has_qualifying_pr(branch: &str, include_closed: bool) -> Result<bool> {
    if gh::has_pr_in_state(branch, "merged")? {
        return Ok(true);
    }
    if include_closed && gh::has_pr_in_state(branch, "closed")? {
        return Ok(true);
    }
    Ok(false)
}
