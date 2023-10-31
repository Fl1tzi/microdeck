use crate::type_definition::PrettyPrint;
use dirs::config_dir;
use serde::Deserialize;
use serde_json;
use std::{
    collections::HashMap,
    env,
    fmt::{self, Display},
    fs,
    io::ErrorKind,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};
use tracing::debug;

/// The name of the folder which holds the config
pub const CONFIG_FOLDER_NAME: &'static str = "microdeck";
pub const CONFIG_FILE: &'static str = "config.json";

/// Combination of buttons acting as a folder which a device can switch to
pub type Space = Vec<Arc<Button>>;

/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// CONFIGURATION DEFINITION
/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// CONFIGURATION
#[derive(Deserialize, Debug)]
pub struct Config {
    pub global: Option<GlobalConfig>,
    pub devices: Vec<DeviceConfig>,
    pub spaces: Arc<HashMap<String, Space>>,
}

/// settings that effect all devices
#[derive(Deserialize, Debug)]
pub struct GlobalConfig {
    pub font_family: Option<String>,
}

/// configuration of a single device with its default page
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
    #[serde(default = "new_hashmap")]
    pub options: HashMap<String, String>,
    /// allows to overwrite what it will do on a click
    pub on_click: Option<String>,
    /// allows to overwrite what it will do on a release
    pub on_release: Option<String>,
}

fn new_hashmap() -> HashMap<String, String> {
    HashMap::new()
}

/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// BUTTON CONFIGURATION ERRORS
/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub enum ButtonConfigError {
    /// (Key, Expected)
    WrongType(String, &'static str),
    /// a required value
    Required(&'static str),
    /// A general error which gets directly outputed to the user
    General(String),
}

impl Display for ButtonConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ButtonConfigError::WrongType(key, expected) => {
                write!(
                    formatter,
                    "Expected value of type {expected} in option \"{key}\"."
                )
            }
            ButtonConfigError::Required(key) => {
                write!(formatter, "A value for the option \"{key}\" is required.")
            }
            ButtonConfigError::General(message) => {
                write!(formatter, "{message}")
            }
        }
    }
}

/// See [parse_button_config()]
pub enum ParseButtonConfigResult<T: FromStr> {
    /// (user-defined value, key)
    Found(T, &'static str),
    /// (alternative value, key)
    NotFound(T, &'static str),
    ParseError(ButtonConfigError),
}

impl<T: FromStr> ParseButtonConfigResult<T> {
    /// instead of the enum return Result<Value, ButtonConfigError>.
    ///
    /// [ParseButtonConfigResult::Found] | [ParseButtonConfigResult::NotFound] => Ok(value)
    /// [ParseButtonConfigResult::ParseError] => Err(e)
    pub fn res(self) -> Result<T, ButtonConfigError> {
        match self {
            ParseButtonConfigResult::Found(v, _) | ParseButtonConfigResult::NotFound(v, _) => Ok(v),
            ParseButtonConfigResult::ParseError(e) => Err(e),
        }
    }

    /// instead of the enum return Result<Value, ButtonConfigError>
    ///
    /// This makes a user-defined value required.
    ///
    /// [ParseButtonConfigResult::Found] => Ok(value)
    /// [ParseButtonConfigResult::NotFound] | [ParseButtonConfigResult::ParseError] => Err(e)
    pub fn required(self) -> Result<T, ButtonConfigError> {
        match self {
            ParseButtonConfigResult::Found(v, _) => Ok(v),
            ParseButtonConfigResult::NotFound(_, k) => Err(ButtonConfigError::Required(k)),
            ParseButtonConfigResult::ParseError(e) => Err(e),
        }
    }
}

impl Button {
    /// reads a key from the config and parses the config in the given type
    ///
    /// # Return
    ///
    /// - ParseButtonConfigResult::Found(value) -> returns the value found in the configuration; value is
    /// parsing result
    /// - ParseButtonConfigResult::NotFound(value) -> the key was not found in the configuration; value
    /// is 'if_wrong_type'
    /// - ParseButtonConfigResult::ParseError(error) -> the value could not be parsed; error is
    /// [ButtonConfigError::WrongType]
    pub fn parse_module<T>(&self, key: &'static str, if_wrong_type: T) -> ParseButtonConfigResult<T>
    where
        T: PrettyPrint + FromStr,
    {
        // try to find value or return None
        let parse_result = match self.options.get(key) {
            Some(value) => value.parse::<T>(),
            _ => return ParseButtonConfigResult::NotFound(if_wrong_type, key),
        };
        // check if value could be parsed
        if let Ok(out) = parse_result {
            return ParseButtonConfigResult::Found(out, key);
        }
        ParseButtonConfigResult::ParseError(ButtonConfigError::WrongType(
            key.to_string(),
            if_wrong_type.pprint(),
        ))
    }

    /// Just retrieve the raw string from a key without trying to parse the value
    pub fn raw_module(&self, key: &String) -> Option<&String> {
        self.options.get(key)
    }
}

/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// PARSING CONFIG
/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

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

/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////
// OVERALL CONFIGURATION ERRORS
/////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////////

/// General error for parsing the configuration
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
