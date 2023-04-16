mod blank;
mod counter;

use self::counter::Counter;
use crate::Button;
use async_trait::async_trait;
pub use deck_driver as streamdeck;
use futures_util::Future;
use image::DynamicImage;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::pin::Pin;
use std::{error::Error, sync::Arc};
pub use streamdeck::info::ImageFormat;
use streamdeck::info::Kind;
use streamdeck::AsyncStreamDeck;
pub use streamdeck::StreamDeckError;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

lazy_static! {
    static ref MODULE_MAP: HashMap<&'static str, ModuleFunction> = {
        let mut m = HashMap::new();
        m.insert("counter", Counter::run as ModuleFunction);
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
pub type ModuleFunction = fn(DeviceAccess, ChannelReceiver, Button) -> ModuleFuture;

pub fn retrieve_module_from_name(name: String) -> Option<ModuleFunction> {
    MODULE_MAP.get(name.as_str()).copied()
}

/// starts a module
#[tracing::instrument(name = "module", skip_all, fields(serial = serial, button = button.index, module = button.module))]
pub async fn start_module(
    // Just for logging purpose
    serial: String,
    button: Button,
    module_function: ModuleFunction,
    device: Arc<AsyncStreamDeck>,
    br: Arc<Mutex<mpsc::Receiver<HostEvent>>>,
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

    pub fn kind(&self) -> Kind {
        self.kind
    }
}

pub type ReturnError = Box<dyn Error + Send + Sync>;
pub type ChannelReceiver = Arc<Mutex<mpsc::Receiver<HostEvent>>>;

#[async_trait]
pub trait Module {
    async fn run(
        device: DeviceAccess,
        button_receiver: ChannelReceiver,
        config: Button,
    ) -> Result<(), ReturnError>;
}
