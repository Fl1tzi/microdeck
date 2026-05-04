use super::prelude::*;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct Pomodoro {
    config: PomodoroConfig,
    state: PomodoroState,
    button_pressed_at: Option<Instant>,
}

struct PomodoroConfig {
    work_duration: u64,
    break_duration: u64,
    long_break_duration: u64,
    cycles_before_long_break: usize,
}

struct PomodoroState {
    current_phase: Phase,
    start_time: Option<Instant>,
    remaining: Duration,
    cycles_completed: usize,
    is_running: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Work,
    ShortBreak,
    LongBreak,
}

impl Pomodoro {
    fn toggle(&mut self) {
        if self.state.is_running {
            self.state.is_running = false;
            self.state.start_time = None;
        } else {
            if self.state.remaining == Duration::from_secs(0) {
                self.advance_phase();
            }
            self.state.is_running = true;
            self.state.start_time = Some(Instant::now());
        }
    }

    fn cancel(&mut self) {
        self.state.is_running = false;
        self.state.start_time = None;
        self.state.current_phase = Phase::Work;
        self.state.remaining = Duration::from_secs(self.config.work_duration);
        self.state.cycles_completed = 0;
    }

    fn update(&mut self, now: Instant) {
        if !self.state.is_running {
            return;
        }

        if let Some(start) = self.state.start_time {
            let elapsed = now.duration_since(start);
            if elapsed >= self.state.remaining {
                self.state.remaining = Duration::from_secs(0);
                self.advance_phase();
                self.state.start_time = Some(now);
            } else {
                self.state.remaining -= elapsed;
                self.state.start_time = Some(now);
            }
        }
    }

    fn advance_phase(&mut self) {
        match self.state.current_phase {
            Phase::Work => {
                self.state.cycles_completed += 1;
                if self.state.cycles_completed >= self.config.cycles_before_long_break {
                    self.state.current_phase = Phase::LongBreak;
                    self.state.remaining = Duration::from_secs(self.config.long_break_duration);
                    self.state.cycles_completed = 0;
                } else {
                    self.state.current_phase = Phase::ShortBreak;
                    self.state.remaining = Duration::from_secs(self.config.break_duration);
                }
            }
            Phase::ShortBreak | Phase::LongBreak => {
                self.state.current_phase = Phase::Work;
                self.state.remaining = Duration::from_secs(self.config.work_duration);
            }
        }
    }

    async fn generate_image(&self, width: usize, height: usize) -> DynamicImage {
        let time_str = self.format_time(self.state.remaining);
        let phase_char = match self.state.current_phase {
            Phase::Work => "W",
            Phase::ShortBreak => "B",
            Phase::LongBreak => "L",
        };

        let total_duration = self.current_phase_duration();
        let progress = if total_duration.as_secs_f32() > 0.0 {
            self.state.remaining.as_secs_f32() / total_duration.as_secs_f32()
        } else {
            0.0
        };

        let bar_length = (width / 12).min(10);
        let filled = (progress * bar_length as f32).round() as usize;
        let empty = bar_length.saturating_sub(filled);
        let progress_bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(empty));

        let display_text = format!("{} {} {}", phase_char, time_str, progress_bar);

        ImageBuilder::new(width, height)
            .set_text(display_text)
            .build()
            .await
    }

    fn current_phase_duration(&self) -> Duration {
        match self.state.current_phase {
            Phase::Work => Duration::from_secs(self.config.work_duration),
            Phase::ShortBreak => Duration::from_secs(self.config.break_duration),
            Phase::LongBreak => Duration::from_secs(self.config.long_break_duration),
        }
    }

    fn format_time(&self, duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{:02}:{:02}", minutes, seconds)
    }
}

#[async_trait]
impl Module for Pomodoro {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let work_duration = config
            .parse_module("work_duration", 25u64 * 60)
            .res()?;
        let break_duration = config
            .parse_module("break_duration", 5u64 * 60)
            .res()?;
        let long_break_duration = config
            .parse_module("long_break_duration", 15u64 * 60)
            .res()?;
        let cycles_before_long_break = config
            .parse_module("cycles_before_long_break", 4usize)
            .res()?;

        let now = Instant::now();
        Ok(Box::new(Pomodoro {
            config: PomodoroConfig {
                work_duration,
                break_duration,
                long_break_duration,
                cycles_before_long_break,
            },
            state: PomodoroState {
                current_phase: Phase::Work,
                start_time: Some(now),
                remaining: Duration::from_secs(work_duration),
                cycles_completed: 0,
                is_running: true,
            },
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
            let sleep_duration = Duration::from_secs(1);

            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {
                    if self.state.is_running {
                        self.update(Instant::now());
                        let image = self.generate_image(width, height).await;
                        streamdeck.write_img(image).await?;
                    }
                }
                _ = button_receiver.recv() => {
                    let now = Instant::now();
                    if let Some(pressed_at) = self.button_pressed_at {
                        let hold_duration = now.duration_since(pressed_at);
                        if hold_duration >= Duration::from_secs(1) {
                            self.cancel();
                        } else {
                            self.toggle();
                        }
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
