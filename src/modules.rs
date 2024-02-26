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
use crate::image_rendering::{retrieve_image, ImageBuilder};
use ::image::imageops::FilterType;
use ::image::io::Reader as ImageReader;
use ::image::DynamicImage;
use async_trait::async_trait;
use base64::engine::{general_purpose, Engine};
pub use deck_driver as streamdeck;
use dirs::cache_dir;
use futures_util::Future;
use once_cell::sync::Lazy;
use ring::digest;
use std::collections::HashMap;
use std::{error::Error, path::PathBuf, pin::Pin, sync::Arc};
use streamdeck::info::ImageFormat;
use streamdeck::info::Kind;
use streamdeck::AsyncStreamDeck;
use streamdeck::StreamDeckError;
use tokio::sync::mpsc;
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
                .build();
            da.write_img(image).await.unwrap();
        }
    }
}

/// Loads an image from the system or retrieves it from the cache. If
/// the provided image is not already in the cache it will be inserted.
#[allow(dead_code)]
pub async fn load_image(path: PathBuf, resolution: (usize, usize)) -> Option<DynamicImage> {
    // hash the image
    let mut image = tokio::task::spawn_blocking(move || retrieve_image(&path))
        .await
        .unwrap()
        .ok()?;

    let image_hash = hash_image(image.as_bytes());

    if let Some(image) = get_image_from_cache(&image_hash, resolution) {
        trace!("Cached image is available");
        return Some(image);
    }

    // TODO prevent multiple buttons from resizing the same image at the same time (performance
    // improvement)
    let image = tokio::task::spawn_blocking(move || {
        trace!("Resizing image");
        image = image.resize_exact(
            resolution.0 as u32,
            resolution.1 as u32,
            FilterType::Lanczos3,
        );
        trace!("Resizing finished");
        let mut path = match cache_dir() {
            Some(dir) => dir,
            None => return None, // System does not provide cache
        };
        path.push("microdeck");
        path.push(image_cache_file_name(&image_hash, resolution));

        image.save(path).ok()?;
        Some(image)
    })
    .await
    .unwrap()?;
    Some(image.into())
}

/// File name for a cached image
///
/// `<hash>-<height>x<width>`
pub fn image_cache_file_name(image_hash: &str, resolution: (usize, usize)) -> String {
    format!("{}-{}x{}.png", image_hash, resolution.0, resolution.1)
}

pub fn hash_image(data: &[u8]) -> String {
    let mut context = digest::Context::new(&digest::SHA256);
    context.update(data);
    let hash = context.finish();
    general_purpose::STANDARD.encode(hash)
}

/// Try to retrieve an image from the cache. Will return None if
/// the image was not cached yet (or is not accessible)
/// or if the system does not provide a [dirs::cache_dir].
#[allow(dead_code)]
pub fn get_image_from_cache(image_hash: &str, resolution: (usize, usize)) -> Option<DynamicImage> {
    let mut path = match cache_dir() {
        Some(dir) => dir,
        None => return None, // System does not provide cache
    };

    path.push("microdeck");
    path.push(image_cache_file_name(image_hash, resolution));

    Some(ImageReader::open(path).ok()?.decode().ok()?)
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
