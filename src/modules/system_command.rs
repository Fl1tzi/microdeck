use super::prelude::*;
use crate::GLOBAL_FONT;
use async_trait::async_trait;
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;
use rusttype::Scale;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    scroll_offset: f32,
    text_width: f32,
    width: usize,
    height: usize,
}

#[async_trait]
impl Module for SystemCommand {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
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
            scroll_offset: 0.0,
            text_width: 0.0,
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
                    if self.text_width > self.width as f32 {
                        self.scroll_offset -= 1.0;
                        if self.scroll_offset <= -self.text_width {
                            self.scroll_offset = self.width as f32;
                        }
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
        let mut img = RgbaImage::new(width as u32, height as u32);

        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 255]);
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

        let font = &GLOBAL_FONT.get().unwrap();
        let text_color = Rgba([255, 255, 255, 255]);

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

        let title_scale = Scale::uniform(10.0);
        let text_scale = Scale::uniform(12.0);

        let status_text_width: f32 = status_text
            .chars()
            .map(|c| font.glyph(c).scaled(title_scale).h_metrics().advance_width)
            .sum();

        let display_text_width: f32 = if !display_text.is_empty() {
            display_text
                .chars()
                .map(|c| font.glyph(c).scaled(text_scale).h_metrics().advance_width)
                .sum()
        } else {
            0.0
        };

        self.text_width = status_text_width.max(display_text_width);
        let needs_scroll = self.text_width > width as f32;

        if needs_scroll && self.scroll_offset == 0.0 {
            self.scroll_offset = width as f32;
        } else if !needs_scroll {
            self.scroll_offset = 0.0;
        }

        let title_x = if needs_scroll {
            self.scroll_offset as i32
        } else {
            (width as i32 / 2) - (status_text_width / 2.0).round() as i32
        };
        let title_y = (height as f32 * 0.25) as i32;
        draw_text_mut(
            &mut img,
            text_color,
            title_x,
            title_y,
            title_scale,
            font,
            &status_text,
        );

        if !display_text.is_empty() {
            let display_x = if needs_scroll {
                self.scroll_offset as i32
            } else {
                (width as i32 / 2) - (display_text_width / 2.0).round() as i32
            };
            let display_y = (height as f32 * 0.6) as i32;

            draw_text_mut(
                &mut img,
                text_color,
                display_x,
                display_y,
                text_scale,
                font,
                &display_text,
            );
        }

        DynamicImage::ImageRgba8(img)
    }
}
