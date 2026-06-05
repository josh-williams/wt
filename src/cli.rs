use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "wt",
    version,
    about = "Per-repo git worktree CLI: create, list, and clean worktrees with configurable startup steps",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create (or re-open) a worktree for a branch, run configured steps, and open the IDE
    #[command(visible_aliases = ["create", "c"])]
    New(NewArgs),

    /// Remove a worktree by name (interactive picker if no name given)
    #[command(visible_aliases = ["remove"])]
    Rm(RmArgs),

    /// Remove all worktrees whose branches have a merged PR
    #[command(visible_aliases = ["cleanup"])]
    Clean(CleanArgs),

    /// List worktrees in the configured worktrees dir
    #[command(visible_aliases = ["list"])]
    Ls(LsArgs),

    /// Print the absolute path to a worktree (e.g. `cd $(wt path foo)`)
    Path(PathArgs),

    /// Open the IDE for an existing worktree without re-running steps
    Open(OpenArgs),

    /// Prune git's record of worktree directories that no longer exist
    Prune,

    /// Generate a shell completion script for the given shell
    Completions(CompletionsArgs),
}

#[derive(Debug, Args)]
pub struct NewArgs {
    /// The branch name. Slashes are converted to hyphens for the directory name.
    pub branch: String,

    /// Skip opening the IDE after the worktree is created
    #[arg(long)]
    pub no_open: bool,
}

#[derive(Debug, Args)]
pub struct RmArgs {
    /// The worktree directory name (e.g. `josh-fix-bug`). Omit to pick interactively.
    pub name: Option<String>,

    /// Interactively select one or more worktrees to remove, then delete them all at the end
    #[arg(long, short = 'i')]
    pub interactive: bool,

    /// Force-remove without prompting on dirty/untracked files
    #[arg(long, short)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct CleanArgs {
    /// Show what would be cleaned, but don't remove anything
    #[arg(long)]
    pub dry_run: bool,

    /// Skip the final confirmation prompt
    #[arg(long, short)]
    pub yes: bool,

    /// Also clean worktrees whose PRs were closed without being merged
    #[arg(long)]
    pub include_closed: bool,
}

#[derive(Debug, Args)]
pub struct LsArgs {
    /// Include dirty status and PR status (slower; runs git/gh per worktree in parallel)
    #[arg(long)]
    pub rich: bool,
}

#[derive(Debug, Args)]
pub struct PathArgs {
    /// The worktree directory name
    pub name: String,
}

#[derive(Debug, Args)]
pub struct OpenArgs {
    /// The worktree directory name. Omit to pick interactively.
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Target shell (bash, zsh, fish, powershell, elvish)
    pub shell: Shell,
}
