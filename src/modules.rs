mod blank;
mod counter;
mod space;

// modules
use self::counter::Counter;
use self::space::Space;

// other things
use crate::config::{Button, ButtonConfigError};
use async_trait::async_trait;
pub use deck_driver as streamdeck;
use futures_util::Future;
use image::DynamicImage;
use std::{error::Error, pin::Pin, sync::Arc};
use streamdeck::info::ImageFormat;
use streamdeck::info::Kind;
use streamdeck::AsyncStreamDeck;
use streamdeck::StreamDeckError;
use tokio::sync::mpsc;
use tracing::{debug, error};

/// Events that are coming from the host
#[derive(Clone, Copy, Debug)]
pub enum HostEvent {
    /// The button was pressed
    ButtonPressed,
    /// The button was released
    ButtonReleased,
}

pub type ModuleFuture =
    Pin<Box<dyn Future<Output = Result<Box<dyn Module + Sync + Send>, ButtonConfigError>> + Send>>;
pub type ModuleInitFunction = fn(Arc<Button>) -> ModuleFuture;

pub fn retrieve_module_from_name(name: &str) -> Option<ModuleInitFunction> {
    match name {
        "space" => Some(Space::init as ModuleInitFunction),
        "counter" => Some(Counter::init as ModuleInitFunction),
        _ => None,
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
    let da = DeviceAccess::new(device, button.index).await;

    // run init first
    //
    // panic should be prevented by the config being checked before running
    let mut module = match module_init_function(button).await {
        Ok(m) => m,
        Err(e) => panic!("{}", e),
    };

    // then run module
    match module.run(da, br).await {
        Ok(_) => debug!("RETURNED"),
        Err(e) => error!("RETURNED_ERROR: {}", e),
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
pub type ModuleObject = Box<dyn Module + Send + Sync>;

#[async_trait]
/// An object safe module trait.
///
/// - init() -> function for checking config and creating module
/// - run() -> function that happens when the device actually runs
pub trait Module: Sync + Send {
    async fn init(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError>
    where
        Self: Sized;
    async fn run(
        &mut self,
        device: DeviceAccess,
        receiver: ChannelReceiver,
    ) -> Result<(), ReturnError>;
}
