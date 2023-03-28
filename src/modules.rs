mod blank;
mod counter;

use std::{
    collections::HashMap, error::Error, future::Future, hash::Hash, pin::Pin, sync::Arc,
    thread::JoinHandle,
};

use crate::{Button, DeviceConfig};
use async_trait::async_trait;
pub use elgato_streamdeck as streamdeck;
use futures_util::TryFutureExt;
use image::DynamicImage;
use log::{debug, error, info, trace, warn};
pub use streamdeck::info::ImageFormat;
use streamdeck::AsyncStreamDeck;
pub use streamdeck::StreamDeckError;
use streamdeck::info::Kind;
use tokio::sync::mpsc;
use tokio::{
    sync::Mutex,
    time::{sleep, Duration},
};

use self::blank::Blank;
use self::counter::Counter;
use lazy_static::lazy_static;

type ModuleFunction = Box<dyn Fn(DeviceAccess, ChannelReceiver, Button) -> Result<(), ReturnError>>;

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
    /*
    match Counter::run(device_access, button_receiver, button.clone()).await {
        Ok(_) => info!("MODULE {} closed", button.index),
        Err(e) => error!("MODULE {}: {:?}", button.index, e)
    }*/
}
/* #[derive(Clone)]
pub struct Bridge;

impl Bridge {

    /*pub async fn start(&mut self) {
        /*tokio::join!(self.listener(), ColorModule::run(channel, button))
            .1
            .unwrap();*/
        /*tokio::select! {
            v = ColorModule::run(self.button_receiver, button) => {
                match v {
                    Ok(_) => info!("MODULE {} closed", self.button.index),
                    Err(e) => error!("MODULE {}: {:?}", self.button.index, e)
                }
            },
            _ = self.listener() => {}
        }*/

        /*tokio::spawn(ColorModule::run(channel, button));
        loop {
            self.listener().await;
            sleep(Duration::from_millis(10)).await;
        }*/
    }*/

    /* pub async fn listener(&mut self) {
        loop {
            if let Ok(event) = self.host_receiver.try_recv() {
                trace!("MODULE {}: {:?}", self.button.index, event);
                match event {
                    ModuleEvent::Subscribe(e) => self.events.push(e),
                    ModuleEvent::Image(i) => {
                        println!("UPLOADED IMAGE")
                    }
                }
            }
            sleep(Duration::from_millis(20)).await;
        }
    }*/

    pub async fn start_module<F, Fut>(&self, config: Button, module: F)
    where
        // Has to be the same as [Module::run]
        F: FnOnce(Receiver<HostEvent>, Button) -> Fut,
        Fut: Future<Output = Result<(), Box<dyn Error>>>,
    {
        //let channel = ModuleChannel::new(self.sender.clone(), self.receiver.clone()).await;
        //module(channel, config).await;
    }
}*/

/*
/// A wrapper around the channel to provide easier communication for the module. It is designed to
/// be not blocking.
pub struct ModuleChannel {
    pub sender: Sender<ModuleEvent>,
    pub receiver: Receiver<HostEvent>,
}

#[derive(Debug)]
pub enum ModuleChannelError {
    /// The host disconnected
    HostDisconnected,
    /// The message buffer is full. Either the host or the module is not consuming
    /// messages.
    BufferFull,
    /// The message buffer does not include any events for the module
    Empty,
}*/

/* impl ModuleChannel {
    pub async fn new(
        sender: Sender<ModuleEvent>,
        receiver: Receiver<HostEvent>,
    ) -> ModuleChannel {
        ModuleChannel { sender, receiver }
    }

    /// Send a message without blocking - [crossbeam_channel::Sender::try_send]
    pub async fn send(&self, message: ModuleEvent) -> Result<(), ModuleChannelError> {
        self.sender
            .try_send(message)
            .map_err(|e| match e {
                crossbeam_channel::TrySendError::Full(_) => ModuleChannelError::BufferFull,
                crossbeam_channel::TrySendError::Disconnected(_) => {
                    ModuleChannelError::HostDisconnected
                }
            })
    }

    /// Receive messages without blocking - [crossbeam_channel::Receiver::try_recv]
    pub async fn receive(&self) -> Result<HostEvent, ModuleChannelError> {
        self.receiver.try_recv().map_err(|e| {
            match e {
                crossbeam_channel::TryRecvError::Empty => ModuleChannelError::Empty,
                crossbeam_channel::TryRecvError::Disconnected => {
                    ModuleChannelError::HostDisconnected
                }
            }
        })
    }

    pub async fn update_image(&self, img: DynamicImage) {}
}*/

/// Wrapper to provide easier access to the Deck
pub struct DeviceAccess {
    streamdeck: Arc<AsyncStreamDeck>,
    kind: Kind,
    index: u8,
}

impl DeviceAccess {
    pub async fn new(streamdeck: Arc<AsyncStreamDeck>, index: u8) -> DeviceAccess {
        let kind = streamdeck.kind();
        DeviceAccess { streamdeck, kind, index }
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
