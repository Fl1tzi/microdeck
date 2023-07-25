use crate::{
    config::{Button, ConfigError, DeviceConfig, Space},
    modules::{retrieve_module_from_name, start_module, HostEvent},
    unwrap_or_error,
};
use deck_driver as streamdeck;
use hidapi::HidApi;
use std::{collections::HashMap, fmt::Display, sync::Arc};
use streamdeck::{
    asynchronous::{AsyncStreamDeck, ButtonStateUpdate},
    info::Kind,
    StreamDeckError,
};
use tokio::{
    process::Command,
    runtime::Runtime,
    sync::mpsc::{self, error::TrySendError},
};
use tracing::{debug, error, info_span, trace, warn};

/// A module controller in holding the information of a Module
pub type ModuleController = (Arc<Button>, Option<mpsc::Sender<HostEvent>>);

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
    spaces: Arc<HashMap<String, Space>>,
    selected_space: Option<String>,
    serial: String,
}

impl Device {
    pub async fn new(
        serial: String,
        kind: Kind,
        device_conf: DeviceConfig,
        spaces: Arc<HashMap<String, Space>>,
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
        for button in device_conf.buttons.as_slice().into_iter() {
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
            spaces,
            selected_space: None,
            serial,
        })
    }

    /// Create the runtime for all the modules and iterate through all the buttons to create their
    /// modules.
    pub async fn init_modules(&mut self) {
        if self.modules_runtime.is_none() {
            self.modules_runtime = Some(Runtime::new().unwrap());
        }
        // TODO: DO THIS WITHOUT CLONING! Currently takes up a big amount of memory.
        let button_config = match &self.selected_space {
            Some(s) => self.spaces.get(s).unwrap_or_else(|| {
                warn!("The space \"{}\" was not found", s);
                &self.config.buttons
            }
            ).to_owned(),
            None => self.config.buttons.to_owned()
        };
        for i in 0..button_config.len() {
            let button = button_config.get(i).unwrap().to_owned();
            unwrap_or_error!(self._create_module(button).await);
        }
    }

    /// spawn the module onto the runtime
    async fn _create_module(&mut self, btn: Arc<Button>) -> Result<(), DeviceError> {
        let runtime = self
            .modules_runtime
            .as_ref()
            .expect("Runtime has to be created before module can be spawned");
        let (module_sender, module_receiver) = mpsc::channel(4);
        if let Some(module) = retrieve_module_from_name(&btn.module) {
            {
                // initialize the module
                let ser = self.serial.clone();
                let dev = self.device.clone();
                let b = btn.clone();

                runtime
                    .spawn(async move { start_module(ser, b, module, dev, module_receiver).await });
            }
            // if the receiver already dropped the listener then just directly insert none.
            // Optimizes performance because the key_listener just does not try to send the event.
            if module_sender.is_closed() {
                self.modules.insert(btn.index, (btn.clone(), None));
            } else {
                self.modules
                    .insert(btn.index, (btn.clone(), Some(module_sender)));
            }
            return Ok(());
        } else {
            return Err(DeviceError::Config(ConfigError::ModuleDoesNotExist(
                btn.index,
                btn.module.to_owned(),
            )));
        }
    }

    pub fn serial(&self) -> String {
        self.serial.clone()
    }

    /// shutdown the runtime and therefore kill all the modules
    fn drop(&mut self) {
        if let Some(handle) = self.modules_runtime.take() {
            handle.shutdown_background();
        }
        self.modules = HashMap::new();
    }

    /// if this device holds any modules
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
                        self.button_state_update(update).await;
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

    /// Switch to a space. This will tear down the whole runtime of the current space.
    #[tracing::instrument(skip_all, fields(serial = self.serial))]
    async fn switch_to_space(&mut self, name: String) {
        debug!("Switching to space {}", name);
        if name.to_lowercase() == "home" {
            self.selected_space = None
        } else {
            self.selected_space = Some(name)
        }
        self.drop();
        self.device.reset().await.unwrap();
        self.init_modules().await;
    }

    /// Handle all incoming button state updates from the listener (shell actions, module sender)
    async fn button_state_update(&mut self, event: ButtonStateUpdate) {
        // get the index out of the enum...
        let index = match event {
            ButtonStateUpdate::ButtonUp(i) => i,
            ButtonStateUpdate::ButtonDown(i) => i,
        };
        // try to get config for the module
        let options = match self.modules.get_mut(&index) {
            Some(options) => options,
            None => return,
        };
        // action will only be some if on_click/on_release is specified in config
        let (action, event) = match event {
            ButtonStateUpdate::ButtonDown(_) => (&options.0.on_click, HostEvent::ButtonPressed),
            ButtonStateUpdate::ButtonUp(_) => (&options.0.on_release, HostEvent::ButtonReleased),
        };
        // try to send to module and drop the sender if the receiver was droppped
        if let Some(sender) = options.to_owned().1 {
            if send_to_channel(&sender, event).await == false {
                trace!("Sender of button {index} dropped");
                options.1 = None
            }
        }
        // if config includes custom actions execute them
        if let Some(action) = action {
            execute_sh(&action).await
        }
        // switch space if needed
        if options.0.module == "space" {
            let name = match options.0.options.get("NAME") {
                Some(n) => n.clone(),
                None => return,
            };
            self.switch_to_space(name).await;
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
