use crate::config::{Config, DeviceConfig};
use deck_driver as streamdeck;
use device::Device;
use font_loader::system_fonts::FontPropertyBuilder;
use hidapi::HidApi;
use rusttype::Font;
use std::{
    collections::HashMap,
    fs,
    process::exit,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::sync::Notify;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::{
    self, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

use config::{load_config, Space};

mod config;
mod device;
mod image_rendering;
mod modules;
mod type_definition;

pub static GLOBAL_FONT: OnceLock<Font> = OnceLock::new();
pub static CACHE_DIR: &'static str = "microdeck";

#[macro_export]
macro_rules! skip_if_none {
    ($res:expr) => {
        match $res {
            Some(v) => v,
            None => continue,
        }
    };
}

#[macro_export]
macro_rules! unwrap_or_error {
    ($res:expr) => {
        match $res {
            Ok(v) => v,
            Err(e) => error!("{}", e),
        }
    };
}

fn main() {
    let fmt_layer = tracing_subscriber::fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .init();

    let config = match load_config() {
        Ok(config) => config,
        Err(e) => {
            error!("{}", e);
            exit(1)
        }
    };

    // create cache dir
    if let Some(mut path) = dirs::cache_dir() {
        path.push(CACHE_DIR);
        if path.exists() == false {
            fs::create_dir(path).expect("Could not create cache directory");
        }
    }

    // load font
    let font = match config.global.font_family {
        Some(ref f) => font_loader::system_fonts::get(
            &mut FontPropertyBuilder::new().family(f.as_str()).build(),
        )
        .unwrap_or_else(|| {
            warn!("Unable to load custom font");
            load_system_font()
        }),
        None => load_system_font(),
    };

    GLOBAL_FONT
        .set(Font::try_from_vec(font.0).expect("Unable to parse font. Maybe try another font?"))
        .unwrap();

    debug!("{:#?}", config);

    let hid = streamdeck::new_hidapi().expect("Could not create HidApi");
    // lets start some async
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(start(config, hid))
}

fn load_system_font() -> (Vec<u8>, i32) {
    debug!("Retrieving system font");
    font_loader::system_fonts::get(&mut FontPropertyBuilder::new().monospace().build())
        .expect("Unable to load system monospace font. Please specify a custom font in the config.")
}

type DeviceHandle<T> = tokio::task::JoinHandle<T>;

async fn start(config: Config, mut hid: HidApi) {
    // This seems far too complex for what its actually doing
    let devices: Arc<Mutex<HashMap<String, DeviceHandle<_>>>> =
        Arc::new(Mutex::new(HashMap::new()));

    // devices which are not configured anyways
    let mut ignore_devices: Vec<String> = Vec::new();

    let refresh_cycle = Duration::from_secs(config.global.device_list_refresh_cycle);

    loop {
        // refresh device list
        trace!("Refreshing device list");
        if let Err(e) = streamdeck::refresh_device_list(&mut hid) {
            warn!("Cannot fetch new devices: {}", e);
        } else {
            trace!("Connected devices: {:?}", devices.lock().unwrap().keys());
            for hw_device in streamdeck::list_devices(&hid) {
                // if the device is already started or is ignored
                if !ignore_devices.contains(&hw_device.1)
                    && !devices.lock().unwrap().contains_key(&hw_device.1)
                {
                    // TODO: match regex for device serial
                    if let Some(device_config) = config
                        .devices
                        .iter()
                        .find(|d| d.serial == hw_device.1 || d.serial == "*")
                    {
                        // start the device and its functions
                        // TODO: start monitor when device dies and remove from HashMap
                        let is_dead = Arc::new(Notify::new());
                        if let Some(device) = start_device(
                            hw_device,
                            &hid,
                            device_config.clone(),
                            config.spaces.clone(),
                            is_dead.clone(),
                        )
                        .await
                        {
                            let serial = device.serial();
                            let handle = tokio::spawn(init_device_functions(device));
                            {
                                let mut devices = devices.lock().unwrap();
                                devices.insert(serial.clone(), handle);
                            }
                            let is_dead_cloned = is_dead.clone();
                            let devices_cloned = devices.clone();
                            tokio::task::spawn(async move {
                                is_dead_cloned.notified().await;
                                let mut devices = devices_cloned.lock().unwrap();
                                let device_handle =
                                    devices.get(&serial).expect("Can't retrieve device handle");
                                // kill runtime of device
                                device_handle.abort();
                                // remove device from list of active devices
                                devices.remove(&serial);
                            });
                        }
                    } else {
                        info!("The device {} is not configured.", hw_device.1);
                        ignore_devices.push(hw_device.1);
                    }
                }
            }
        }
        tokio::time::sleep(refresh_cycle).await;
    }
}

/// Create [Device] modules and init listener
async fn init_device_functions(device: Device) {
    let mut device = device;
    device.init_modules().await;
    device.key_listener().await;
}

/// Start a device by initially creating the [Device]
#[tracing::instrument(name = "device", skip_all, fields(serial = device.1))]
async fn start_device(
    device: (streamdeck::info::Kind, String),
    hid: &HidApi,
    device_config: DeviceConfig,
    spaces: Arc<HashMap<String, Space>>,
    is_dead: Arc<Notify>,
) -> Option<Device> {
    match Device::new(device.1, device.0, device_config, is_dead, spaces, &hid).await {
        Ok(device) => {
            info!("Connected");
            Some(device)
        }
        Err(e) => {
            error!("Unable to connect: {}", e);
            None
        }
    }
}
