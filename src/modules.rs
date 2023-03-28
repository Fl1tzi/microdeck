mod blank;
mod counter;

use std::{error::Error, sync::Arc};

use self::blank::Blank;
use self::counter::Counter;
use crate::Button;
use async_trait::async_trait;
pub use elgato_streamdeck as streamdeck;
use image::DynamicImage;
use log::{error, info, trace};
pub use streamdeck::info::ImageFormat;
use streamdeck::info::Kind;
use streamdeck::AsyncStreamDeck;
pub use streamdeck::StreamDeckError;
use tokio::sync::mpsc;
use tokio::sync::Mutex;

/// Events that are coming from the host
#[derive(Clone, Copy, Debug)]
pub enum HostEvent {
    /// The button was pressed
    ButtonPressed,
    /// The button was released
    ButtonReleased,
    /// The channel was initialized and there were no events yet
    Init,
}

/// starts a module
pub async fn start_module(
    button: Button,
    device: Arc<AsyncStreamDeck>,
    br: Arc<Mutex<mpsc::Receiver<HostEvent>>>,
) {
    trace!("Starting MODULE {}", button.index);
    let b = button.clone();
    let da = DeviceAccess::new(device, button.index).await;
    let module = match button.module.as_str() {
        "counter" => Counter::run(da, br, b),
        _ => {
            error!("Module \'{}\' does not exist", button.module);
            Blank::run(da, br, b)
        }
    };

    match module.await {
        Ok(_) => info!("MODULE {} closed", button.index),
        Err(e) => error!("MODULE {}: {:?}", button.index, e),
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

    /// write a raw image to the Deck
    pub async fn write_raw_img(&self, img: &[u8]) -> Result<(), StreamDeckError> {
        self.streamdeck.write_image(self.index, img).await
    }

    /// Write an image to the Deck
    pub async fn write_img(&self, img: DynamicImage) -> Result<(), StreamDeckError> {
        self.streamdeck.set_button_image(self.index, img).await
    }

    /// reset the image
    pub async fn clear_img(&self) -> Result<(), StreamDeckError> {
        self.streamdeck.clear_button_image(self.index).await
    }

    pub fn format(&self) -> ImageFormat {
        self.kind.key_image_format()
    }

    /// The resolution of the image on the Deck
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
