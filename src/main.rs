use hidapi::HidApi;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;
use std::{env, path::PathBuf};
use streamdeck::StreamDeckError;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace, warn};
mod modules;
use elgato_streamdeck as streamdeck;
use streamdeck::asynchronous::{AsyncStreamDeck, ButtonStateUpdate};

use dirs::config_dir;

use crate::modules::{retrieve_module_from_name, start_module, HostEvent};

/// The name of the folder which holds the config
pub const CONFIG_FOLDER_NAME: &'static str = "dach-decker";

macro_rules! skip_if_none {
    ($res:expr) => {
        match $res {
            Some(v) => v,
            None => continue,
        }
    };
}

/// The config structure
#[derive(Deserialize, Debug)]
pub struct Config {
    global: Option<GlobalConfig>,
    device: Vec<DeviceConfig>,
}

#[derive(Deserialize, Debug)]
struct GlobalConfig;

fn main() {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

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
    // hidapi
    let hid = match streamdeck::new_hidapi() {
        Ok(v) => v,
        Err(e) => {
            error!("HidApi Error:\n{}", e);
            exit(1);
        }
    };
    // list devices
    // TODO: allow hotplug
    let devices = streamdeck::list_devices(&hid);
    // lets start some async
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(start(config, hid, devices))
}

pub async fn start(config: Config, hid: HidApi, hw_devices: Vec<(streamdeck::info::Kind, String)>) {
    init_devices(config, hid, hw_devices).await;

    loop {
        tokio::time::sleep(Duration::from_secs(2000)).await;
    }
}

/// try to send an event to the module channel.
/// If the module dropped the listener this will return false.
pub async fn send_to_channel(sender: mpsc::Sender<HostEvent>, event: HostEvent) -> bool {
    if let Err(e) = sender.try_send(event) {
        match e {
            TrySendError::Full(_) => trace!("Buffer full: {:?}", e),
            TrySendError::Closed(_) => return false,
        }
    }
    true
}

pub struct DeviceManager {
    modules: HashMap<u8, (Button, JoinHandle<()>, mpsc::Sender<HostEvent>)>,
    device: Arc<AsyncStreamDeck>,
    serial: String,
}

impl DeviceManager {
    async fn new(
        serial: String,
        device: Arc<AsyncStreamDeck>,
        modules: HashMap<u8, (Button, JoinHandle<()>, mpsc::Sender<HostEvent>)>,
    ) -> DeviceManager {
        DeviceManager {
            modules,
            device,
            serial,
        }
    }

    /// stops all modules of the device
    #[tracing::instrument(skip_all, fields(serial = self.serial))]
    fn shutdown(self) {
        for (index, (_, handle, _)) in self.modules {
            trace!("Destroying module {}", index);
            handle.abort();
        }
    }
    /// listener for button press changes on the device
    #[tracing::instrument(skip_all, fields(serial = self.serial))]
    async fn key_listener(self) {
        loop {
            match self.device.get_reader().read(7.0).await {
                Ok(v) => {
                    trace!("{:?}", v);
                    for update in v {
                        match update {
                            ButtonStateUpdate::ButtonDown(i) => {
                                let options = skip_if_none!(self.modules.get(&i));
                                if let Some(on_click) = &options.0.on_click {
                                    execute_sh(on_click).await;
                                } else {
                                    send_to_channel(options.2.clone(), HostEvent::ButtonPressed)
                                        .await;
                                }
                            }
                            ButtonStateUpdate::ButtonUp(i) => {
                                let options = skip_if_none!(self.modules.get(&i));
                                if let Some(on_release) = &options.0.on_release {
                                    execute_sh(on_release).await;
                                } else {
                                    send_to_channel(options.2.clone(), HostEvent::ButtonReleased)
                                        .await;
                                }
                            }
                        }
                    }
                }
                Err(e) => match e {
                    StreamDeckError::HidError(e) => {
                        error!("Shutting down device because of: {e}");
                        self.shutdown();
                        break;
                    }
                    _ => error!("{e}"),
                },
            }
        }
    }
}

/// This is the entry point for the application. This will check all devices for their config,
/// start the bridges and the device button listeners.
async fn init_devices(config: Config, hid: HidApi, devices: Vec<(streamdeck::info::Kind, String)>) {
    // check if configuration is correct for device
    if devices.len() == 0 {
        error!("There are no Decks connected");
        exit(1);
    }
    info!("There are {} Decks connected", devices.len());
    'outer: for device in devices {
        // no pedals are supported
        if !device.0.is_visual() {
            continue;
        }
        // device.1 is the serial number
        if let Some(device_conf) = config.device.iter().find(|s| s.serial == device.1) {
            // connect to deck or continue to next
            let deck = match AsyncStreamDeck::connect(&hid, device.0, &device.1) {
                Ok(deck) => {
                    info!("Successfully connected to {}", device.1);
                    deck
                }
                Err(e) => {
                    error!("Cannot connect to Deck {}: {}", device.1, e);
                    continue 'outer;
                }
            };
            // set brightness
            deck.set_brightness(device_conf.brightness).await.unwrap();
            // reset
            deck.reset().await.unwrap();
            // initialize buttons
            let button_count = device.0.key_count();
            let mut buttons_keys = HashMap::new();
            for button in device_conf.buttons.clone().into_iter() {
                // if the index of the button is higher than the button count
                if button_count < button.index {
                    warn!(
                        "The button {} does not exist on Deck {}; skipping",
                        button.index, device.1
                    );
                    continue 'outer;
                }
                // create a channel for the module to receive device events
                let (button_sender, button_receiver) = mpsc::channel(4);
                // spawn the module
                if let Some(module) = retrieve_module_from_name(button.module.clone()) {
                    let b = button.clone();
                    let rx = Arc::new(Mutex::new(button_receiver));
                    let dev = deck.clone();
                    let ser = device.1.clone();
                    let handle = tokio::spawn(async move {
                        start_module(ser, b, module, dev, rx).await;
                    });

                    buttons_keys.insert(button.index, (button, handle, button_sender));
                } else {
                    warn!("The module \"{}\" does not exist.", button.module)
                }
            }
            let manager = DeviceManager::new(device.1, deck, buttons_keys).await;
            // start the device key listener
            tokio::spawn(async move {
                manager.key_listener().await;
            });
        } else {
            info!("Deck {} is not configured; skipping", device.1);
        }
    }
}

pub async fn execute_sh(command: &str) {
    match Command::new("sh").arg(command).output().await {
        Ok(o) => debug!("Command \'{}\' returned: {}", command, o.status),
        Err(e) => error!("Command \'{}\' failed: {}", command, e),
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
