use anyhow::Result;

use crate::config::LoadedConfig;
use crate::repo;

pub fn run() -> Result<()> {
    let loaded = LoadedConfig::load()?;
    repo::worktree_prune(&loaded.repo_root)
}
