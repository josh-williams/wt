use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use anyhow::{anyhow, Context, Result};

/// Result of a `git worktree remove` attempt.
pub enum RemoveOutcome {
    /// Removed cleanly.
    Removed,
    /// Failed because the worktree has modified or untracked files; needs --force.
    NeedsForce(String),
    /// Failed because git doesn't recognize the path as a working tree (orphan
    /// directory: dir exists on disk but git's admin record is gone).
    Orphan(String),
    /// Some other failure with the captured stderr.
    Other(String),
}

/// Returns the absolute path to the main git checkout.
pub fn repo_root() -> Result<PathBuf> {
    let out = run_git(None, &["rev-parse", "--show-toplevel"])?;
    let s = stdout_trimmed(&out)?;
    Ok(PathBuf::from(s))
}

/// `git fetch origin <branch>` (quiet, ignore failure: matches old script behavior).
pub fn fetch_origin_quiet(repo_root: &Path, branch: &str) {
    let _ = Command::new("git")
        .args(["-C", repo_root.to_str().unwrap_or(".")])
        .args(["fetch", "origin", branch, "--quiet"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

pub fn local_branch_exists(repo_root: &Path, branch: &str) -> bool {
    let refspec = format!("refs/heads/{}", branch);
    git_status(repo_root, &["show-ref", "--verify", "--quiet", &refspec])
}

pub fn remote_branch_exists(repo_root: &Path, branch: &str) -> bool {
    let refspec = format!("refs/remotes/origin/{}", branch);
    git_status(repo_root, &["show-ref", "--verify", "--quiet", &refspec])
}

/// `git worktree add <path> <branch>` (existing branch).
pub fn worktree_add_existing(repo_root: &Path, path: &Path, branch: &str) -> Result<()> {
    run_git_inherit(
        Some(repo_root),
        &["worktree", "add", path_str(path)?, branch],
    )
}

/// `git worktree add <path> -b <branch> --no-track origin/<base>` (new branch off base).
pub fn worktree_add_new_no_track(
    repo_root: &Path,
    path: &Path,
    branch: &str,
    base: &str,
) -> Result<()> {
    let base_ref = format!("origin/{}", base);
    run_git_inherit(
        Some(repo_root),
        &[
            "worktree",
            "add",
            path_str(path)?,
            "-b",
            branch,
            "--no-track",
            &base_ref,
        ],
    )
}

/// Try `git worktree remove <path>`. On failure, classify the error so callers
/// can decide whether to prompt for `--force`.
pub fn worktree_remove(repo_root: &Path, path: &Path, force: bool) -> Result<RemoveOutcome> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    let path_arg = path_str(path)?;
    args.push(path_arg);

    let out = capture_git(Some(repo_root), &args)?;
    if out.status.success() {
        // Mirror old script: echo the output for the user.
        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !combined.trim().is_empty() {
            print!("{}", combined);
        }
        return Ok(RemoveOutcome::Removed);
    }

    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();

    // Classify before deciding whether to echo. For orphan dirs we'd rather
    // print our own friendlier explanation than git's raw "fatal: ... is not a
    // working tree" line.
    let outcome = if stderr.contains("contains modified or untracked files") {
        RemoveOutcome::NeedsForce(stderr.clone())
    } else if stderr.contains("is not a working tree") {
        RemoveOutcome::Orphan(stderr.clone())
    } else {
        RemoveOutcome::Other(stderr.clone())
    };

    let suppress_stderr = matches!(outcome, RemoveOutcome::Orphan(_));
    if !stdout.trim().is_empty() {
        println!("{}", stdout.trim_end());
    }
    if !suppress_stderr && !stderr.trim().is_empty() {
        eprintln!("{}", stderr.trim_end());
    }

    Ok(outcome)
}

/// Resolve the current branch name for a worktree directory.
/// Returns `None` if HEAD is detached or git fails.
pub fn current_branch(worktree_path: &Path) -> Option<String> {
    let out = capture_git(Some(worktree_path), &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.is_empty() || s == "HEAD" {
        None
    } else {
        Some(s)
    }
}

/// True iff the worktree has uncommitted changes (modified, staged, or untracked).
pub fn is_dirty(worktree_path: &Path) -> bool {
    capture_git(Some(worktree_path), &["status", "--porcelain"])
        .map(|out| !out.stdout.is_empty())
        .unwrap_or(false)
}

/// Try `git -C <repo_root> branch -D <branch>`. Best-effort; logs failure.
pub fn delete_local_branch(repo_root: &Path, branch: &str) -> Result<()> {
    let out = capture_git(Some(repo_root), &["branch", "-D", branch])?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("git branch -D {} failed: {}", branch, stderr.trim()));
    }
    Ok(())
}

/// `git worktree prune`.
pub fn worktree_prune(repo_root: &Path) -> Result<()> {
    run_git_inherit(Some(repo_root), &["worktree", "prune"])
}

// ---------- internals ----------

fn run_git(cwd: Option<&Path>, args: &[&str]) -> Result<Output> {
    let mut cmd = Command::new("git");
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    cmd.args(args);
    let out = cmd.output().with_context(|| {
        format!(
            "spawning git {}",
            args.join(" ")
        )
    })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(anyhow!(
            "git {} failed: {}",
            args.join(" "),
            if stderr.is_empty() {
                format!("exit code {:?}", out.status.code())
            } else {
                stderr
            }
        ));
    }
    Ok(out)
}

fn capture_git(cwd: Option<&Path>, args: &[&str]) -> Result<Output> {
    let mut cmd = Command::new("git");
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    cmd.args(args);
    cmd.output()
        .with_context(|| format!("spawning git {}", args.join(" ")))
}

fn run_git_inherit(cwd: Option<&Path>, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("git");
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    cmd.args(args);
    let status = cmd
        .status()
        .with_context(|| format!("spawning git {}", args.join(" ")))?;
    if !status.success() {
        return Err(anyhow!(
            "git {} failed (exit {:?})",
            args.join(" "),
            status.code()
        ));
    }
    Ok(())
}

fn git_status(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(cwd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn stdout_trimmed(out: &Output) -> Result<String> {
    let s = String::from_utf8(out.stdout.clone())?;
    Ok(s.trim().to_string())
}

fn path_str(p: &Path) -> Result<&str> {
    p.to_str()
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", p.display()))
}
