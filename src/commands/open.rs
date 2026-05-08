use anyhow::{anyhow, Result};
use dialoguer::{theme::ColorfulTheme, Select};

use crate::cli::OpenArgs;
use crate::config::LoadedConfig;
use crate::ide;

pub fn run(args: OpenArgs) -> Result<()> {
    let loaded = LoadedConfig::load()?;

    let name = match args.name {
        Some(n) => n,
        None => pick_worktree(&loaded)?,
    };

    let path = loaded.worktree_path_for_name(&name);
    if !path.exists() {
        return Err(anyhow!(
            "worktree {} does not exist at {}",
            name,
            path.display()
        ));
    }
    ide::open_in_cursor(&path)
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
        .with_prompt("Select a worktree to open")
        .items(&names)
        .default(0)
        .interact()
        .map_err(|e| anyhow!("picker failed: {}", e))?;
    Ok(names[idx].clone())
}
