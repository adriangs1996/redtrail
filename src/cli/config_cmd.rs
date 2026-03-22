use crate::config::Config;
use crate::error::Error;
use crate::resolve;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum ConfigCommands {
    #[command(about = "Display the resolved configuration")]
    List,
    #[command(about = "Get a config value by dot-notation key (e.g. general.autonomy)")]
    Get {
        #[arg(help = "Dot-notation key (e.g. general.autonomy, tools.aliases)")]
        key: String,
    },
    #[command(about = "Set a config value")]
    Set {
        #[arg(help = "Dot-notation key")]
        key: String,
        #[arg(help = "Value to set")]
        value: String,
        #[arg(long, help = "Write to global config instead of session config")]
        global: bool,
    },
}

pub fn run(cmd: ConfigCommands) -> Result<(), Error> {
    match cmd {
        ConfigCommands::List => {
            let config = resolve_config()?;
            let toml_str =
                toml::to_string_pretty(&config).map_err(|e| Error::Config(e.to_string()))?;
            print!("{}", toml_str);
            Ok(())
        }

        ConfigCommands::Get { key } => {
            let config = resolve_config()?;
            match config.get_key(&key) {
                Some(val) => {
                    println!("{val}");
                    Ok(())
                }
                None => Err(Error::Config(format!("key not found: {key}"))),
            }
        }

        ConfigCommands::Set { key, value, global } => {
            let cwd = std::env::current_dir()?;

            if global {
                let ctx = resolve::resolve_global()?;
                crate::db::config::set_global_config(&ctx.conn, &key, &value)?;
            } else {
                match resolve::resolve(&cwd) {
                    Ok(ctx) => {
                        crate::db::config::set_session_config(
                            &ctx.conn,
                            &ctx.session_id,
                            &key,
                            &value,
                        )?;
                    }
                    Err(_) => {
                        let ctx = resolve::resolve_global()?;
                        crate::db::config::set_global_config(&ctx.conn, &key, &value)?;
                    }
                }
            }
            println!("set {key} = {value}");
            Ok(())
        }
    }
}

fn resolve_config() -> Result<Config, Error> {
    let cwd = std::env::current_dir()?;
    match resolve::resolve(&cwd) {
        Ok(ctx) => Ok(ctx.config),
        Err(_) => {
            let ctx = resolve::resolve_global()?;
            Ok(Config::resolved_global(&ctx.conn)?)
        }
    }
}
