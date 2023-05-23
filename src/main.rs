use deck_driver as streamdeck;
use crate::config::{Config, DeviceConfig};
use device::Device;
use hidapi::HidApi;
use std::{collections::HashMap, process::exit, time::Duration};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{
    self, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, EnvFilter,
};

use config::load_config;

mod config;
mod device;
mod modules;

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

    debug!("{:#?}", config);

    let hid = streamdeck::new_hidapi().expect("Could not create HidApi");
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
                        config.devices.iter().find(|d| d.serial == hw_device.1)
                    {
                        // start the device and its listener
                        if let Some(device) =
                            start_device(hw_device, &hid, device_config.clone()).await
                        {
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

/// Start a device by initially creating the [Device] and then starting all modules and listeners for that device
#[tracing::instrument(name = "device", skip_all, fields(serial = device.1))]
pub async fn start_device(
    device: (streamdeck::info::Kind, String),
    hid: &HidApi,
    device_config: DeviceConfig,
) -> Option<Device> {
    match Device::new(device.1, device.0, device_config, &hid).await {
        Ok(mut device) => {
            info!("Connected");
            // start all modules or print out the error
            /* device_config.buttons.iter().for_each(|button| {
                device
                    .create_module(&button)
                    .unwrap_or_else(|e| error!("{}", e))
            });*/
            device.init_modules().await;
            device.key_listener().await;
            if !device.has_modules() {
                warn!("All modules have failed to start");
            }
            Some(device)
        }
        Err(e) => {
            error!("Unable to connect: {}", e);
            None
        }
    }
}

