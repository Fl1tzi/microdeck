mod blank;
mod counter;
mod image;
mod space;

// modules
use self::counter::Counter;
use self::image::Image;
use self::space::Space;

// other things
use crate::config::{Button, ButtonConfigError};
use crate::device::ImageCache;
use crate::image_rendering::load_image;
use ::image::imageops::FilterType;
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
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, trace};

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
pub type ModuleInitFunction = fn(Arc<Button>, ModuleCache) -> ModuleFuture;

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
    image_cache: Arc<Mutex<ImageCache>>,
) {
    debug!("STARTED");
    let mc = ModuleCache::new(
        image_cache,
        button.index,
        device.kind().key_image_format().size,
    );
    let da = DeviceAccess::new(device, button.index).await;

    // init
    //
    // This function should be called after the config was checked,
    // otherwise it will panic and the module wont be started.
    let mut module = match module_init_function(button, mc).await {
        Ok(m) => m,
        Err(e) => panic!("{}", e),
    };

    // then run module
    match module.run(da, br).await {
        Ok(_) => debug!("RETURNED"),
        Err(e) => error!("RETURNED_ERROR: {}", e),
    }
}

/// A wrapper around [ImageCache] to provide easy access to values in the device cache
pub struct ModuleCache {
    image_cache: Arc<Mutex<ImageCache>>,
    button_index: u8,
    /// Resolution of the deck (required for optimization of storage space)
    resolution: (usize, usize),
}

impl ModuleCache {
    pub fn new(
        image_cache: Arc<Mutex<ImageCache>>,
        button_index: u8,
        resolution: (usize, usize),
    ) -> Self {
        ModuleCache {
            image_cache,
            button_index,
            resolution,
        }
    }

    /// Load an image from the [ImageCache] or create a new one and insert it into the [ImageCache].
    /// Returns None if no image was found.
    ///
    /// index: Provide an index where your data is cached. With this number the value can be
    /// accessed again. Use [DeviceAccess::get_image_cached()] for just getting the data.
    #[allow(dead_code)]
    pub async fn load_image(&mut self, path: String, index: u32) -> Option<Arc<DynamicImage>> {
        if let Some(image) = self.get_image(index).await {
            Some(image)
        } else {
            trace!("Decoding image");
            let mut image = tokio::task::spawn_blocking(move || load_image(path))
                .await
                .unwrap()
                .ok()?;
            image = image.resize_exact(
                self.resolution.0 as u32,
                self.resolution.1 as u32,
                FilterType::Lanczos3,
            );
            trace!("Decoding finished");
            let image = Arc::new(image);
            let mut data = self.image_cache.lock().await;
            data.put((self.button_index, index), image.clone());
            trace!("Wrote data into cache (new size: {})", data.len());
            drop(data);
            Some(image.into())
        }
    }

    /// Just try to retrieve a value from the key (index) in the [ImageCache].
    #[allow(dead_code)]
    pub async fn get_image(&self, index: u32) -> Option<Arc<DynamicImage>> {
        let mut data = self.image_cache.lock().await;
        data.get(&(self.button_index, index)).cloned()
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
    async fn new(
        config: Arc<Button>,
        mut cache: ModuleCache,
    ) -> Result<ModuleObject, ButtonConfigError>
    where
        Self: Sized;
    /// Function for actually running the module and interacting with the device. Errors that
    /// happen here should be mostly prevented.
    ///
    /// TODO: The return error is not sent anywhere and is just a panic
    async fn run(
        &mut self,
        device: DeviceAccess,
        receiver: ChannelReceiver,
    ) -> Result<(), ReturnError>;
}
