use super::prelude::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::Networks;

/// Network usage module showing upload/download speeds
pub struct NetworkUsage {
    config: NetworkUsageConfig,
    previous_stats: HashMap<String, NetworkStats>,
    last_update: Option<Instant>,
    initialized: bool,
}

struct NetworkUsageConfig {
    update_interval_ms: u64,
    interface: Option<String>,
    show_upload: bool,
    show_download: bool,
    text_format: String,
    font_size: f32,
}

#[derive(Clone, Copy)]
struct NetworkStats {
    received: u64,
    transmitted: u64,
}

#[async_trait]
impl Module for NetworkUsage {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let update_interval_ms = config.parse_module("update_interval_ms", 2000u64).res()?;

        let interface_name: String = config.parse_module("interface", "".to_string()).res()?;
        let interface = if interface_name.is_empty() {
            None
        } else {
            Some(interface_name)
        };

        let show_upload = config.parse_module("show_upload", true).res()?;

        let show_download = config.parse_module("show_download", true).res()?;

        let text_format = config
            .parse_module("text_format", "↓\n{down}\n↑\n{up}".to_string())
            .res()?;

        let font_size = config.parse_module("font_size", 13.0).res()?;

        Ok(Box::new(NetworkUsage {
            config: NetworkUsageConfig {
                update_interval_ms,
                interface,
                show_upload,
                show_download,
                text_format,
                font_size,
            },
            previous_stats: HashMap::new(),
            last_update: None,
            initialized: false,
        }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        let res = streamdeck.resolution();

        loop {
            let networks = Networks::new_with_refreshed_list();
            let now = Instant::now();

            let interval_secs = if let Some(last) = self.last_update {
                now.duration_since(last).as_secs_f64()
            } else {
                1.0
            };

            let mut text_parts = Vec::new();

            for (name, interface) in networks.list() {
                if self.config.interface.as_ref().map_or(true, |i| i == name) {
                    let current_received = interface.total_received();
                    let current_transmitted = interface.total_transmitted();

                    let previous = self
                        .previous_stats
                        .get(name)
                        .copied()
                        .unwrap_or(NetworkStats {
                            received: current_received,
                            transmitted: current_transmitted,
                        });

                    if !self.initialized {
                        self.previous_stats.insert(
                            name.clone(),
                            NetworkStats {
                                received: current_received,
                                transmitted: current_transmitted,
                            },
                        );
                        self.last_update = Some(now);
                        self.initialized = true;
                        continue;
                    }

                    if self.config.show_download {
                        let received_delta =
                            current_received.saturating_sub(previous.received) as f64;
                        let download_speed = received_delta / interval_secs.max(0.001);
                        text_parts.push(format_speed(download_speed));
                    }

                    if self.config.show_upload {
                        let transmitted_delta =
                            current_transmitted.saturating_sub(previous.transmitted) as f64;
                        let upload_speed = transmitted_delta / interval_secs.max(0.001);
                        text_parts.push(format_speed(upload_speed));
                    }

                    self.previous_stats.insert(
                        name.clone(),
                        NetworkStats {
                            received: current_received,
                            transmitted: current_transmitted,
                        },
                    );
                }
            }

            self.last_update = Some(now);

            let text = self.config.text_format.clone();
            let formatted_text = text
                .replace(
                    "{down}",
                    &text_parts
                        .get(0)
                        .cloned()
                        .unwrap_or_else(|| "- B/s".to_string()),
                )
                .replace(
                    "{up}",
                    &text_parts
                        .get(1)
                        .cloned()
                        .unwrap_or_else(|| "- B/s".to_string()),
                );

            let image = ImageBuilder::new(res.0, res.1)
                .set_text(formatted_text)
                .set_font_size(self.config.font_size)
                .build()
                .await;

            streamdeck.write_img(image).await?;

            tokio::time::sleep(Duration::from_millis(self.config.update_interval_ms)).await;
        }
    }
}

fn format_speed(bytes_per_second: f64) -> String {
    if bytes_per_second < 1024.0 {
        format!("{:.0} B/s", bytes_per_second)
    } else if bytes_per_second < 1024.0 * 1024.0 {
        format!("{:.1} KB/s", bytes_per_second / 1024.0)
    } else if bytes_per_second < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes_per_second / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB/s", bytes_per_second / (1024.0 * 1024.0 * 1024.0))
    }
}
