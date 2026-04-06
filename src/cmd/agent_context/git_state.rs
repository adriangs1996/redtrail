// Collect current git repository state via shell commands.

use crate::core::fmt::ascii::format_relative_time;

pub struct GitState {
    pub branch: Option<String>,
    pub uncommitted_count: usize,
    pub last_commit_message: Option<String>,
    pub last_commit_relative_time: Option<String>,
}

pub fn collect_git_state() -> GitState {
    GitState {
        branch: git_branch(),
        uncommitted_count: git_uncommitted_count(),
        last_commit_message: git_last_commit_message(),
        last_commit_relative_time: git_last_commit_time(),
    }
}

fn git_branch() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if branch.is_empty() { None } else { Some(branch) }
}

fn git_uncommitted_count() -> usize {
    std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .count()
        })
        .unwrap_or(0)
}

fn git_last_commit_message() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let msg = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if msg.is_empty() { None } else { Some(msg) }
}

fn git_last_commit_time() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["log", "-1", "--format=%ct"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let ts: i64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(format_relative_time(ts))
}
