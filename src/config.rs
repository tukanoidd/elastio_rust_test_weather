use std::io::Write;
use std::path::PathBuf;

use color_eyre::eyre;

use crate::{built_info, providers::Provider};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Config {
    pub(crate) provider: Provider,

    #[serde(skip)]
    file_path: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            provider: Provider::OpenMeteo,
            file_path: PathBuf::new(),
        }
    }
}

impl Config {
    pub(crate) fn new() -> eyre::Result<Self> {
        // Get system config directory
        let config_dir =
            dirs::config_dir().ok_or(eyre::eyre!("Could not find config directory"))?;
        // Create a path to the weather cli config directory
        let weather_config_dir = config_dir.join(built_info::PKG_NAME);

        // Create the weather cli config directory if it doesn't exist
        if !weather_config_dir.exists() {
            std::fs::create_dir_all(&weather_config_dir)?;
        }

        // Create a path to the weather cli config file
        let weather_config_file_path = weather_config_dir.join("config.json");

        // Check if the config file exists
        let mut config = match weather_config_file_path.exists() {
            // If it does, read it, parse th data and return the config struct
            true => serde_json::from_str(&std::fs::read_to_string(&weather_config_file_path)?)?,
            false => {
                // If it doesn't create a default config
                let default_config = Self::default();
                // And serialize it into json format
                let default_config_json = serde_json::to_string_pretty(&default_config)?;

                // Create the config file
                let mut config_file = std::fs::File::create(&weather_config_file_path)?;

                // Write the default config data to the config file
                config_file.write_all(default_config_json.as_bytes())?;

                // Return the default config
                default_config
            }
        };

        config.file_path = weather_config_file_path;

        Ok(config)
    }

    pub(crate) fn save(&self) -> eyre::Result<()> {
        // Serialize the config struct into json format
        let config_json = serde_json::to_string_pretty(&self)?;

        // Create the config file
        let mut config_file = std::fs::File::create(&self.file_path)?;

        // Write the config data to the config file
        config_file.write_all(config_json.as_bytes())?;

        Ok(())
    }
}
