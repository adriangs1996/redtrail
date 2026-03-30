use redtrail::core::classify::{classify_command, CommandCategory};

#[test]
fn classify_git_operations() {
    assert_eq!(classify_command("git", Some("commit"), None), CommandCategory::GitOperation);
    assert_eq!(classify_command("git", Some("push"), None), CommandCategory::GitOperation);
    assert_eq!(classify_command("git", Some("diff"), None), CommandCategory::GitOperation);
    assert_eq!(classify_command("git", Some("status"), None), CommandCategory::GitOperation);
}

#[test]
fn classify_test_runs() {
    assert_eq!(classify_command("cargo", Some("test"), None), CommandCategory::TestRun);
    assert_eq!(classify_command("npm", Some("test"), None), CommandCategory::TestRun);
    assert_eq!(classify_command("pytest", None, None), CommandCategory::TestRun);
    assert_eq!(classify_command("jest", None, None), CommandCategory::TestRun);
    assert_eq!(classify_command("go", Some("test"), None), CommandCategory::TestRun);
}

#[test]
fn classify_build_commands() {
    assert_eq!(classify_command("cargo", Some("build"), None), CommandCategory::Build);
    assert_eq!(classify_command("make", None, None), CommandCategory::Build);
    assert_eq!(classify_command("tsc", None, None), CommandCategory::Build);
    assert_eq!(classify_command("gcc", None, None), CommandCategory::Build);
}

#[test]
fn classify_package_management() {
    assert_eq!(classify_command("npm", Some("install"), None), CommandCategory::PackageManagement);
    assert_eq!(classify_command("pip", Some("install"), None), CommandCategory::PackageManagement);
    assert_eq!(classify_command("cargo", Some("add"), None), CommandCategory::PackageManagement);
    assert_eq!(classify_command("yarn", Some("add"), None), CommandCategory::PackageManagement);
    assert_eq!(classify_command("brew", Some("install"), None), CommandCategory::PackageManagement);
}

#[test]
fn classify_file_operations_from_tool_name() {
    assert_eq!(classify_command("Write", None, Some("Write")), CommandCategory::FileWrite);
    assert_eq!(classify_command("Edit", None, Some("Edit")), CommandCategory::FileWrite);
    assert_eq!(classify_command("Read", None, Some("Read")), CommandCategory::FileRead);
    assert_eq!(classify_command("Glob", None, Some("Glob")), CommandCategory::FileRead);
    assert_eq!(classify_command("Grep", None, Some("Grep")), CommandCategory::FileRead);
}

#[test]
fn classify_file_read_from_binary() {
    assert_eq!(classify_command("cat", None, None), CommandCategory::FileRead);
    assert_eq!(classify_command("head", None, None), CommandCategory::FileRead);
    assert_eq!(classify_command("tail", None, None), CommandCategory::FileRead);
    assert_eq!(classify_command("less", None, None), CommandCategory::FileRead);
}

#[test]
fn classify_unknown_defaults_to_shell() {
    assert_eq!(classify_command("curl", None, None), CommandCategory::ShellCommand);
    assert_eq!(classify_command("docker", Some("run"), None), CommandCategory::ShellCommand);
}

#[test]
fn classify_npm_build_as_build() {
    assert_eq!(classify_command("npm", Some("build"), None), CommandCategory::Build);
    assert_eq!(classify_command("npm", Some("run"), None), CommandCategory::ShellCommand);
}
