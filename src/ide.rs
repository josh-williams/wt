use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

/// Open the given worktree path in Cursor. If the `cursor` command isn't on
/// PATH, prints a warning and the path so the user can open it manually.
pub fn open_in_cursor(worktree_path: &Path) -> Result<()> {
    if which::which("cursor").is_err() {
        eprintln!(
            "warning: `cursor` command not found on PATH; skipping IDE launch."
        );
        println!("Worktree at: {}", worktree_path.display());
        return Ok(());
    }

    println!("Opening worktree in Cursor...");
    let status = Command::new("cursor")
        .arg(worktree_path)
        .status()
        .with_context(|| format!("spawning cursor {}", worktree_path.display()))?;
    if !status.success() {
        eprintln!(
            "warning: cursor exited with status {:?}; worktree at: {}",
            status.code(),
            worktree_path.display()
        );
    }
    Ok(())
}
