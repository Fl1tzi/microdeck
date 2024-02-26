use super::Button;
use super::ButtonConfigError;
use super::ChannelReceiver;
use super::DeviceAccess;
use super::Module;
use super::ModuleObject;
use super::ReturnError;
use async_trait::async_trait;
use std::sync::Arc;

pub struct Blank;

#[async_trait]
impl Module for Blank {
    async fn new(_config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        Ok(Box::new(Blank {}))
    }

    async fn run(
        &mut self,
        _streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        Ok(())
    }
}
