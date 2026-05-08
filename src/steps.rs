use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use crate::config::{CommandStep, CopyStep, ScriptStep, Step};

/// Context shared by all steps for a given `wt new` invocation. Used to
/// resolve relative paths (against `repo_root`) and to populate the WT_* env
/// vars exposed to `command` and `script` steps.
pub struct StepContext<'a> {
    pub repo_root: &'a Path,
    pub worktree_path: &'a Path,
    pub worktree_name: &'a str,
    pub branch: &'a str,
    pub default_branch: &'a str,
}

impl<'a> StepContext<'a> {
    fn env_pairs(&self) -> [(&'static str, String); 5] {
        [
            ("WT_PATH", self.worktree_path.display().to_string()),
            ("WT_NAME", self.worktree_name.to_string()),
            ("WT_BRANCH", self.branch.to_string()),
            ("WT_REPO_ROOT", self.repo_root.display().to_string()),
            ("WT_DEFAULT_BRANCH", self.default_branch.to_string()),
        ]
    }
}

/// Run all steps in order. Aborts (returns Err) on the first failure; the
/// caller leaves the worktree in place per the design.
pub fn run_all(ctx: &StepContext<'_>, steps: &[Step]) -> Result<()> {
    for (idx, step) in steps.iter().enumerate() {
        let label = step_label(step);
        println!("Step {}: {}", idx + 1, label);
        run_one(ctx, step)
            .with_context(|| format!("step {} ({}) failed", idx + 1, label))?;
    }
    Ok(())
}

fn step_label(step: &Step) -> String {
    match step {
        Step::Copy(s) => format!("copy {} path(s)", s.paths.len()),
        Step::Command(s) => format!("command `{}`", s.run),
        Step::Script(s) => format!("script {}", s.path),
    }
}

fn run_one(ctx: &StepContext<'_>, step: &Step) -> Result<()> {
    match step {
        Step::Copy(s) => run_copy(ctx, s),
        Step::Command(s) => run_command(ctx, s),
        Step::Script(s) => run_script(ctx, s),
    }
}

fn run_copy(ctx: &StepContext<'_>, step: &CopyStep) -> Result<()> {
    for rel in &step.paths {
        let src = ctx.repo_root.join(rel);
        let dst = ctx.worktree_path.join(rel);

        if src.is_dir() {
            println!("  copying directory {}", rel);
            copy_dir_recursive(&src, &dst)
                .with_context(|| format!("copying directory {}", rel))?;
        } else if src.is_file() {
            println!("  copying file {}", rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating {}", parent.display()))?;
            }
            std::fs::copy(&src, &dst)
                .with_context(|| format!("copying {} -> {}", src.display(), dst.display()))?;
        } else {
            // Mirror the old script: silently skip if source doesn't exist.
            // (No warning, no failure.)
        }
    }
    Ok(())
}

fn run_command(ctx: &StepContext<'_>, step: &CommandStep) -> Result<()> {
    // Run via the user's shell so that pipes, env expansion, etc. work without
    // requiring callers to wrap things in `bash -c` themselves.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let mut cmd = Command::new(&shell);
    cmd.arg("-c").arg(&step.run);
    cmd.current_dir(ctx.worktree_path);
    for (k, v) in ctx.env_pairs() {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .with_context(|| format!("spawning shell for command `{}`", step.run))?;
    if !status.success() {
        return Err(anyhow!(
            "command `{}` exited with status {:?}",
            step.run,
            status.code()
        ));
    }
    Ok(())
}

fn run_script(ctx: &StepContext<'_>, step: &ScriptStep) -> Result<()> {
    let script_path: PathBuf = if Path::new(&step.path).is_absolute() {
        PathBuf::from(&step.path)
    } else {
        ctx.repo_root.join(&step.path)
    };

    if !script_path.exists() {
        if step.optional {
            println!("  script {} not found; skipping (optional)", step.path);
            return Ok(());
        }
        return Err(anyhow!("script not found: {}", script_path.display()));
    }

    let mut cmd = Command::new("bash");
    cmd.arg(&script_path);
    cmd.current_dir(ctx.worktree_path);
    for (k, v) in ctx.env_pairs() {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .with_context(|| format!("spawning bash {}", script_path.display()))?;
    if !status.success() {
        return Err(anyhow!(
            "script {} exited with status {:?}",
            script_path.display(),
            status.code()
        ));
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .with_context(|| format!("creating {}", dst.display()))?;
    for entry in std::fs::read_dir(src)
        .with_context(|| format!("reading {}", src.display()))?
    {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ft.is_symlink() {
            let target = std::fs::read_link(&from)
                .with_context(|| format!("readlink {}", from.display()))?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &to)
                .with_context(|| format!("symlink {} -> {}", to.display(), target.display()))?;
            #[cfg(not(unix))]
            std::fs::copy(&from, &to)
                .with_context(|| format!("copying {} -> {}", from.display(), to.display()))?;
        } else {
            std::fs::copy(&from, &to)
                .with_context(|| format!("copying {} -> {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}
