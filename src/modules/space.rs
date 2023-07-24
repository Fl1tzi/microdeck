use super::Button;
use super::ChannelReceiver;
use super::DeviceAccess;
use super::Module;
use super::ReturnError;
use async_trait::async_trait;
use std::sync::Arc;

/// module to represent the switching of a space (just visual)
pub struct Space;

#[async_trait]
impl Module for Space {
    async fn run(
        _streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
        _config: Arc<Button>,
    ) -> Result<(), ReturnError> {
        Ok(())
    }
}
