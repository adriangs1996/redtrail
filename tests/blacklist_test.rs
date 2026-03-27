use redtrail::core::capture;

#[test]
fn default_blacklist_blocks_vim() {
    assert!(capture::is_blacklisted("vim", &capture::default_blacklist()));
}

#[test]
fn default_blacklist_blocks_ssh() {
    assert!(capture::is_blacklisted("ssh", &capture::default_blacklist()));
}

#[test]
fn default_blacklist_blocks_interactive_tools() {
    let bl = capture::default_blacklist();
    for cmd in &["vim", "nvim", "nano", "ssh", "top", "htop", "less", "more", "man", "tmux", "screen"] {
        assert!(capture::is_blacklisted(cmd, &bl), "{cmd} should be blacklisted");
    }
}

#[test]
fn non_blacklisted_commands_pass() {
    let bl = capture::default_blacklist();
    for cmd in &["ls", "git", "cargo", "echo", "docker", "curl"] {
        assert!(!capture::is_blacklisted(cmd, &bl), "{cmd} should NOT be blacklisted");
    }
}

#[test]
fn custom_blacklist_entry() {
    let mut bl = capture::default_blacklist();
    bl.push("python".to_string());
    assert!(capture::is_blacklisted("python", &bl));
}

#[test]
fn blacklist_checks_binary_not_full_command() {
    let bl = capture::default_blacklist();
    // "vim" is blacklisted, but "vim --version" should still match on binary "vim"
    assert!(capture::is_blacklisted("vim", &bl));
    // "ls" is not blacklisted even if args mention "vim"
    assert!(!capture::is_blacklisted("ls", &bl));
}

#[test]
fn extracts_binary_from_command_string() {
    assert_eq!(capture::extract_binary("git status"), "git");
    assert_eq!(capture::extract_binary("  ls -la /tmp  "), "ls");
    assert_eq!(capture::extract_binary("sudo apt install foo"), "sudo");
    assert_eq!(capture::extract_binary(""), "");
}
