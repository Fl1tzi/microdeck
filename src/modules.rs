mod blank;
mod counter;
mod space;

// modules
use self::counter::Counter;
use self::space::Space;

use crate::GLOBAL_FONT;
// other things
use crate::config::Button;
use async_trait::async_trait;
pub use deck_driver as streamdeck;
use futures_util::Future;
use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;
use lazy_static::lazy_static;
use rusttype::Scale;
use std::collections::HashMap;
use std::pin::Pin;
use std::str::FromStr;
use std::{error::Error, sync::Arc};
pub use streamdeck::info::ImageFormat;
use streamdeck::info::Kind;
use streamdeck::AsyncStreamDeck;
pub use streamdeck::StreamDeckError;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

lazy_static! {
    static ref MODULE_MAP: HashMap<&'static str, ModuleFunction> = {
        let mut m = HashMap::new();
        m.insert("counter", Counter::run as ModuleFunction);
        m.insert("space", Space::run as ModuleFunction);
        m
    };
}

/// Events that are coming from the host
#[derive(Clone, Copy, Debug)]
pub enum HostEvent {
    /// The button was pressed
    ButtonPressed,
    /// The button was released
    ButtonReleased,
}

pub type ModuleFuture = Pin<Box<dyn Future<Output = Result<(), ReturnError>> + Send>>;
pub type ModuleFunction = fn(DeviceAccess, ChannelReceiver, Arc<Button>) -> ModuleFuture;

pub fn retrieve_module_from_name(name: &str) -> Option<ModuleFunction> {
    MODULE_MAP.get(name).copied()
}

/// starts a module
#[tracing::instrument(name = "module", skip_all, fields(serial = serial, button = button.index, module = button.module))]
pub async fn start_module(
    // Just for logging purpose
    serial: String,
    button: Arc<Button>,
    module_function: ModuleFunction,
    device: Arc<AsyncStreamDeck>,
    br: ChannelReceiver,
) {
    debug!("STARTED");
    let da = DeviceAccess::new(device, button.index).await;

    // actually run the module
    match module_function(da, br, button).await {
        Ok(_) => info!("CLOSED"),
        Err(e) => error!("ERR: {:?}", e),
    }
}

/// Wrapper to provide easier access to the Deck
pub struct DeviceAccess {
    streamdeck: Arc<AsyncStreamDeck>,
    kind: Kind,
    index: u8,
}

impl DeviceAccess {
    pub async fn new(streamdeck: Arc<AsyncStreamDeck>, index: u8) -> DeviceAccess {
        let kind = streamdeck.kind();
        DeviceAccess {
            streamdeck,
            kind,
            index,
        }
    }

    /// write a raw image to the Deck.
    pub async fn write_raw_img(&self, img: &[u8]) -> Result<(), StreamDeckError> {
        self.streamdeck.write_image(self.index, img).await
    }

    /// Write an image to the Deck.
    pub async fn write_img(&self, img: DynamicImage) -> Result<(), StreamDeckError> {
        self.streamdeck.set_button_image(self.index, img).await
    }

    /// reset the image.
    pub async fn clear_img(&self) -> Result<(), StreamDeckError> {
        self.streamdeck.clear_button_image(self.index).await
    }

    pub fn format(&self) -> ImageFormat {
        self.kind.key_image_format()
    }

    /// The resolution of the image on the Deck.
    pub fn resolution(&self) -> (usize, usize) {
        self.format().size
    }

    /// Draw an image with text
    ///
    /// 60% image
    /// 40% text
    pub fn image_with_text(&self, image: Vec<u8>, text: String, config: &Button) -> DynamicImage {
        let res = self.resolution();
        let mut image = RgbImage::new(res.0 as u32, res.1 as u32);
        draw_text_mut(
            &mut image,
            Rgb([255, 255, 255]),
            -10,
            10,
            Scale::uniform(parse_config(config, &"FONT_SIZE".into(), 20.0)),
            &GLOBAL_FONT.get().unwrap(),
            &text,
        );
        image::DynamicImage::ImageRgb8(image)
    }

    /// Draw text
    pub fn text(&self, text: String, config: &Button) -> DynamicImage {
        let res = self.resolution();
        let mut image = RgbImage::new(res.0 as u32, res.1 as u32);
        draw_text_mut(
            &mut image,
            Rgb([255, 255, 255]),
            10,
            10,
            Scale::uniform(parse_config(config, &"FONT_SIZE".into(), 20.0)),
            &GLOBAL_FONT.get().unwrap(),
            &text,
        );
        image::DynamicImage::ImageRgb8(image)
    }
}

/// reads a key from the config and parses the config in the given type
fn parse_config<T>(config: &Button, key: &String, if_wrong_type: T) -> T
where
    T: FromStr,
{
    let out = match config.options.get(key) {
        Some(value) => value.parse::<T>().unwrap_or(if_wrong_type),
        None => if_wrong_type,
    };
    out
}

pub type ReturnError = Box<dyn Error + Send + Sync>;
pub type ChannelReceiver = mpsc::Receiver<HostEvent>;

#[async_trait]
pub trait Module {
    async fn run(
        device: DeviceAccess,
        receiver: ChannelReceiver,
        config: Arc<Button>,
    ) -> Result<(), ReturnError>;
}
