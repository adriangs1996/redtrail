// Command classification for analysis and reporting.

use crate::core::db::CommandRow;

/// Binaries that produce noise — read-only, navigation, or shell builtins.
const NOISE_BINARIES: &[&str] = &[
    "find", "ls", "cd", "cat", "clear", "pwd", "echo", "which", "type", "head", "tail", "less",
    "more", "wc", "bat", "tree", "file", "stat", "du", "df", "env", "printenv", "whoami",
];

/// Claude Code tool names that are read-only noise.
const NOISE_TOOLS: &[&str] = &["Read", "Glob", "Grep", "TodoRead", "WebFetch", "LSP"];

/// Binaries that represent project-level commands (builds, tests, package managers, etc.).
const PROJECT_BINARIES: &[&str] = &[
    "cargo", "npm", "npx", "yarn", "pnpm", "bun", "pip", "pip3", "python", "python3", "go",
    "make", "cmake", "docker", "dotnet", "gcc", "g++", "clang", "javac", "tsc", "webpack",
    "esbuild", "vite", "rollup", "pytest", "jest", "vitest", "mocha", "rspec", "phpunit", "mvn",
    "gradle", "ruby", "rake", "mix", "rustc",
];

/// Returns true if this command is noise — read-only investigation, navigation, or shell builtins.
pub fn is_noise_command(cmd: &CommandRow) -> bool {
    if cmd.tool_name.as_deref().is_some_and(|t| NOISE_TOOLS.contains(&t)) {
        return true;
    }
    if cmd.command_binary.as_deref().is_some_and(|b| NOISE_BINARIES.contains(&b)) {
        return true;
    }
    let category = classify_command(
        cmd.command_binary.as_deref().unwrap_or(""),
        cmd.command_subcommand.as_deref(),
        cmd.tool_name.as_deref(),
    );
    category.is_read_only()
}

/// Returns true if the binary is a project-level command (build, test, package manager, etc.).
pub fn is_project_command(binary: &str) -> bool {
    PROJECT_BINARIES.contains(&binary)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    FileWrite,
    FileRead,
    ShellCommand,
    TestRun,
    Build,
    GitOperation,
    PackageManagement,
}

impl CommandCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::FileWrite => "file_write",
            Self::FileRead => "file_read",
            Self::ShellCommand => "shell_command",
            Self::TestRun => "test_run",
            Self::Build => "build",
            Self::GitOperation => "git_operation",
            Self::PackageManagement => "package_management",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::FileWrite => "File writes",
            Self::FileRead => "File reads",
            Self::ShellCommand => "Shell commands",
            Self::TestRun => "Test runs",
            Self::Build => "Builds",
            Self::GitOperation => "Git operations",
            Self::PackageManagement => "Package management",
        }
    }

    /// Returns true if this category represents a read-only/investigation action.
    pub fn is_read_only(&self) -> bool {
        matches!(self, Self::FileRead)
    }
}

/// Classify a command based on binary name, subcommand, and optional tool_name.
pub fn classify_command(
    binary: &str,
    subcommand: Option<&str>,
    tool_name: Option<&str>,
) -> CommandCategory {
    // Tier 1: tool_name from Claude Code
    if let Some(tool) = tool_name {
        match tool {
            "Write" | "Edit" | "NotebookEdit" => return CommandCategory::FileWrite,
            "Read" | "Glob" | "Grep" => return CommandCategory::FileRead,
            _ => {}
        }
    }

    let sub = subcommand.unwrap_or("");

    match binary {
        "git" => CommandCategory::GitOperation,
        "pytest" | "jest" | "vitest" | "mocha" | "rspec" | "phpunit" => CommandCategory::TestRun,
        "make" | "cmake" | "tsc" | "gcc" | "g++" | "clang" | "javac" | "webpack" | "esbuild"
        | "vite" | "rollup" => CommandCategory::Build,
        "brew" | "apt" | "apt-get" | "dnf" | "yum" | "pacman" | "nix" => {
            CommandCategory::PackageManagement
        }
        "cargo" => match sub {
            "test" | "nextest" => CommandCategory::TestRun,
            "build" | "check" | "clippy" => CommandCategory::Build,
            "add" | "remove" | "install" | "update" => CommandCategory::PackageManagement,
            _ => CommandCategory::ShellCommand,
        },
        "npm" | "npx" | "yarn" | "pnpm" | "bun" => match sub {
            "test" => CommandCategory::TestRun,
            "build" => CommandCategory::Build,
            "install" | "add" | "remove" | "uninstall" | "update" | "upgrade" | "ci" => {
                CommandCategory::PackageManagement
            }
            _ => CommandCategory::ShellCommand,
        },
        "pip" | "pip3" | "pipenv" | "poetry" | "uv" => match sub {
            "install" | "uninstall" | "update" | "add" | "remove" | "sync" | "lock" => {
                CommandCategory::PackageManagement
            }
            _ => CommandCategory::ShellCommand,
        },
        "go" => match sub {
            "test" => CommandCategory::TestRun,
            "build" | "install" => CommandCategory::Build,
            "get" | "mod" => CommandCategory::PackageManagement,
            _ => CommandCategory::ShellCommand,
        },
        "dotnet" => match sub {
            "test" => CommandCategory::TestRun,
            "build" | "publish" => CommandCategory::Build,
            "add" | "restore" => CommandCategory::PackageManagement,
            _ => CommandCategory::ShellCommand,
        },
        "cat" | "head" | "tail" | "less" | "more" | "bat" | "wc" => CommandCategory::FileRead,
        _ => CommandCategory::ShellCommand,
    }
}
