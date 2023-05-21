use crate::Config;
use dirs::config_dir;
use std::{
    env,
    fmt::{self, Display},
    fs,
    io::ErrorKind,
    path::PathBuf,
};
use tracing::debug;

/// The name of the folder which holds the config
pub const CONFIG_FOLDER_NAME: &'static str = "virtual-deck";

#[tracing::instrument]
pub fn load_config() -> Result<Config, ConfigError> {
    let config_file: PathBuf = match env::var_os("DACH_DECKER_CONFIG") {
        Some(path) => {
            debug!("Using env variable: {:?}", path);
            PathBuf::from(path)
        }
        None => {
            // try to get the system config dir; env var required if not available
            if let Some(mut path) = config_dir() {
                path.push(CONFIG_FOLDER_NAME);
                path.push("config.toml");
                debug!("Using system path: {:?}", path);
                path
            } else {
                return Err(ConfigError::PathNotAvailable());
            }
        }
    };

    let path = config_file.display().to_string().clone();

    match fs::read_to_string(config_file) {
        Ok(content) => {
            toml::from_str(&content).map_err(|e| ConfigError::SyntaxError(e.to_string()))
        }
        Err(file_error) => {
            if file_error.kind() == ErrorKind::NotFound {
                return Err(ConfigError::FilePathDoesNotExist(path));
            } else {
                return Err(ConfigError::ReadError(file_error.to_string()));
            }
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    ButtonDoesNotExist(u8),
    ModuleDoesNotExist(u8, String),
    PathNotAvailable(),
    SyntaxError(String),
    FilePathDoesNotExist(String),
    ReadError(String),
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::ButtonDoesNotExist(index) => {
                write!(formatter, "Button {}: does not exist.", index)
            }
            ConfigError::ModuleDoesNotExist(index, module) => write!(
                formatter,
                "Button {}: The module \"{}\" does not exist.",
                index, module
            ),
            ConfigError::PathNotAvailable() => write!(
                formatter,
                "Config directory not available. Please use the environment variable \"DACH_DECKER_CONFIG\" to specify the location of the config."
            ),
            ConfigError::SyntaxError(text) => write!(
                formatter,
                "Syntax error in configuration:\n{}",
                text
            ),
            ConfigError::FilePathDoesNotExist(path) => write!(
                formatter,
                "The configuration file does not exist in {}",
                path
            ),
            ConfigError::ReadError(error) => write!(
                formatter,
                "Could not read the configuration file: {}",
                error
            )
        }
    }
}
