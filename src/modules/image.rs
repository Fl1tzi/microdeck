use super::Button;
use super::ButtonConfigError;
use super::ChannelReceiver;
use super::DeviceAccess;
use super::Module;
use super::ModuleCache;
use super::ModuleObject;
use super::ReturnError;
use crate::image_rendering::ImageBuilder;
use async_trait::async_trait;
use image::DynamicImage;
use std::sync::Arc;

pub struct Image {
    image: Arc<DynamicImage>,
    scale: f32,
}

#[async_trait]
impl Module for Image {
    async fn init(
        config: Arc<Button>,
        mut cache: ModuleCache,
    ) -> Result<ModuleObject, ButtonConfigError> {
        let path = config.parse_module("PATH", String::new()).required()?;
        let scale = config.parse_module("SCALE", 100.0).res()?;

        let image = cache
            .load_image(path, 1)
            .await
            .ok_or(ButtonConfigError::General(
                "Image was not found".to_string(),
            ))?;

        Ok(Box::new(Image { image, scale }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        let (h, w) = streamdeck.resolution();
        let img = (*self.image).clone();
        let img = ImageBuilder::new(h, w)
            .set_image(img)
            .set_image_scale(self.scale)
            .build();
        streamdeck.write_img(img).await.unwrap();
        Ok(())
    }
}
