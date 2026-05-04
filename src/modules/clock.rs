use super::prelude::*;
use async_trait::async_trait;
use chrono::Local;
use std::sync::Arc;
use std::time::Duration;

/// Clock module displaying current time and optionally date
pub struct Clock {
    config: ClockConfig,
}

struct ClockConfig {
    update_interval_ms: u64,
    time_format: String,
    show_date: bool,
    date_format: String,
}

#[async_trait]
impl Module for Clock {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let update_interval_ms = config.parse_module("update_interval_ms", 1000u64).res()?;

        let time_format = config
            .parse_module("time_format", "%H:%M:%S".to_string())
            .res()?;

        let show_date = config.parse_module("show_date", false).res()?;

        let date_format = config
            .parse_module("date_format", "%Y %m-%d".to_string())
            .res()?;

        Ok(Box::new(Clock {
            config: ClockConfig {
                update_interval_ms,
                time_format,
                show_date,
                date_format,
            },
        }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        let res = streamdeck.resolution();

        loop {
            let now = Local::now();
            let time_str = now.format(&self.config.time_format).to_string();

            let text = if self.config.show_date {
                let date_str = now.format(&self.config.date_format).to_string();
                format!("{} {}", time_str, date_str)
            } else {
                time_str
            };

            let image = ImageBuilder::new(res.0, res.1).set_text(text).build().await;

            streamdeck.write_img(image).await?;

            tokio::time::sleep(Duration::from_millis(self.config.update_interval_ms)).await;
        }
    }
}
