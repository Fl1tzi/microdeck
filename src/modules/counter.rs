use std::sync::Arc;

use crate::config::Button;

use super::{
    ButtonConfigError, ChannelReceiver, DeviceAccess, HostEvent, Module, ModuleObject, ReturnError,
};

use crate::image_rendering::wrap_text;
use crate::GLOBAL_FONT;
use image::{DynamicImage, Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;
use rusttype::Scale;

use async_trait::async_trait;

/// A module which displays a counter
pub struct Counter {
    title: String,
    title_size: f32,
    number_size: f32,
    increment: u32,
}

#[async_trait]
impl Module for Counter {
    async fn new(config: Arc<Button>) -> Result<ModuleObject, ButtonConfigError> {
        let title = config.parse_module("title", " ".to_string()).res()?;
        let title_size = config.parse_module("title_size", 15.0).res()?;
        let number_size = config.parse_module("number_size", 25.0).res()?;
        let increment = config.parse_module("increment", 1).res()?;

        Ok(Box::new(Counter {
            title,
            title_size,
            number_size,
            increment,
        }))
    }

    async fn run(
        &mut self,
        streamdeck: DeviceAccess,
        button_receiver: ChannelReceiver,
    ) -> Result<(), ReturnError> {
        let mut button_receiver = button_receiver;

        streamdeck.clear_img().await.unwrap();

        let mut counter: u32 = 0;

        // render the 0 at the beginning
        let image = render_text(
            &streamdeck,
            &self.title,
            &counter.to_string(),
            self.title_size,
            self.number_size,
        );
        streamdeck.write_img(image).await.unwrap();

        loop {
            if let Some(event) = button_receiver.recv().await {
                match event {
                    HostEvent::ButtonPressed => {
                        // just return to zero if u32 MAX is reached
                        counter = counter.checked_add(self.increment).unwrap_or(0);
                        let image = render_text(
                            &streamdeck,
                            &self.title,
                            &counter.to_string(),
                            self.title_size,
                            self.number_size,
                        );
                        streamdeck.write_img(image).await.unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
}

fn render_text(
    streamdeck: &DeviceAccess,
    title: &String,
    counter: &String,
    title_size: f32,
    number_size: f32,
) -> DynamicImage {
    let res = streamdeck.resolution();
    let mut image = RgbImage::new(res.0 as u32, res.1 as u32);

    let scale = Scale::uniform(title_size);
    let font = &GLOBAL_FONT.get().unwrap();
    let v_metrics = font.v_metrics(scale);
    let height = (v_metrics.ascent - v_metrics.descent + v_metrics.line_gap).round() as i32;

    let text = wrap_text(image.width(), scale, &title);

    // start at y = 0
    let mut y_pos = 0;

    for line in text.split("\n") {
        draw_text_mut(
            &mut image,
            Rgb([255, 255, 255]),
            0,
            y_pos,
            Scale::uniform(title_size),
            &GLOBAL_FONT.get().unwrap(),
            &line,
        );
        y_pos += height;
    }

    draw_text_mut(
        &mut image,
        Rgb([255, 255, 255]),
        0,
        y_pos,
        Scale::uniform(number_size),
        &GLOBAL_FONT.get().unwrap(),
        &counter,
    );

    image::DynamicImage::ImageRgb8(image)
}
