use hidapi::HidApi;
use log::{debug, error, info, trace, warn};
use serde::Deserialize;
use simple_logger;
use tokio::sync::mpsc::error::TrySendError;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;
use std::{env, path::PathBuf};
use std::fs;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
mod modules;
use elgato_streamdeck as streamdeck;
use streamdeck::asynchronous::{AsyncStreamDeck, ButtonStateUpdate};

use dirs::config_dir;

use crate::modules::{start_module, HostEvent};

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
struct GlobalConfig {
    default_font: Option<String>
}

fn main() {
    simple_logger::init_with_env().unwrap();

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

    // TODO: PLEASE IMPROVE THIS!!
    // Issue is that tokio sleeps are not kept running while they are sleeping which results in the
    // program exiting...
    //
    // However, this will stay open even if the program is nothing doing anymore.
    loop {
        tokio::time::sleep(Duration::from_secs(2000)).await;
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
                    error!("Failed to connect to Deck {}:\n{}", device.1, e);
                    continue 'outer;
                }
            };
            // set brightness
            deck.set_brightness(device_conf.brightness).await.unwrap();
            // reset
            deck.reset().await.unwrap();
            // initialize buttons
            // let mut bridges: Vec<Bridge> = Vec::new();
            let button_count = device.0.key_count();
            let mut buttons_key = HashMap::new();
            for button in device_conf.buttons.clone().into_iter() {
                // if the index of the button is higher than the button count
                if button_count < button.index {
                    warn!(
                        "The button {} does not exist on Deck {}; skipping",
                        button.index, device.1
                    );
                    continue 'outer;
                }
                // check if the action has the correct syntax
                for key in vec![&button.on_click, &button.on_release] {
                    if let Some(a) = key {
                        for action in a {
                            if !action.starts_with("bash:") && !action.starts_with("sh:") {
                                error!(
                                    "Unknown action in button {} on Deck {}; skipping",
                                    button.index, device.1
                                );
                                continue 'outer;
                            }
                        }
                    }
                }
                // create a watch channel for the module to receive device events
                let (button_sender, button_receiver) = mpsc::channel(4);
                buttons_key.insert(
                    button.index,
                    (
                        button_sender,
                        (button.on_click.clone(), button.on_release.clone()),
                    ),
                );
                // spawn the module
                let b = button.clone();
                let rx = Arc::new(Mutex::new(button_receiver));
                let dev = deck.clone();
                tokio::spawn(async move {
                    start_module(b, dev, rx).await;
                });
            }
            // start the device key listener
            tokio::spawn(async move {
                device_key_listener(deck, buttons_key).await;
            });
        } else {
            info!("Deck {} is not configured; skipping", device.1);
        }
    }
}

/// listener for button press changes on the device. Also executes the scripts.
pub async fn device_key_listener(
    device: Arc<AsyncStreamDeck>,
    mut keys: HashMap<
        u8,
        (
            mpsc::Sender<HostEvent>,
            (Option<Vec<String>>, Option<Vec<String>>),
        ),
    >,
) {
    loop {
        match device.get_reader().read(7.0).await {
            Ok(v) => {
                trace!("Received Keypress: {:?}", v);
                for update in v {
                    match update {
                        ButtonStateUpdate::ButtonDown(i) => {
                            let options = skip_if_none!(keys.get(&i));
                            let actions = &options.1 .0;
                            if send_key_event(options, actions, HostEvent::ButtonPressed).await == false {
                                debug!("Removed key {} from listeners (receiver dropped)", &i);
                                keys.remove(&i);
                            }
                        }
                        ButtonStateUpdate::ButtonUp(i) => {
                            let options = skip_if_none!(keys.get(&i));
                            let actions = &options.1.1;
                            /* let sender = &options.0;
                            let on_release = &options.1 .1;
                            if let Some(actions) = on_release {
                                execute_button_action(actions).await;
                            } else {
                                if sender.try_send(HostEvent::ButtonReleased).is_err() {
                                    keys.remove(&i);
                                    debug!("Removed key {} from listeners (does not respond)", &i);
                                }
                            }*/
                            if send_key_event(options, actions, HostEvent::ButtonReleased).await == false {
                                debug!("Removed key {} from listeners (receiver dropped)", &i);
                                keys.remove(&i);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Error while retrieving key status: {:?}", e);
            }
        }
    }
}

/// manually sends the script event or try to send it to the module.
/// Returns false if the receiver is dead and can therefore be removed.
pub async fn send_key_event(options: &(mpsc::Sender<HostEvent>, (Option<Vec<String>>, Option<Vec<String>>)), actions: &Option<Vec<String>>, event: HostEvent) -> bool {
    let sender = &options.0;
    if let Some(actions) = actions {
        execute_button_action(actions).await;
    } else {
        if let Err(e) = sender.try_send(event) {
            match e {
                TrySendError::Full(_) => trace!("Buffer full: {:?}", e),
                TrySendError::Closed(_) => {
                    return false
                }
            }
        }
    }
    true
}

/// executes a shell script
pub async fn execute_button_action(actions: &Vec<String>) {
    for a in actions {
        if let Some(v) = a.strip_prefix("bash:") {
            execute_bash(v).await;
        } else if let Some(v) = a.strip_prefix("sh:") {
            execute_sh(v).await;
        } else {
            unreachable!()
        }
    }
}

pub async fn execute_bash(command: &str) {
    match Command::new("/bin/bash").arg(command).output().await {
        Ok(o) => debug!("Command \'{}\' returned: {}", command, o.status),
        Err(e) => error!("Command \'{}\' failed: {}", command, e),
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
    pub on_click: Option<Vec<String>>,
    /// allows to overwrite what it will do on a release; Same options as [on_click]
    pub on_release: Option<Vec<String>>,
}
