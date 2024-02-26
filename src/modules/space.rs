use super::Button;
use super::ButtonConfigError;
use super::ChannelReceiver;
use super::DeviceAccess;
use super::Module;
use super::ModuleObject;
use super::ReturnError;
use crate::image_rendering::{create_error_image, ImageBuilder};
use async_trait::async_trait;
use std::sync::Arc;

/// module to represent the switching of a space (just visual)
pub struct Space {
    name: String,
}

#[async_trait]
impl Module for Space {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let name = config.parse_module("name", "Unknown".to_string()).res()?;
        Ok(Box::new(Space { name }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        // let icon = load_image(&config).unwrap();
        let icon = create_error_image();

        let res = streamdeck.resolution();
        let image = ImageBuilder::new(res.0, res.1)
            .set_image(icon)
            .set_text(self.name.clone())
            .build();

        streamdeck.write_img(image).await.unwrap();
        Ok(())
    }
}
