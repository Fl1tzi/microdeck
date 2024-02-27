use super::prelude::*;
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
            .set_image(self.path.clone())
            .set_image_scale(self.scale)
            .build()
            .await;
        streamdeck.write_img(img).await.unwrap();
        Ok(())
    }
}
