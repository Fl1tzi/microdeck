use std::sync::Arc;

use crate::config::Button;

use super::Module;
use super::{ChannelReceiver, DeviceAccess, HostEvent, ReturnError};

use async_trait::async_trait;
use image::{Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};

/// A module which displays a counter
pub struct Counter;

#[async_trait]
impl Module for Counter {
    async fn run(
        streamdeck: DeviceAccess,
        button_receiver: ChannelReceiver,
        _config: Arc<Button>,
    ) -> Result<(), ReturnError> {
        let mut button_receiver = button_receiver;

        let font_data: &[u8] = include_bytes!("../../fonts/SpaceGrotesk.ttf");
        let font: Font<'static> = Font::try_from_bytes(font_data).unwrap();

        let (h, w) = streamdeck.resolution();

        let mut counter: u32 = 0;
        loop {
            if let Some(event) = button_receiver.recv().await {
                match event {
                    HostEvent::ButtonPressed => {
                        counter += 1;
                        let mut image = RgbImage::new(h as u32, w as u32);
                        draw_text_mut(
                            &mut image,
                            Rgb([255, 255, 255]),
                            10,
                            10,
                            Scale::uniform(20.0),
                            &font,
                            format!("{}", counter).as_str(),
                        );
                        streamdeck
                            .write_img(image::DynamicImage::ImageRgb8(image))
                            .await
                            .unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
}
