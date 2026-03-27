use crate::config::Config;
use crate::error::Error;

pub fn view(config_path: &str) -> Result<(), Error> {
    let config = Config::load(config_path)?;
    let yaml = serde_yaml::to_string(&config)
        .map_err(|e| Error::Config(format!("serialize error: {e}")))?;
    print!("{yaml}");
    Ok(())
}

pub fn set(config_path: &str, key: &str, value: &str) -> Result<(), Error> {
    let mut config = Config::load(config_path)?;
    config.set_value(key, value)?;
    config.save(config_path)?;
    println!("Set {key} = {value}");
    Ok(())
}
