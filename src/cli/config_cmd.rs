use clap::Subcommand;
use crate::error::Error;
use crate::config::Config;
use crate::workspace;

#[derive(Subcommand)]
pub enum ConfigCommands {
    List,
    Get {
        key: String,
    },
    Set {
        key: String,
        value: String,
    },
}

pub fn run(cmd: ConfigCommands) -> Result<(), Error> {
    match cmd {
        ConfigCommands::List => {
            let cwd = std::env::current_dir()?;
            let config = if let Some(ws) = workspace::find_workspace(&cwd) {
                Config::resolved(&ws)?
            } else {
                Config::load_global()?
            };
            let toml_str = toml::to_string_pretty(&config)
                .map_err(|e| Error::Config(e.to_string()))?;
            print!("{}", toml_str);
            Ok(())
        }

        ConfigCommands::Get { key } => {
            let cwd = std::env::current_dir()?;
            let config = if let Some(ws) = workspace::find_workspace(&cwd) {
                Config::resolved(&ws)?
            } else {
                Config::load_global()?
            };
            let toml_val = toml::Value::try_from(&config)
                .map_err(|e| Error::Config(e.to_string()))?;

            let parts: Vec<&str> = key.split('.').collect();
            let mut current = &toml_val;
            for part in &parts {
                match current {
                    toml::Value::Table(t) => {
                        current = t.get(*part).ok_or_else(|| Error::Config(format!("key not found: {key}")))?;
                    }
                    _ => return Err(Error::Config(format!("key not found: {key}"))),
                }
            }
            println!("{}", current);
            Ok(())
        }

        ConfigCommands::Set { key, value } => {
            let cwd = std::env::current_dir()?;
            let config_path = if let Some(ws) = workspace::find_workspace(&cwd) {
                workspace::config_path(&ws)
            } else {
                dirs::home_dir()
                    .ok_or_else(|| Error::Config("cannot determine home directory".to_string()))?
                    .join(".redtrail/config.toml")
            };

            let existing = if config_path.exists() {
                std::fs::read_to_string(&config_path)?
            } else {
                String::new()
            };

            let mut toml_val: toml::Value = if existing.is_empty() {
                toml::Value::Table(toml::map::Map::new())
            } else {
                toml::from_str(&existing).map_err(|e| Error::Config(e.to_string()))?
            };

            let parts: Vec<&str> = key.split('.').collect();
            let parsed_value: toml::Value = value.parse::<i64>()
                .map(toml::Value::Integer)
                .or_else(|_| value.parse::<f64>().map(toml::Value::Float))
                .or_else(|_| value.parse::<bool>().map(toml::Value::Boolean))
                .unwrap_or_else(|_| toml::Value::String(value.clone()));

            set_nested(&mut toml_val, &parts, parsed_value)?;

            let out = toml::to_string_pretty(&toml_val)
                .map_err(|e| Error::Config(e.to_string()))?;

            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&config_path, out)?;
            println!("set {key} = {value}");
            Ok(())
        }
    }
}

fn set_nested(val: &mut toml::Value, parts: &[&str], new_val: toml::Value) -> Result<(), Error> {
    if parts.is_empty() { return Ok(()); }
    match val {
        toml::Value::Table(t) => {
            if parts.len() == 1 {
                t.insert(parts[0].to_string(), new_val);
            } else {
                let entry = t.entry(parts[0].to_string())
                    .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
                set_nested(entry, &parts[1..], new_val)?;
            }
            Ok(())
        }
        _ => Err(Error::Config(format!("cannot set key on non-table value at '{}'", parts[0]))),
    }
}
