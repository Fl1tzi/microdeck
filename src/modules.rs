mod blank;
mod counter;
mod image;
mod space;
mod system_metrics;

// modules
use self::counter::Counter;
use self::image::Image;
use self::space::Space;
use self::system_metrics::SystemMetrics;

// other things
use crate::config::{Button, ButtonConfigError};
use crate::image_rendering::ImageBuilder;
use ::image::DynamicImage;
use async_trait::async_trait;
pub use deck_driver as streamdeck;
use futures_util::Future;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::{error::Error, pin::Pin, sync::Arc};
use streamdeck::info::ImageFormat;
use streamdeck::info::Kind;
use streamdeck::AsyncStreamDeck;
use streamdeck::StreamDeckError;
use tokio::sync::mpsc;
use tracing::{debug, error};

pub mod prelude {
    pub use super::{ChannelReceiver, DeviceAccess, HostEvent, Module, ModuleObject, ReturnError};
    pub use crate::config::{Button, ButtonConfigError};
    pub use crate::image_rendering::{cache::*, ImageBuilder};
    pub use image::DynamicImage;
}

pub static MODULE_REGISTRY: Lazy<ModuleRegistry> = Lazy::new(|| ModuleRegistry::default());

/// Events that are coming from the host
#[derive(Clone, Copy, Debug)]
pub enum HostEvent {
    /// The button was pressed
    ButtonPressed,
    /// The button was released
    ButtonReleased,
}

pub type ModuleObject = Box<dyn Module + Send + Sync>;
pub type ModuleFuture =
    Pin<Box<dyn Future<Output = Result<ModuleObject, ButtonConfigError>> + Send>>;
pub type ModuleInitFunction = fn(Arc<Button>) -> ModuleFuture;

pub type ModuleMap = HashMap<&'static str, ModuleInitFunction>;

/// Registry of available modules
pub struct ModuleRegistry {
    modules: ModuleMap,
}

impl Default for ModuleRegistry {
    fn default() -> Self {
        let mut modules = ModuleMap::new();
        modules.insert("space", Space::new as ModuleInitFunction);
        modules.insert("image", Image::new as ModuleInitFunction);
        modules.insert("counter", Counter::new as ModuleInitFunction);
        modules.insert("system_metrics", SystemMetrics::new as ModuleInitFunction);
        ModuleRegistry { modules }
    }
}

impl ModuleRegistry {
    /// Retrieve a module from the registry
    pub fn get_module(&self, name: &str) -> Option<&ModuleInitFunction> {
        self.modules.get(name)
    }

    // TODO: Idk if the &&str will be fine in the future
    /// List all available modules
    #[allow(dead_code)]
    pub fn list_modules(&self) -> Vec<&&str> {
        self.modules.keys().collect()
    }
}

/// starts a module
#[tracing::instrument(name = "module", skip_all, fields(serial = serial, button = button.index, module = button.module))]
pub async fn start_module(
    // Just for logging purpose
    serial: String,
    button: Arc<Button>,
    module_init_function: ModuleInitFunction,
    device: Arc<AsyncStreamDeck>,
    br: ChannelReceiver,
) {
    debug!("STARTED");
    let da = DeviceAccess::new(device.clone(), button.index).await;

    // init
    //
    // This function should be called after the config was checked,
    // otherwise it will panic and the module wont be started.
    let mut module = match module_init_function(button.clone()).await {
        Ok(m) => m,
        Err(e) => panic!("{}", e),
    };

    // then run module
    match module.run(da, br).await {
        Ok(_) => debug!("RETURNED"),
        // TODO: maybe find calculation for font size
        // print error on display
        Err(e) => {
            error!("{e}");
            let da = DeviceAccess::new(device, button.index).await;
            let res = da.resolution();
            let image = ImageBuilder::new(res.0, res.1)
                .set_text(format!("E: {}", e))
                .set_font_size(12.0)
                .set_text_color([255, 0, 0])
                .build()
                .await;
            da.write_img(image).await.unwrap();
        }
    }
}

/// Wrapper to provide easier access to the Deck
pub struct DeviceAccess {
    streamdeck: Arc<AsyncStreamDeck>,
    kind: Kind,
    index: u8,
}

impl DeviceAccess {
    #[allow(dead_code)]
    pub async fn new(streamdeck: Arc<AsyncStreamDeck>, index: u8) -> DeviceAccess {
        let kind = streamdeck.kind();
        DeviceAccess {
            streamdeck,
            kind,
            index,
        }
    }

    /// write a raw image to the Deck.
    #[allow(dead_code)]
    pub async fn write_raw_img(&self, img: &[u8]) -> Result<(), StreamDeckError> {
        self.streamdeck.write_image(self.index, img).await
    }

    /// Write an image to the Deck.
    #[allow(dead_code)]
    pub async fn write_img(&self, img: DynamicImage) -> Result<(), StreamDeckError> {
        self.streamdeck.set_button_image(self.index, img).await
    }

    /// reset the image.
    #[allow(dead_code)]
    pub async fn clear_img(&self) -> Result<(), StreamDeckError> {
        self.streamdeck.clear_button_image(self.index).await
    }

    #[allow(dead_code)]
    pub fn format(&self) -> ImageFormat {
        self.kind.key_image_format()
    }

    /// The resolution of the image on the Deck.
    #[allow(dead_code)]
    pub fn resolution(&self) -> (usize, usize) {
        self.format().size
    }
}

pub type ReturnError = Box<dyn Error + Send + Sync>;
pub type ChannelReceiver = mpsc::Receiver<HostEvent>;

#[async_trait]
/// An object safe module Trait for representing a single Module.
pub trait Module: Sync + Send {
    /// Function for validating configuration and creating module instance. Every time the config
    /// is checked this function gets called. It therefore should validate the most efficient
    /// things first.
    ///
    /// This function should **not** panic as the panic will not be catched and therefore would be
    /// not noticed.
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError>
    where
        Self: Sized;
    /// Function for actually running the module and interacting with the device. Errors that
    /// happen here should be mostly prevented as they are not properly handled.
    async fn run(
        &mut self,
        device: DeviceAccess,
        receiver: ChannelReceiver,
    ) -> Result<(), ReturnError>;
}
