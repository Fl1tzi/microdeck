use std::sync::Arc;

use crate::config::Button;

use super::Module;
use super::{ChannelReceiver, DeviceAccess, HostEvent, ReturnError};

use async_trait::async_trait;

/// A module which displays a counter
pub struct Counter;

#[async_trait]
impl Module for Counter {
    async fn run(
        streamdeck: DeviceAccess,
        button_receiver: ChannelReceiver,
        config: Arc<Button>,
    ) -> Result<(), ReturnError> {
        let mut button_receiver = button_receiver;

        streamdeck.clear_img().await.unwrap();

        let mut counter: u32 = 0;

        // render the 0 at the beginning
        let image = streamdeck.text(counter.to_string(), &config);
        streamdeck.write_img(image).await.unwrap();

        loop {
            if let Some(event) = button_receiver.recv().await {
                match event {
                    HostEvent::ButtonPressed => {
                        counter += 1;
                        let image = streamdeck.text(counter.to_string(), &config);
                        streamdeck.write_img(image).await.unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
}
