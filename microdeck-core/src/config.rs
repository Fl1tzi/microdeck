use serde::Deserialize;
use dirs::config_dir;
use std::{
    env,
    fmt::{self, Display},
    fs,
    io::ErrorKind,
    path::PathBuf,
    collections::HashMap,
    sync::Arc
};
use tracing::debug;
use serde_json;

/// The name of the folder which holds the config
pub const CONFIG_FOLDER_NAME: &'static str = "microdeck";
pub const CONFIG_FILE: &'static str = "config.json";

#[derive(Deserialize, Debug)]
pub struct Config {
    pub global: Option<GlobalConfig>,
    pub devices: Vec<DeviceConfig>,
}

#[derive(Deserialize, Debug)]
pub struct GlobalConfig;

#[derive(Deserialize, Debug, Clone)]
pub struct DeviceConfig {
    pub serial: String,
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    pub buttons: Vec<Arc<Button>>,
}

fn default_brightness() -> u8 {
    100
}

#[derive(Deserialize, Debug, Clone)]
pub struct Button {
    pub index: u8,
    pub module: String,
    /// options which get passed to the module
    pub options: Option<HashMap<String, String>>,
    /// allows to overwrite what it will do on a click (executes in /bin/sh)
    pub on_click: Option<String>,
    /// allows to overwrite what it will do on a release
    pub on_release: Option<String>,
}

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
                path.push(CONFIG_FILE);
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
            serde_json::from_str(&content).map_err(|e| ConfigError::SyntaxError(e.to_string()))
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
