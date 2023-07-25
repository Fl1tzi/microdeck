use super::Button;
use super::ChannelReceiver;
use super::DeviceAccess;
use super::Module;
use super::ReturnError;
use super::create_error_image;
use super::load_image;
use super::parse_config;
use async_trait::async_trait;
use std::sync::Arc;

/// module to represent the switching of a space (just visual)
pub struct Space;

#[async_trait]
impl Module for Space {
    async fn run(
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
        config: Arc<Button>,
    ) -> Result<(), ReturnError> {
        // let icon = load_image(&config).unwrap();
        let icon = create_error_image();

        let image = streamdeck.image_with_text(icon, parse_config(&config, &"NAME".into(), "Unknown".to_string()), &config);

        streamdeck.write_img(image).await.unwrap();
        Ok(())
    }
}
