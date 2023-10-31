use super::Button;
use super::ButtonConfigError;
use super::ChannelReceiver;
use super::DeviceAccess;
use super::Module;
use super::ModuleObject;
use super::ReturnError;
use crate::image_rendering::{load_image, ImageBuilder};
use async_trait::async_trait;
use image::DynamicImage;
use std::sync::Arc;
use tokio::task;

pub struct Image {
    image: DynamicImage,
    scale: f32,
}

#[async_trait]
impl Module for Image {
    async fn init(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let path = config.parse_module("PATH", String::new()).required()?;
        let scale = config.parse_module("SCALE", 100.0).res()?;

        // TODO: decoding takes really long sometimes. Maybe this can be cached?
        let image = task::spawn_blocking(move || {
            load_image(path)
                .map_err(|_| ButtonConfigError::General("Image was not found.".to_string()))
        })
        .await
        .unwrap()?;

        Ok(Box::new(Image { image, scale }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        streamdeck.write_img(self.image.clone()).await.unwrap();
        Ok(())
    }
}
