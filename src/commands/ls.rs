use std::path::PathBuf;

use anyhow::Result;
use owo_colors::OwoColorize;

use crate::cli::LsArgs;
use crate::config::LoadedConfig;
use crate::gh;
use crate::repo;

struct Row {
    name: String,
    branch: String,
    // Kept for use by the rich pass (dirty check, gh lookup); not displayed.
    path: PathBuf,
    rich: Option<RichBits>,
}

struct RichBits {
    dirty: bool,
    pr: Option<String>,
}

pub fn run(args: LsArgs) -> Result<()> {
    let loaded = LoadedConfig::load()?;
    let names = loaded.list_worktree_names()?;

    if names.is_empty() {
        println!("(no worktrees in {})", loaded.worktrees_dir.display());
        return Ok(());
    }

    // First pass: cheap data (branch + path) sequentially.
    let mut rows: Vec<Row> = names
        .into_iter()
        .map(|name| {
            let path = loaded.worktree_path_for_name(&name);
            let branch = repo::current_branch(&path).unwrap_or_else(|| "(detached)".to_string());
            Row {
                name,
                branch,
                path,
                rich: None,
            }
        })
        .collect();

    // Second pass (only when --rich): parallel git status + gh pr lookup per row.
    if args.rich {
        std::thread::scope(|s| {
            let handles: Vec<_> = rows
                .iter()
                .map(|row| {
                    let path = row.path.clone();
                    let branch = row.branch.clone();
                    s.spawn(move || RichBits {
                        dirty: repo::is_dirty(&path),
                        pr: gh::pr_summary_best_effort(&branch).map(|p| {
                            format!("#{} ({})", p.number, p.state.to_lowercase())
                        }),
                    })
                })
                .collect();
            for (row, handle) in rows.iter_mut().zip(handles) {
                row.rich = handle.join().ok();
            }
        });
    }

    print_table(&rows, args.rich);
    Ok(())
}

fn print_table(rows: &[Row], rich: bool) {
    let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let branch_w = rows
        .iter()
        .map(|r| r.branch.len())
        .max()
        .unwrap_or(6)
        .max(6);

    if rich {
        println!(
            "{:<name_w$}  {:<branch_w$}  {:<7}  {}",
            "NAME".bold(),
            "BRANCH".bold(),
            "DIRTY".bold(),
            "PR".bold(),
            name_w = name_w,
            branch_w = branch_w
        );
        for r in rows {
            let bits = r.rich.as_ref();
            let dirty = bits
                .map(|b| if b.dirty { "yes" } else { "" })
                .unwrap_or("");
            let pr = bits
                .and_then(|b| b.pr.as_deref())
                .unwrap_or("");
            println!(
                "{:<name_w$}  {:<branch_w$}  {:<7}  {}",
                r.name,
                r.branch,
                dirty,
                pr,
                name_w = name_w,
                branch_w = branch_w
            );
        }
    } else {
        println!(
            "{:<name_w$}  {}",
            "NAME".bold(),
            "BRANCH".bold(),
            name_w = name_w,
        );
        for r in rows {
            println!(
                "{:<name_w$}  {}",
                r.name,
                r.branch,
                name_w = name_w,
            );
        }
    }
}
