use super::prelude::*;
use crate::GLOBAL_FONT;
use async_trait::async_trait;
use image::{DynamicImage, Rgba, RgbaImage};
use imageproc::drawing::draw_hollow_circle_mut;
use imageproc::drawing::draw_text_mut;
use lazy_static::lazy_static;
use rusttype::Scale;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

lazy_static! {
    static ref GLOBAL_STATE: Arc<Mutex<GlobalPomodoroState>> =
        Arc::new(Mutex::new(GlobalPomodoroState::new()));
}

struct GlobalPomodoroState {
    current_phase: Phase,
    remaining: Duration,
    start_time: Option<Instant>,
    cycles_completed: usize,
    is_running: bool,
    config: PomodoroConfig,
}

impl GlobalPomodoroState {
    fn new() -> Self {
        Self {
            current_phase: Phase::Work,
            remaining: Duration::from_secs(0),
            start_time: None,
            cycles_completed: 0,
            is_running: false,
            config: PomodoroConfig {
                work_duration: 25 * 60,
                break_duration: 5 * 60,
                long_break_duration: 15 * 60,
                cycles_before_long_break: 4,
            },
        }
    }

    fn current_phase_duration(&self) -> Duration {
        match self.current_phase {
            Phase::Work => Duration::from_secs(self.config.work_duration),
            Phase::ShortBreak => Duration::from_secs(self.config.break_duration),
            Phase::LongBreak => Duration::from_secs(self.config.long_break_duration),
        }
    }

    fn toggle(&mut self, now: Instant) {
        if self.is_running {
            self.is_running = false;
            self.start_time = None;
        } else {
            if self.remaining == Duration::from_secs(0) {
                self.advance_phase();
            }
            self.is_running = true;
            self.start_time = Some(now);
        }
    }

    fn cancel(&mut self) {
        self.is_running = false;
        self.start_time = None;
        self.current_phase = Phase::Work;
        self.remaining = Duration::from_secs(self.config.work_duration);
        self.cycles_completed = 0;
    }

    fn update(&mut self, now: Instant) {
        if !self.is_running {
            return;
        }

        if let Some(start) = self.start_time {
            let elapsed = now.duration_since(start);
            if elapsed >= self.remaining {
                self.remaining = Duration::from_secs(0);
                self.advance_phase();
                self.start_time = Some(now);
            } else {
                self.remaining -= elapsed;
                self.start_time = Some(now);
            }
        }
    }

