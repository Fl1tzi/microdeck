use crate::{
    modules::{retrieve_module_from_name, start_module, HostEvent},
    Button, ConfigError, DeviceConfig,
    skip_if_none, unwrap_or_error
};
use deck_driver as streamdeck;
use hidapi::HidApi;
use std::{
    collections::HashMap,
    fmt::Display,
    sync::Arc,
};
use streamdeck::{
    asynchronous::{AsyncStreamDeck, ButtonStateUpdate},
    info::Kind,
    StreamDeckError,
};
use tokio::{
    process::Command,
    sync::mpsc::{self, error::TrySendError},
    runtime::Runtime,
};
use tracing::{debug, error, info_span, trace};

pub type ModuleController = (Arc<Button>, mpsc::Sender<HostEvent>);

pub enum DeviceError {
    DriverError(StreamDeckError),
    Config(ConfigError),
}

impl Display for DeviceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            DeviceError::DriverError(e) => write!(formatter, "Driver: {}", e),
            DeviceError::Config(e) => write!(formatter, "Config: {}", e),
        }
    }
}

/// Handles everything related to a single device
pub struct Device {
    modules: HashMap<u8, ModuleController>,
    device: Arc<AsyncStreamDeck>,
    modules_runtime: Option<Runtime>,
    config: DeviceConfig,
    serial: String,
}

impl Device {
    pub async fn new(
        serial: String,
        kind: Kind,
        device_conf: DeviceConfig,
        hid: &HidApi,
    ) -> Result<Device, DeviceError> {
        // connect to deck or continue to next
        let deck = match AsyncStreamDeck::connect(hid, kind, &serial) {
            Ok(deck) => deck,
            Err(e) => return Err(DeviceError::DriverError(e)),
        };
        // set brightness
        deck.set_brightness(device_conf.brightness).await.unwrap();
        // reset
        deck.reset().await.unwrap();
        // initialize buttons
        let button_count = kind.key_count();

        // CONFIG VALIDATING
        for button in device_conf.buttons.clone().into_iter() {
            let _span_button = info_span!("button", index = button.index).entered();
            // if the index of the button is higher than the button count
            if button_count < button.index {
                return Err(DeviceError::Config(ConfigError::ButtonDoesNotExist(
                    button.index,
                )));
            }
        }
        Ok(Device {
            modules: HashMap::new(),
            device: deck,
            modules_runtime: None,
            config: device_conf,
            serial,
        })
    }

    pub async fn init_modules(&mut self) {
        if self.modules_runtime.is_none() {
            self.modules_runtime = Some(Runtime::new().unwrap());
        }
        for i in 0..self.config.buttons.len() {
            let button = self.config.buttons.get(i).unwrap().to_owned();
            unwrap_or_error!(self._create_module(button).await);
        }
        
    }

    async fn _create_module(&mut self, btn: Arc<Button>) -> Result<(), DeviceError> {
        let runtime = self.modules_runtime.as_ref().expect("Runtime has to be created before module can be spawned");
        let (button_sender, button_receiver) = mpsc::channel(4);
        if let Some(module) = retrieve_module_from_name(&btn.module) {
            {
                let ser = self.serial.clone();
                let dev = self.device.clone();
                let b = btn.clone();

                runtime.spawn(async move {
                    start_module(ser, b, module, dev, Box::new(button_receiver)).await
                });
            }
            self.modules
                .insert(btn.index, (btn.clone(), button_sender));
            return Ok(());
        } else {
            return Err(DeviceError::Config(ConfigError::ModuleDoesNotExist(
                btn.index,
                btn.module.clone(),
            )));
        }
    }

    pub fn serial(&self) -> String {
        self.serial.clone()
    }

    fn drop(&mut self) {
        if let Some(handle) = self.modules_runtime.take() {
            handle.shutdown_background();
        }
    }

    pub fn is_dropped(&self) -> bool {
        self.modules_runtime.is_none()
    }

    pub fn has_modules(&self) -> bool {
        !self.modules.is_empty()
    }

    /// listener for button press changes on the device
    #[tracing::instrument(skip_all, fields(serial = self.serial))]
    pub async fn key_listener(&mut self) {
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
                                    send_to_channel(&options.1, HostEvent::ButtonPressed).await;
                                }
                            }
                            ButtonStateUpdate::ButtonUp(i) => {
                                let options = skip_if_none!(self.modules.get(&i));
                                if let Some(on_release) = &options.0.on_release {
                                    execute_sh(on_release).await;
                                } else {
                                    send_to_channel(&options.1, HostEvent::ButtonReleased).await;
                                }
                            }
                        }
                    }
                }
                Err(e) => match e {
                    StreamDeckError::HidError(e) => {
                        error!("Shutting down device because of: {e}");
                        self.drop();
                        break;
                    }
                    _ => error!("{e}"),
                },
            }
        }
    }
}

pub async fn execute_sh(command: &str) {
    match Command::new("sh").arg(command).output().await {
        Ok(o) => debug!("Command \'{}\' returned: {}", command, o.status),
        Err(e) => error!("Command \'{}\' failed: {}", command, e),
    }
}

/// try to send an event to the module channel.
/// If the module dropped the listener this will return false.
pub async fn send_to_channel(sender: &mpsc::Sender<HostEvent>, event: HostEvent) -> bool {
    if let Err(e) = sender.try_send(event) {
        match e {
            TrySendError::Full(_) => trace!("Buffer full: {:?}", e),
            TrySendError::Closed(_) => return false,
        }
    }
    true
}
