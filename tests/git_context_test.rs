use redtrail::core::capture;

#[test]
fn detect_git_repo_in_git_directory() {
    // We're running from the redtrail repo itself, so this should work
    let cwd = std::env::current_dir().unwrap();
    let ctx = capture::git_context(cwd.to_str().unwrap());

    assert!(ctx.repo.is_some(), "should detect git repo root");
    assert!(ctx.branch.is_some(), "should detect git branch");
}

#[test]
fn detect_git_repo_returns_none_outside_repo() {
    let ctx = capture::git_context("/tmp");

    assert!(ctx.repo.is_none(), "should return None outside git repo");
    assert!(
        ctx.branch.is_none(),
        "should return None for branch outside git repo"
    );
}

#[test]
fn git_context_struct_has_repo_and_branch() {
    let ctx = capture::GitContext {
        repo: Some("/home/user/project".to_string()),
        branch: Some("main".to_string()),
    };
    assert_eq!(ctx.repo.as_deref(), Some("/home/user/project"));
    assert_eq!(ctx.branch.as_deref(), Some("main"));
}
