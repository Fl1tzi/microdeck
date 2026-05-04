use super::prelude::*;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use sysinfo::{CpuRefreshKind, Disks, System};

/// Configurable system metrics module
pub struct SystemMetrics {
    config: SystemMetricsConfig,
    system: System,
}

struct SystemMetricsConfig {
    update_interval_ms: u64,
    show_cpu: bool,
    show_memory: bool,
    show_disk: bool,
    cpu_core: Option<usize>,
    text_format: String,
}

#[async_trait]
impl Module for SystemMetrics {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let update_interval_ms = config.parse_module("update_interval_ms", 5000u64).res()?;

        let show_cpu = config.parse_module("show_cpu", true).res()?;

        let show_memory = config.parse_module("show_memory", true).res()?;

        let show_disk = config.parse_module("show_disk", false).res()?;

        let cpu_core_raw: i64 = config.parse_module("cpu_core", -1i64).res()?;
        let cpu_core = if cpu_core_raw >= 0 {
            Some(cpu_core_raw as usize)
        } else {
            None
        };

        let text_format = config
            .parse_module("text_format", "CPU:{cpu}%\nMEM:{mem}%".to_string())
            .res()?;

        let mut system = System::new_all();
        system.refresh_all();

        Ok(Box::new(SystemMetrics {
            config: SystemMetricsConfig {
                update_interval_ms,
                show_cpu,
                show_memory,
                show_disk,
                cpu_core,
                text_format,
            },
            system,
        }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        _button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        let res = streamdeck.resolution();

        loop {
            self.system
                .refresh_cpu_specifics(CpuRefreshKind::everything());
            self.system.refresh_memory();

            let mut text_parts = Vec::new();

            if self.config.show_cpu {
                let cpu_usage = if let Some(core_idx) = self.config.cpu_core {
                    if core_idx < self.system.cpus().len() {
                        self.system.cpus()[core_idx].cpu_usage()
                    } else {
                        0.0
                    }
                } else {
                    self.system.global_cpu_usage()
                };
                text_parts.push(format!("{:.0}", cpu_usage));
            }

            if self.config.show_memory {
                let used_memory = self.system.used_memory();
                let total_memory = self.system.total_memory();
                let mem_usage = (used_memory as f64 / total_memory as f64) * 100.0;
                text_parts.push(format!("{:.0}", mem_usage));
            }

            if self.config.show_disk {
                let disks = Disks::new_with_refreshed_list();
                if !disks.is_empty() {
                    let disk = &disks[0];
                    let used = disk.total_space() - disk.available_space();
                    let usage = (used as f64 / disk.total_space() as f64) * 100.0;
                    text_parts.push(format!("{:.0}", usage));
                }
            }

            let text = self.config.text_format.clone();
            let formatted_text = text
                .replace("{cpu}", &text_parts.get(0).cloned().unwrap_or_default())
                .replace("{mem}", &text_parts.get(1).cloned().unwrap_or_default())
                .replace("{disk}", &text_parts.get(2).cloned().unwrap_or_default());

            let image = ImageBuilder::new(res.0, res.1)
                .set_text(formatted_text)
                .build()
                .await;

            streamdeck.write_img(image).await?;

            tokio::time::sleep(Duration::from_millis(self.config.update_interval_ms)).await;
        }
    }
}
