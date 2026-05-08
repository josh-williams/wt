use anyhow::{anyhow, Result};

use crate::cli::PathArgs;
use crate::config::LoadedConfig;

pub fn run(args: PathArgs) -> Result<()> {
    let loaded = LoadedConfig::load()?;
    let path = loaded.worktree_path_for_name(&args.name);
    if !path.exists() {
        return Err(anyhow!(
            "worktree {} does not exist at {}",
            args.name,
            path.display()
        ));
    }
    println!("{}", path.display());
    Ok(())
}
