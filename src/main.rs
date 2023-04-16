use deck_driver as streamdeck;
use device::Device;
use hidapi::HidApi;
use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    fmt::{self, Display},
    fs,
    io::ErrorKind,
    path::PathBuf,
    process::exit,
    time::Duration,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{
    self, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

use dirs::config_dir;

mod device;
mod modules;

/// The name of the folder which holds the config
pub const CONFIG_FOLDER_NAME: &'static str = "dach-decker";

/// The config structure
#[derive(Deserialize, Debug)]
pub struct Config {
    global: Option<GlobalConfig>,
    device: Vec<DeviceConfig>,
}

#[derive(Deserialize, Debug)]
struct GlobalConfig;

fn main() {
    // ------ LOAD CONFIG

    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();

    let config_file: PathBuf = match env::var_os("DACH_DECKER_CONFIG") {
        Some(path) => PathBuf::from(path),
        None => {
            if let Some(mut path) = config_dir() {
                path.push(CONFIG_FOLDER_NAME);
                path.push("config.toml");
                path
            } else {
                error!("Please use the \"DACH_DECKER_CONFIG\" environment variable to provide a path to your config");
                exit(1);
            }
        }
    };

    info!("Loading configuration from \"{}\"", config_file.display());

    let config: Config = match fs::read_to_string(config_file) {
        Ok(content) => match toml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                error!("Error detected in configuration:\n{}", e);
                exit(1);
            }
        },
        Err(file_error) => {
            if file_error.kind() == ErrorKind::NotFound {
                error!("Unable to load configuration because the file does not exist. Please create the configuration file.");
            } else {
                error!("Cannot open the configuration file: {}", file_error);
            }
            exit(1);
        }
    };
    debug!("{:#?}", config);

    // ------ START APPLICATION

    let hid = match streamdeck::new_hidapi() {
        Ok(v) => v,
        Err(e) => {
            error!("HidApi Error:\n{}", e);
            exit(1);
        }
    };
    // lets start some async
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(start(config, hid))
}

pub async fn start(config: Config, mut hid: HidApi) {
    let mut devices: HashMap<String, Device> = HashMap::new();

    // devices which are not configured anyways
    let mut ignore_devices: Vec<String> = Vec::new();

    loop {
        // check for devices that can be removed
        let mut removable_devices = Vec::new();
        for (key, device) in &devices {
            if device.is_dropped() {
                removable_devices.push(key.to_owned());
            }
        }
        for d in removable_devices {
            devices.remove(&d);
        }

        // refresh device list
        if let Err(e) = streamdeck::refresh_device_list(&mut hid) {
            warn!("Cannot fetch new devices: {}", e);
        } else {
            for hw_device in streamdeck::list_devices(&hid) {
                // if the device is not ignored and device is not already started
                if !ignore_devices.contains(&hw_device.1) && devices.get(&hw_device.1).is_none() {
                    debug!("New device detected: {}", &hw_device.1);
                    if let Some(device_config) =
                        config.device.iter().find(|d| d.serial == hw_device.1)
                    {
                        // start the device and its listener
                        if let Some(mut device) = start_device(hw_device, &hid, device_config).await
                        {
                            device.key_listener().await;
                            devices.insert(device.serial(), device);
                        }
                    } else {
                        info!("The device {} is not configured.", hw_device.1);
                        ignore_devices.push(hw_device.1);
                    }
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

#[tracing::instrument(name = "device", skip_all, fields(serial = device.1))]
pub async fn start_device(
    device: (streamdeck::info::Kind, String),
    hid: &HidApi,
    device_config: &DeviceConfig,
) -> Option<Device> {
    match Device::new(device.1, device.0, device_config, &hid).await {
        Ok(mut d) => {
            info!("Connected");
            // start all modules
            for button in device_config.buttons.iter() {
                if let Err(e) = d.start_button(&button) {
                    error!("{}", e)
                }
            }
            Some(d)
        }
        Err(e) => {
            error!("Unable to connect: {}", e);
            None
        }
    }
}

pub enum ConfigError {
    ButtonDoesNotExist(u8),
    ModuleDoesNotExist(u8, String),
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
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct DeviceConfig {
    pub serial: String,
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    pub buttons: Vec<Button>,
}

fn default_brightness() -> u8 {
    100
}

#[derive(Deserialize, Debug, Clone)]
pub struct Button {
    index: u8,
    module: String,
    /// options which get passed to the module
    options: Option<HashMap<String, String>>,
    /// allows to overwrite what it will do on a click
    /// available options:
    /// - \"sh:date\" - executes in sh
    /// - \"bash:date\" - executes in bash
    pub on_click: Option<String>,
    /// allows to overwrite what it will do on a release; Same options as [on_click]
    pub on_release: Option<String>,
}
