use anyhow::{anyhow, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Select};

use crate::cli::RmArgs;
use crate::config::LoadedConfig;
use crate::repo::{self, RemoveOutcome};

pub fn run(args: RmArgs) -> Result<()> {
    let loaded = LoadedConfig::load()?;

    let name = match args.name {
        Some(n) => n,
        None => pick_worktree(&loaded)?,
    };

    let worktree_path = loaded.worktree_path_for_name(&name);
    if !worktree_path.exists() {
        return Err(anyhow!(
            "worktree {} does not exist at {}",
            name,
            worktree_path.display()
        ));
    }

    match repo::worktree_remove(&loaded.repo_root, &worktree_path, args.force)? {
        RemoveOutcome::Removed => Ok(()),
        RemoveOutcome::NeedsForce(_) if args.force => {
            // Should be unreachable: --force was passed but git still says it
            // needs --force. Surface as an error.
            Err(anyhow!(
                "git refused to remove {} even with --force",
                worktree_path.display()
            ))
        }
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
            match repo::worktree_remove(&loaded.repo_root, &worktree_path, true)? {
                RemoveOutcome::Removed => Ok(()),
                RemoveOutcome::NeedsForce(msg) | RemoveOutcome::Other(msg) => {
                    Err(anyhow!("git worktree remove --force failed: {}", msg.trim()))
                }
            }
        }
        RemoveOutcome::Other(msg) => {
            Err(anyhow!("git worktree remove failed: {}", msg.trim()))
        }
    }
}

fn pick_worktree(loaded: &LoadedConfig) -> Result<String> {
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
