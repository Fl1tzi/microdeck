use super::load_image;
use super::Button;
use super::ButtonConfigError;
use super::ChannelReceiver;
use super::DeviceAccess;
use super::Module;
use super::ModuleObject;
use super::ReturnError;
use crate::image_rendering::ImageBuilder;
use async_trait::async_trait;
use std::{path::PathBuf, sync::Arc};

pub struct Image {
    scale: f32,
    path: PathBuf,
}

#[async_trait]
impl Module for Image {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let path = config.parse_module("path", String::new()).required()?;
        let scale = config.parse_module("scale", 100.0).res()?;

        let path = PathBuf::from(path);
        if path.exists() == false {
            return Err(ButtonConfigError::General(
                "Image was not found".to_string(),
            ));
        };

        Ok(Box::new(Image { scale, path }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        let (h, w) = streamdeck.resolution();
        let img = ImageBuilder::new(h, w)
            .set_image(load_image(self.path.clone(), (h, w)).await.unwrap())
            .set_image_scale(self.scale)
            .build();
        streamdeck.write_img(img).await.unwrap();
        Ok(())
    }
}
