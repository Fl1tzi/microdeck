use super::prelude::*;
use crate::image_rendering::{draw_text_on_image, wrap_text};
use crate::GLOBAL_FONT;
use async_trait::async_trait;
use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;
use rusttype::Scale;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

// TODO: Remember state when switching spaces.

#[derive(Clone, Copy, PartialEq)]
enum CommandStatus {
    Idle,
    Running,
    Finished,
    Error,
}

pub struct SystemCommand {
    command: String,
    title: String,
    show_output: bool,
    last_output: String,
    last_exit_code: i32,
    status: CommandStatus,
    child_process: Option<Child>,
    button_pressed_at: Option<Instant>,
    width: usize,
    height: usize,
}

#[async_trait]
impl Module for SystemCommand {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        println!("NOTE that the system_command module is incomplete and can lead to processes not attached to the button. Use with caution.");
        let command = config
            .parse_module("command", "echo Hello".to_string())
            .res()?;
        let title = config.parse_module("title", "Command".to_string()).res()?;
        let show_output = config.parse_module("show_output", false).res()?;

        Ok(Box::new(SystemCommand {
            command,
            title,
            show_output,
            last_output: String::new(),
            last_exit_code: 0,
            status: CommandStatus::Idle,
            child_process: None,
            button_pressed_at: None,
            width: 0,
            height: 0,
        }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        mut button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        let res = streamdeck.resolution();
        self.width = res.0;
        self.height = res.1;

        let image = self.generate_image().await;
        streamdeck.write_img(image).await?;

        loop {
            if let Some(ref mut child) = self.child_process {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        self.last_exit_code = status.code().unwrap_or(1);
                        self.status = if self.last_exit_code == 0 {
                            CommandStatus::Finished
                        } else {
                            CommandStatus::Error
                        };
                        self.child_process = None;
                        let image = self.generate_image().await;
                        streamdeck.write_img(image).await?;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        self.status = CommandStatus::Error;
                        self.last_output = "Wait error".to_string();
                        self.last_exit_code = 1;
                        self.child_process = None;
                        let image = self.generate_image().await;
                        streamdeck.write_img(image).await?;
                    }
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    if let Some(ref mut child) = self.child_process {
                        let image = self.generate_image().await;
                        streamdeck.write_img(image).await?;
                    }
                }
                _ = button_receiver.recv() => {
                    let now = Instant::now();
                    if let Some(pressed_at) = self.button_pressed_at {
                        let hold_duration = now.duration_since(pressed_at);
                        if hold_duration >= Duration::from_secs(1) {
                            if self.status == CommandStatus::Running {
                                self.cancel_command();
                            }
                        } else {
                            self.execute_command();
                        }
                        self.button_pressed_at = None;
                    } else {
                        self.button_pressed_at = Some(now);
                    }
                    let image = self.generate_image().await;
                    streamdeck.write_img(image).await?;
                }
            }
        }
    }
}

impl SystemCommand {
    fn execute_command(&mut self) {
        self.status = CommandStatus::Running;
        self.last_output.clear();
        self.last_exit_code = 0;

        if let Some(mut child) = self.child_process.take() {
            let _ = child.kill();
        }

        let mut command = if cfg!(target_os = "windows") {
            let mut cmd = Command::new("cmd");
            cmd.args(["/C", &self.command]);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.arg("-c").arg(&self.command);
            cmd
        };

        if self.show_output {
            command.stdout(Stdio::piped());
            command.stderr(Stdio::piped());
        }

        match command.spawn() {
            Ok(child) => {
                self.child_process = Some(child);
            }
            Err(e) => {
                self.status = CommandStatus::Error;
                self.last_output = format!("Failed: {}", e);
                self.last_exit_code = 1;
            }
        }
    }

    fn cancel_command(&mut self) {
        if let Some(mut child) = self.child_process.take() {
            let _ = child.kill();
            self.status = CommandStatus::Idle;
            self.last_output = "Cancelled".to_string();
            self.last_exit_code = 0;
        }
    }

    async fn generate_image(&mut self) -> DynamicImage {
        let width = self.width;
        let height = self.height;

        let status_text = match self.status {
            CommandStatus::Idle => self.title.clone(),
            CommandStatus::Running => format!("{} - Running", self.title),
            CommandStatus::Finished => format!("{} - Done", self.title),
            CommandStatus::Error => format!("{} - Error", self.title),
        };

        let display_text = if self.show_output && !self.last_output.is_empty() {
            self.last_output.clone()
        } else {
            String::new()
        };

        let mut full_text = status_text;
        if !display_text.is_empty() {
            full_text.push('\n');
            full_text.push_str(&display_text);
        }

        let wrapped_text = wrap_text(width as u32, Scale::uniform(10.0), &full_text);

        let text_with_offset = format!("\n{}", wrapped_text);

        let mut rgb_img = RgbImage::new(width as u32, height as u32);
        for pixel in rgb_img.pixels_mut() {
            *pixel = Rgb([0, 0, 0]);
        }

        let text_color = Rgb([255, 255, 255]);
        let rgb_img =
            draw_text_on_image(text_with_offset, rgb_img, text_color, Scale::uniform(10.0));

        let mut img = RgbaImage::new(width as u32, height as u32);
        for (x, y, pixel) in rgb_img.enumerate_pixels() {
            img.put_pixel(x, y, Rgba([pixel[0], pixel[1], pixel[2], 255]));
        }

        let bar_color = match self.status {
            CommandStatus::Idle => Rgba([60, 60, 60, 255]),
            CommandStatus::Running => Rgba([255, 200, 0, 255]),
            CommandStatus::Finished => Rgba([0, 255, 0, 255]),
            CommandStatus::Error => Rgba([255, 0, 0, 255]),
        };
        for x in 0..width as u32 {
            img.put_pixel(x, 0, bar_color);
        }

        DynamicImage::ImageRgba8(img)
    }
}
