use std::path::Path;

use anyhow::{Result, anyhow};
use dialoguer::{Confirm, MultiSelect, Select, theme::ColorfulTheme};

use crate::cli::RmArgs;
use crate::config::LoadedConfig;
use crate::repo::{self, RemoveOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoveKind {
    Normal,
    Orphan,
}

pub fn run(args: RmArgs) -> Result<()> {
    let loaded = LoadedConfig::load()?;

    if args.interactive {
        if args.name.is_some() {
            return Err(anyhow!(
                "cannot pass a worktree name with --interactive; use `wt rm -i` alone"
            ));
        }
        return run_interactive(&loaded, args.force, false);
    }

    let name = match args.name {
        Some(n) => n,
        None => pick_one(&loaded)?,
    };

    let worktree_path = loaded.worktree_path_for_name(&name);
    if !worktree_path.exists() {
        return Err(anyhow!(
            "worktree {} does not exist at {}",
            name,
            worktree_path.display()
        ));
    }

    match remove_worktree(&loaded, &worktree_path, args.force, false)? {
        RemoveKind::Orphan => {
            if let Err(e) = repo::worktree_prune(&loaded.repo_root) {
                eprintln!("warning: git worktree prune failed: {}", e);
            }
        }
        RemoveKind::Normal => {}
    }
    Ok(())
}

/// Multi-select worktrees, confirm once, then remove each (no per-worktree prompts).
pub fn run_interactive(loaded: &LoadedConfig, skip_confirm: bool, dry_run: bool) -> Result<()> {
    let names = loaded.list_worktree_names()?;
    if names.is_empty() {
        println!("No worktrees found in {}", loaded.worktrees_dir.display());
        return Ok(());
    }

    let labels: Vec<String> = names
        .iter()
        .map(|name| {
            let path = loaded.worktree_path_for_name(name);
            let branch = repo::current_branch(&path).unwrap_or_else(|| "(detached)".into());
            format!("{name}  [{branch}]")
        })
        .collect();

    let selected = MultiSelect::with_theme(&ColorfulTheme::default())
        .with_prompt("Select worktrees to remove (space to toggle, enter to confirm)")
        .items(&labels)
        .interact()
        .map_err(|e| anyhow!("picker failed: {}", e))?;

    if selected.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    let chosen: Vec<String> = selected.iter().map(|&i| names[i].clone()).collect();

    println!();
    println!("==========================================");
    println!("Will remove {} worktree(s):", chosen.len());
    println!("==========================================");
    for name in &chosen {
        let path = loaded.worktree_path_for_name(name);
        let branch = repo::current_branch(&path).unwrap_or_else(|| "(detached)".into());
        println!("  {name}  [{branch}]");
    }
    println!();

    if dry_run {
        println!("--dry-run: not removing anything.");
        return Ok(());
    }

    if !skip_confirm {
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Remove {} worktree(s)?", chosen.len()))
            .default(false)
            .interact()
            .map_err(|e| anyhow!("prompt failed: {}", e))?;
        if !confirmed {
            println!("Removal cancelled.");
            return Ok(());
        }
    }

    println!();
    println!("Removing...");
    println!();

    let mut had_errors = false;
    let mut removed_orphan = false;

    for name in &chosen {
        let path = loaded.worktree_path_for_name(name);
        println!("Removing {name}...");
        match remove_worktree(loaded, &path, skip_confirm, true) {
            Ok(RemoveKind::Normal) => {}
            Ok(RemoveKind::Orphan) => removed_orphan = true,
            Err(e) => {
                eprintln!("  failed: {:#}", e);
                had_errors = true;
            }
        }
        println!();
    }

    if removed_orphan {
        if let Err(e) = repo::worktree_prune(&loaded.repo_root) {
            eprintln!("warning: git worktree prune failed: {}", e);
        }
    }

    if had_errors {
        return Err(anyhow!("one or more worktrees failed to remove"));
    }

    println!("Done.");
    Ok(())
}

/// Remove a single worktree. When `batch` is true, skip per-worktree prompts (caller
/// already confirmed); use force semantics for dirty trees and orphans.
fn remove_worktree(
    loaded: &LoadedConfig,
    worktree_path: &Path,
    force: bool,
    batch: bool,
) -> Result<RemoveKind> {
    let effective_force = force || batch;

    match repo::worktree_remove(&loaded.repo_root, worktree_path, effective_force)? {
        RemoveOutcome::Removed => Ok(RemoveKind::Normal),
        RemoveOutcome::NeedsForce(_) if effective_force => Err(anyhow!(
            "git refused to remove {} even with --force",
            worktree_path.display()
        )),
        RemoveOutcome::NeedsForce(_) => {
            println!();
            println!("The worktree contains modified or untracked files.");
            let confirmed = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("Force remove it?")
                .default(false)
                .interact()
                .map_err(|e| anyhow!("prompt failed: {}", e))?;
            if !confirmed {
                println!("Worktree removal cancelled.");
                return Err(anyhow!("cancelled"));
            }
            match repo::worktree_remove(&loaded.repo_root, worktree_path, true)? {
                RemoveOutcome::Removed => Ok(RemoveKind::Normal),
                RemoveOutcome::NeedsForce(msg)
                | RemoveOutcome::Orphan(msg)
                | RemoveOutcome::Other(msg) => Err(anyhow!(
                    "git worktree remove --force failed: {}",
                    msg.trim()
                )),
            }
        }
        RemoveOutcome::Orphan(_) => {
            handle_orphan(worktree_path, effective_force)?;
            Ok(RemoveKind::Orphan)
        }
        RemoveOutcome::Other(msg) => Err(anyhow!("git worktree remove failed: {}", msg.trim())),
    }
}

fn handle_orphan(worktree_path: &Path, force: bool) -> Result<()> {
    println!();
    println!(
        "{} is an orphan directory (git has no record of it as a worktree).",
        worktree_path.display()
    );

    let dot_git = worktree_path.join(".git");
    let dot_git_meta = std::fs::symlink_metadata(&dot_git);
    if let Ok(meta) = &dot_git_meta
        && meta.file_type().is_dir()
    {
        return Err(anyhow!(
            "refusing to delete {}: it contains a `.git` directory, which means \
             it's a separate git repository, not a stale worktree. delete it \
             manually if that's really what you want.",
            worktree_path.display()
        ));
    }
    let has_dot_git_file = dot_git_meta
        .map(|m| !m.file_type().is_dir())
        .unwrap_or(false);
    if has_dot_git_file {
        println!(
            "  (it has a `.git` file pointing to a missing admin record -- still safe to remove)"
        );
    } else {
        println!("  (no `.git` inside -- contents are stale files left behind from a prior cleanup)");
    }

    if !force {
        let confirmed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Delete it from disk?")
            .default(false)
            .interact()
            .map_err(|e| anyhow!("prompt failed: {}", e))?;
        if !confirmed {
            println!("Removal cancelled.");
            return Err(anyhow!("cancelled"));
        }
    }

    std::fs::remove_dir_all(worktree_path).map_err(|e| {
        anyhow!(
            "failed to remove orphan directory {}: {}",
            worktree_path.display(),
            e
        )
    })?;
    println!("Removed {}", worktree_path.display());
    Ok(())
}

fn pick_one(loaded: &LoadedConfig) -> Result<String> {
    let names = loaded.list_worktree_names()?;
    if names.is_empty() {
        return Err(anyhow!(
            "no worktrees found in {}",
            loaded.worktrees_dir.display()
        ));
    }
    let idx = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select a worktree to remove")
        .items(&names)
        .default(0)
        .interact()
        .map_err(|e| anyhow!("picker failed: {}", e))?;
    Ok(names[idx].clone())
}
