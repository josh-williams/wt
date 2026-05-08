use std::process::Command;

use anyhow::{anyhow, Context, Result};

/// Returns true if `gh pr list --head <branch> --state <state>` returns at least
/// one row that matches the branch (mirrors the old script's grep check).
///
/// `state` should be one of "merged" or "closed" (gh's --state values).
pub fn has_pr_in_state(branch: &str, state: &str) -> Result<bool> {
    let out = Command::new("gh")
        .args(["pr", "list", "--head", branch, "--state", state])
        .output()
        .with_context(|| "spawning gh (is the gh CLI installed and authenticated?)")?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(anyhow!(
            "gh pr list --head {} --state {} failed: {}",
            branch,
            state,
            if stderr.is_empty() {
                format!("exit code {:?}", out.status.code())
            } else {
                stderr
            }
        ));
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout.lines().any(|line| line.contains(branch)))
}

/// Returns the PR number + state for `branch`, or None if no PR exists.
/// Used by `wt ls --rich`. Best-effort: returns Ok(None) on any gh error so
/// that `ls` doesn't blow up just because gh isn't available.
pub fn pr_summary_best_effort(branch: &str) -> Option<PrSummary> {
    let out = Command::new("gh")
        .args([
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "all",
            "--json",
            "number,state",
            "--limit",
            "1",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let body = String::from_utf8(out.stdout).ok()?;
    parse_pr_summary(&body)
}

#[derive(Debug, Clone)]
pub struct PrSummary {
    pub number: u64,
    pub state: String,
}

/// Tiny ad-hoc parser for `[{"number":123,"state":"OPEN"}]` so we don't take a
/// JSON dep just for this.
fn parse_pr_summary(body: &str) -> Option<PrSummary> {
    let trimmed = body.trim();
    if trimmed == "[]" || trimmed.is_empty() {
        return None;
    }
    let number = extract_field(trimmed, "\"number\":")
        .and_then(|s| s.parse::<u64>().ok())?;
    let state = extract_quoted_field(trimmed, "\"state\":")?;
    Some(PrSummary { number, state })
}

fn extract_field(body: &str, key: &str) -> Option<String> {
    let start = body.find(key)? + key.len();
    let rest = &body[start..];
    let end = rest
        .find([',', '}', ']'])
        .unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

fn extract_quoted_field(body: &str, key: &str) -> Option<String> {
    let start = body.find(key)? + key.len();
    let rest = body[start..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}
