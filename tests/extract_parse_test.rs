use redtrail::extract::parse::split_segments;

#[test]
fn single_command() {
    let segs = split_segments("git status");
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].raw, "git status");
}

#[test]
fn pipe_splits() {
    let segs = split_segments("git log --oneline | head -5");
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].raw, "git log --oneline");
    assert_eq!(segs[1].raw, "head -5");
}

#[test]
fn chain_and_splits() {
    let segs = split_segments("git add . && git commit -m \"done\"");
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].raw, "git add .");
    assert_eq!(segs[1].raw, "git commit -m \"done\"");
}

#[test]
fn no_split_inside_quotes() {
    let segs = split_segments("echo \"hello | world && foo\"");
    assert_eq!(segs.len(), 1);
}

#[test]
fn redirect_stripped() {
    let segs = split_segments("cargo build 2>&1 > /dev/null");
    assert_eq!(segs.len(), 1);
    // After stripping redirects, should just have the command
    assert!(!segs[0].raw.contains(">"), "raw should not contain > after stripping");
    assert!(segs[0].raw.contains("cargo build"));
}

#[test]
fn semicolon_splits() {
    let segs = split_segments("cd /tmp; ls -la");
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].raw, "cd /tmp");
    assert_eq!(segs[1].raw, "ls -la");
}

#[test]
fn mixed_pipe_and_chain() {
    let segs = split_segments("docker build -t app . && docker push app | tee log.txt");
    assert_eq!(segs.len(), 3);
}

#[test]
fn empty_command() {
    let segs = split_segments("");
    assert_eq!(segs.len(), 0);
}

#[test]
fn or_chain_splits() {
    let segs = split_segments("test -f file.txt || echo missing");
    assert_eq!(segs.len(), 2);
}

#[test]
fn single_quotes_respected() {
    let segs = split_segments("echo 'pipe | here && and ; semi'");
    assert_eq!(segs.len(), 1);
}

#[test]
fn escaped_pipe_not_split() {
    let segs = split_segments("echo hello \\| world");
    assert_eq!(segs.len(), 1);
}
