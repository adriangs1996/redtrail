use redtrail::core::db::CommandRow;
use redtrail::extract::git::GitExtractor;
use redtrail::extract::types::DomainExtractor;

fn git_cmd(subcommand: &str, stdout: &str) -> CommandRow {
    CommandRow {
        id: "test-id".into(),
        session_id: "sess".into(),
        command_raw: format!("git {subcommand}"),
        command_binary: Some("git".into()),
        command_subcommand: Some(subcommand.into()),
        git_repo: Some("/home/user/project".into()),
        git_branch: Some("main".into()),
        stdout: Some(stdout.into()),
        source: "human".into(),
        timestamp_start: 1000,
        ..Default::default()
    }
}

#[test]
fn can_handle_git() {
    let ext = GitExtractor;
    assert!(ext.can_handle("git", Some("status")));
    assert!(ext.can_handle("git", Some("log")));
    assert!(ext.can_handle("git", None));
    assert!(!ext.can_handle("docker", Some("ps")));
}

#[test]
fn parse_git_status_long_format() {
    let stdout = "On branch main\nYour branch is up to date with 'origin/main'.\n\nChanges not staged for commit:\n  (use \"git add <file>...\" to update what will be committed)\n\n\tmodified:   src/main.rs\n\tmodified:   src/lib.rs\n\nUntracked files:\n  (use \"git add <file>...\" to include in what will be committed)\n\n\tnew_file.txt\n";
    let cmd = git_cmd("status", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    let files: Vec<&str> = result.entities.iter()
        .filter(|e| e.entity_type == "git_file")
        .map(|e| e.name.as_str())
        .collect();
    assert!(files.contains(&"src/main.rs"), "missing src/main.rs, got: {:?}", files);
    assert!(files.contains(&"src/lib.rs"), "missing src/lib.rs, got: {:?}", files);
    assert!(files.contains(&"new_file.txt"), "missing new_file.txt, got: {:?}", files);
    assert_eq!(files.len(), 3);
}

#[test]
fn parse_git_status_short_format() {
    let stdout = " M src/main.rs\n M src/lib.rs\n?? new_file.txt\nA  staged.rs\n";
    let cmd = git_cmd("status", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert_eq!(result.entities.iter().filter(|e| e.entity_type == "git_file").count(), 4);
}

#[test]
fn parse_git_status_clean() {
    let stdout = "On branch main\nnothing to commit, working tree clean\n";
    let cmd = git_cmd("status", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert_eq!(result.entities.iter().filter(|e| e.entity_type == "git_file").count(), 0);
}

#[test]
fn parse_git_log_default() {
    let stdout = "commit abc1234567890def1234567890abcdef12345678\nAuthor: Jane Doe <jane@example.com>\nDate:   Mon Mar 31 10:00:00 2026 -0700\n\n    Fix the bug\n\ncommit def4567890123abc4567890123456789abcdef01\nAuthor: John Smith <john@example.com>\nDate:   Sun Mar 30 09:00:00 2026 -0700\n\n    Initial commit\n";
    let cmd = git_cmd("log", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    let commits: Vec<_> = result.entities.iter().filter(|e| e.entity_type == "git_commit").collect();
    assert_eq!(commits.len(), 2);
}

#[test]
fn parse_git_log_oneline() {
    let stdout = "abc1234 Fix the bug\ndef4567 Initial commit\n";
    let cmd = git_cmd("log", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert_eq!(result.entities.iter().filter(|e| e.entity_type == "git_commit").count(), 2);
}

#[test]
fn parse_git_branch() {
    let stdout = "  develop\n* main\n  feature/auth\n";
    let cmd = git_cmd("branch", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    let branches: Vec<_> = result.entities.iter().filter(|e| e.entity_type == "git_branch").collect();
    assert_eq!(branches.len(), 3);
}

#[test]
fn parse_git_branch_with_remotes() {
    let stdout = "* main\n  remotes/origin/main\n  remotes/origin/develop\n";
    let cmd = git_cmd("branch", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    let branches: Vec<_> = result.entities.iter().filter(|e| e.entity_type == "git_branch").collect();
    assert_eq!(branches.len(), 3);
    // Check remote branches have is_remote in their canonical key
    let remote_count = branches.iter().filter(|b| b.canonical_key.ends_with(":true")).count();
    assert_eq!(remote_count, 2);
}

#[test]
fn parse_git_remote_v() {
    let stdout = "origin\tgit@github.com:user/repo.git (fetch)\norigin\tgit@github.com:user/repo.git (push)\nupstream\thttps://github.com/org/repo.git (fetch)\nupstream\thttps://github.com/org/repo.git (push)\n";
    let cmd = git_cmd("remote", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    let remotes: Vec<_> = result.entities.iter().filter(|e| e.entity_type == "git_remote").collect();
    assert_eq!(remotes.len(), 2, "should deduplicate fetch/push");
}

#[test]
fn parse_git_diff_stat() {
    let stdout = " src/main.rs | 10 ++++------\n src/lib.rs  |  3 +++\n 2 files changed, 7 insertions(+), 6 deletions(-)\n";
    let cmd = git_cmd("diff", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert_eq!(result.entities.iter().filter(|e| e.entity_type == "git_file").count(), 2);
}

#[test]
fn parse_git_tag() {
    let stdout = "v0.1.0\nv0.2.0\nv1.0.0\n";
    let cmd = git_cmd("tag", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert_eq!(result.entities.iter().filter(|e| e.entity_type == "git_tag").count(), 3);
}

#[test]
fn parse_git_stash_list() {
    let stdout = "stash@{0}: WIP on main: abc1234 Fix bug\nstash@{1}: On develop: save progress\n";
    let cmd = git_cmd("stash", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert_eq!(result.entities.iter().filter(|e| e.entity_type == "git_stash").count(), 2);
}

#[test]
fn empty_stdout_returns_empty_extraction() {
    let cmd = git_cmd("status", "");
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(result.is_empty());
}

#[test]
fn repo_entity_created() {
    let stdout = "* main\n";
    let cmd = git_cmd("branch", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(result.entities.iter().any(|e| e.entity_type == "git_repo"), "should create implicit git_repo entity");
}

#[test]
fn belongs_to_relationships_created() {
    let stdout = "* main\n  develop\n";
    let cmd = git_cmd("branch", stdout);
    let ext = GitExtractor;
    let result = ext.extract(&cmd).unwrap();
    let branch_count = result.entities.iter().filter(|e| e.entity_type == "git_branch").count();
    let belongs_to_count = result.relationships.iter().filter(|r| r.relation_type == "belongs_to").count();
    assert_eq!(belongs_to_count, branch_count, "each branch should have a belongs_to relationship");
}
