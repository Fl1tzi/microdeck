use super::Button;
use super::ChannelReceiver;
use super::DeviceAccess;
use super::Module;
use super::ReturnError;
use async_trait::async_trait;

pub struct Blank;

#[async_trait]
impl Module for Blank {
    async fn run(
        _streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
        _config: Button,
    ) -> Result<(), ReturnError> {
        Ok(())
    }
}