    fn advance_phase(&mut self) {
        match self.current_phase {
            Phase::Work => {
                self.cycles_completed += 1;
                if self.cycles_completed >= self.config.cycles_before_long_break {
                    self.current_phase = Phase::LongBreak;
                    self.remaining = Duration::from_secs(self.config.long_break_duration);
                    self.cycles_completed = 0;
                } else {
                    self.current_phase = Phase::ShortBreak;
                    self.remaining = Duration::from_secs(self.config.break_duration);
                }
            }
            Phase::ShortBreak | Phase::LongBreak => {
                self.current_phase = Phase::Work;
                self.remaining = Duration::from_secs(self.config.work_duration);
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Work,
    ShortBreak,
    LongBreak,
}

struct PomodoroConfig {
    work_duration: u64,
    break_duration: u64,
    long_break_duration: u64,
    cycles_before_long_break: usize,
}

pub struct Pomodoro {
    global_state: Arc<Mutex<GlobalPomodoroState>>,
    button_pressed_at: Option<Instant>,
}

impl Pomodoro {
    async fn generate_image(&self, width: usize, height: usize) -> DynamicImage {
        let state = self.global_state.lock().await;

        let mut img = RgbaImage::new(width as u32, height as u32);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 255]);
        }

        let center_x = width as i32 / 2;
        let center_y = height as i32 / 2;
        let outer_radius = (width.min(height) / 2) as i32 - 2;
        let inner_radius = outer_radius - 3;

        let progress_color = match state.current_phase {
            Phase::Work => Rgba([255, 80, 80, 255]),
            Phase::ShortBreak => Rgba([80, 255, 80, 255]),
            Phase::LongBreak => Rgba([80, 80, 255, 255]),
        };

        let total_duration = state.current_phase_duration();
        let progress = if total_duration.as_secs_f32() > 0.0 {
            1.0 - (state.remaining.as_secs_f32() / total_duration.as_secs_f32())
        } else {
            0.0
        };

        let end_angle = progress * 2.0 * std::f32::consts::PI - std::f32::consts::PI / 2.0;
        let num_segments = 72;
        for i in 0..=num_segments {
            let angle1 = (i as f32 / num_segments as f32) * 2.0 * std::f32::consts::PI
                - std::f32::consts::PI / 2.0;
            let angle2 = ((i + 1) as f32 / num_segments as f32) * 2.0 * std::f32::consts::PI
                - std::f32::consts::PI / 2.0;

            if angle1 <= end_angle {
                for r in inner_radius..=outer_radius {
                    let x1 = (center_x as f32 + r as f32 * angle1.cos()).round() as i32;
                    let y1 = (center_y as f32 + r as f32 * angle1.sin()).round() as i32;
                    let x2 = (center_x as f32 + r as f32 * angle2.cos()).round() as i32;
                    let y2 = (center_y as f32 + r as f32 * angle2.sin()).round() as i32;

                    if x1 >= 0 && x1 < width as i32 && y1 >= 0 && y1 < height as i32 {
                        img.put_pixel(x1 as u32, y1 as u32, progress_color);
                    }
                    if x2 >= 0 && x2 < width as i32 && y2 >= 0 && y2 < height as i32 {
                        img.put_pixel(x2 as u32, y2 as u32, progress_color);
                    }
                }
            }
        }

        let time_str = format_time(state.remaining);
        let phase_str = match state.current_phase {
            Phase::Work => "W",
            Phase::ShortBreak => "B",
            Phase::LongBreak => "L",
        };
        let display_text = format!("{} {}", phase_str, time_str);

        let font = &GLOBAL_FONT.get().unwrap();
        let font_scale = Scale::uniform(12.0);
        let text_color = Rgba([255, 255, 255, 255]);

        let text_width: f32 = display_text
            .chars()
            .map(|c| font.glyph(c).scaled(font_scale).h_metrics().advance_width)
            .sum();

        let text_x = center_x - (text_width / 2.0).round() as i32;
        let text_y = center_y - 4;

        draw_text_mut(
            &mut img,
            text_color,
            text_x,
            text_y,
            font_scale,
            font,
            &display_text,
        );

        DynamicImage::ImageRgba8(img)
    }
}

fn format_time(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    format!("{:02}:{:02}", minutes, seconds)
}

#[async_trait]
impl Module for Pomodoro {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let mut state = GLOBAL_STATE.lock().await;

        if state.config.work_duration == 0 {
            state.config.work_duration = config.parse_module("work_duration", 25u64 * 60).res()?;
            state.config.break_duration = config.parse_module("break_duration", 5u64 * 60).res()?;
            state.config.long_break_duration = config
                .parse_module("long_break_duration", 15u64 * 60)
                .res()?;
            state.config.cycles_before_long_break = config
                .parse_module("cycles_before_long_break", 4usize)
                .res()?;

            if state.remaining == Duration::from_secs(0) {
                state.remaining = Duration::from_secs(state.config.work_duration);
            }
        }

        Ok(Box::new(Pomodoro {
            global_state: GLOBAL_STATE.clone(),
            button_pressed_at: None,
        }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        mut button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        let res = streamdeck.resolution();
        let width = res.0;
        let height = res.1;

        let image = self.generate_image(width, height).await;
        streamdeck.write_img(image).await?;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    let mut state = self.global_state.lock().await;
                    if state.is_running {
                        state.update(Instant::now());
                        drop(state);
                        let image = self.generate_image(width, height).await;
                        streamdeck.write_img(image).await?;
                    }
                }
                _ = button_receiver.recv() => {
                    let now = Instant::now();
                    if let Some(pressed_at) = self.button_pressed_at {
                        let hold_duration = now.duration_since(pressed_at);
                        let mut state = self.global_state.lock().await;
                        if hold_duration >= Duration::from_secs(1) {
                            state.cancel();
                        } else {
                            state.toggle(now);
                        }
                        drop(state);
                        self.button_pressed_at = None;
                    } else {
                        self.button_pressed_at = Some(now);
                    }
                    let image = self.generate_image(width, height).await;
                    streamdeck.write_img(image).await?;
                }
            }
        }
    }
}
