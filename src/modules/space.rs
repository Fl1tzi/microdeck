use super::prelude::*;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

/// module to represent the switching of a space (just visual)
pub struct Space {
    name: String,
    path: Option<PathBuf>,
}

#[async_trait]
impl Module for Space {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let name = config.parse_module("name", "Unknown".to_string()).res()?;
        let path_string = config.parse_module("path", "".to_string()).required().ok();
        let path: Option<PathBuf>;
        if let Some(string) = path_string {
            path = Some(PathBuf::from(string))
        } else {
            path = None;
        }
        Ok(Box::new(Space { name, path }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        // let icon = load_image(&config).unwrap();

        let res = streamdeck.resolution();
        let image;
        // custom image is set
        if let Some(path) = &self.path {
            image = ImageBuilder::new(res.0, res.1)
                .set_image(path.clone())
                .set_text(self.name.clone())
                .build()
                .await;
        } else {
            image = ImageBuilder::new(res.0, res.1)
                .set_folder_icon()
                .set_text(self.name.clone())
                .build()
                .await;
        }

        streamdeck.write_img(image).await.unwrap();
        Ok(())
    }
}
