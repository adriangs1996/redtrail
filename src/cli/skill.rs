use crate::error::Error;
use crate::skill_loader::KNOWN_TOOL_NAMES;
use clap::Subcommand;
use std::fs;
use std::path::Path;

#[derive(Subcommand)]
pub enum SkillCommands {
    #[command(about = "Scaffold a new skill directory with skill.toml and prompt.md")]
    Init {
        #[arg(help = "Skill name (becomes the directory name)")]
        name: String,
    },
    #[command(about = "Validate a skill's structure and required fields")]
    Test {
        #[arg(help = "Path to skill directory")]
        path: String,
    },
    #[command(about = "List all installed skills")]
    List,
    #[command(about = "Install a skill from a local directory into ~/.redtrail/skills/")]
    Install {
        #[arg(help = "Path to skill directory to install")]
        path: String,
    },
    #[command(about = "Remove an installed skill by name")]
    Remove {
        #[arg(help = "Name of the skill to remove")]
        name: String,
    },
}

pub fn run(cmd: SkillCommands) -> Result<(), Error> {
    match cmd {
        SkillCommands::Init { name } => init_skill(&name),
        SkillCommands::Test { path } => test_skill(&path),
        SkillCommands::List => list_skills(),
        SkillCommands::Install { path } => install_skill(&path),
        SkillCommands::Remove { name } => remove_skill(&name),
    }
}

fn skills_dir() -> Result<std::path::PathBuf, Error> {
    let dir = dirs::home_dir()
        .ok_or(Error::Config("no home dir".into()))?
        .join(".redtrail/skills");
    Ok(dir)
}

fn init_skill(name: &str) -> Result<(), Error> {
    let dir = Path::new(name);
    if dir.exists() {
        return Err(Error::Config(format!("{name} already exists")));
    }
    fs::create_dir_all(dir)?;

    let skill_toml = format!(
        r#"[skill]
name = "{name}"
version = "0.1.0"
description = ""
author = ""

[triggers]
keywords = []

# tools = ["query_table", "create_record", "update_record", "run_command", "suggest", "respond"]

[dependencies]
commands = []
rt_commands = []
"#
    );
    fs::write(dir.join("skill.toml"), skill_toml)?;
    fs::write(
        dir.join("prompt.md"),
        format!("# {name}\n\nYour skill prompt here.\n"),
    )?;
    println!("skill scaffolded: {name}/");
    println!("  skill.toml — metadata");
    println!("  prompt.md  — prompt content");
    Ok(())
}

fn test_skill(path: &str) -> Result<(), Error> {
    let dir = Path::new(path);
    let mut errors = Vec::new();

    let toml_path = dir.join("skill.toml");
    if !toml_path.exists() {
        errors.push("skill.toml not found".to_string());
    } else {
        let content = fs::read_to_string(&toml_path)?;
        match toml::from_str::<toml::Value>(&content) {
            Ok(val) => {
                let has_name = val.get("skill").and_then(|s| s.get("name")).is_some()
                    || val.get("name").is_some();
                if !has_name {
                    errors.push(
                        "skill.toml missing name (either [skill].name or root name)".to_string(),
                    );
                }
                if let Some(tools) = val.get("tools") {
                    if let Some(arr) = tools.as_array() {
                        for t in arr {
                            if let Some(name) = t.as_str() {
                                if !KNOWN_TOOL_NAMES.contains(&name) {
                                    errors.push(format!(
                                        "unknown tool in tools array: \"{name}\" (known: {})",
                                        KNOWN_TOOL_NAMES.join(", ")
                                    ));
                                }
                            } else {
                                errors.push("tools array must contain strings".to_string());
                            }
                        }
                    } else {
                        errors.push("tools field must be an array".to_string());
                    }
                }
            }
            Err(e) => errors.push(format!("skill.toml parse error: {e}")),
        }
    }

    let prompt_path = dir.join("prompt.md");
    if !prompt_path.exists() {
        errors.push("prompt.md not found".to_string());
    } else {
        let content = fs::read_to_string(&prompt_path)?;
        if content.trim().is_empty() {
            errors.push("prompt.md is empty".to_string());
        }
    }

    if errors.is_empty() {
        println!("skill valid: {path}");
        Ok(())
    } else {
        for e in &errors {
            eprintln!("error: {e}");
        }
        Err(Error::Config(format!("{} validation errors", errors.len())))
    }
}

fn list_skills() -> Result<(), Error> {
    let dir = skills_dir()?;
    if !dir.exists() {
        println!("no skills installed");
        return Ok(());
    }
    let mut found = false;
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let toml_path = entry.path().join("skill.toml");
        if !toml_path.exists() {
            continue;
        }
        if let Ok(content) = fs::read_to_string(&toml_path)
            && let Ok(val) = toml::from_str::<toml::Value>(&content)
        {
            let skill_section = val.get("skill");
            let name = skill_section
                .and_then(|s| s.get("name"))
                .or(val.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let version = skill_section
                .and_then(|s| s.get("version"))
                .or(val.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let desc = skill_section
                .and_then(|s| s.get("description"))
                .or(val.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!("{name} ({version}) — {desc}");
            found = true;
        }
    }
    if !found {
        println!("no skills installed");
    }
    Ok(())
}

fn install_skill(path: &str) -> Result<(), Error> {
    test_skill(path)?;
    let src = Path::new(path);
    let toml_content = fs::read_to_string(src.join("skill.toml"))?;
    let val: toml::Value =
        toml::from_str(&toml_content).map_err(|e| Error::Config(e.to_string()))?;
    let skill_section = val.get("skill");
    let name = skill_section
        .and_then(|s| s.get("name"))
        .or(val.get("name"))
        .and_then(|v| v.as_str())
        .ok_or(Error::Config("missing skill name".into()))?;

    let dest = skills_dir()?.join(name);
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(&dest)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        fs::copy(entry.path(), target)?;
    }
    println!("installed: {name} -> {}", dest.display());
    Ok(())
}

fn remove_skill(name: &str) -> Result<(), Error> {
    let dir = skills_dir()?.join(name);
    if !dir.exists() {
        return Err(Error::Config(format!("skill '{name}' not installed")));
    }
    fs::remove_dir_all(&dir)?;
    println!("removed: {name}");
    Ok(())
}
