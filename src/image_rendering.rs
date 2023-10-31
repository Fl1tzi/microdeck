// multiple functions for rendering various images/buttons
//
use crate::GLOBAL_FONT;
use image::imageops;
use image::{io::Reader, DynamicImage, ImageBuffer, Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;
use rusttype::Scale;
use std::io;

/// Loads an image from a path
#[allow(dead_code)]
pub fn load_image(path: String) -> io::Result<DynamicImage> {
    Ok(Reader::open(path)?
        .decode()
        .expect("Unable to decode image"))
}

/// A red image which should represent an missing image or error
#[allow(dead_code)]
pub fn create_error_image() -> DynamicImage {
    let mut error_img: RgbImage = ImageBuffer::new(1, 1);

    for pixel in error_img.enumerate_pixels_mut() {
        *pixel.2 = image::Rgb([240, 128, 128]);
    }

    DynamicImage::ImageRgb8(error_img)
}

trait Component {
    fn render(&self) -> DynamicImage;
}

/// The ImageBuilder is an easy way to build images.
///
/// # Example
///
/// ```rust
/// ImageBuilder::new(20, 20)
///     .set_text("This is a test")
///     .build();
/// ```
pub struct ImageBuilder {
    height: usize,
    width: usize,
    scale: f32,
    font_size: f32,
    text: Option<String>,
    image: Option<DynamicImage>,
}

impl Default for ImageBuilder {
    fn default() -> ImageBuilder {
        ImageBuilder {
            // will get changed
            height: 0,
            // will get changed
            width: 0,
            scale: 60.0,
            font_size: 15.0,
            text: None,
            image: None,
        }
    }
}

impl ImageBuilder {
    #[allow(dead_code)]
    pub fn new(height: usize, width: usize) -> Self {
        ImageBuilder {
            height,
            width,
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub fn set_image_scale(mut self, scale: f32) -> Self {
        self.scale = scale;
        self
    }

    #[allow(dead_code)]
    pub fn set_text(mut self, text: String) -> Self {
        self.text = Some(text);
        self
    }

    #[allow(dead_code)]
    pub fn set_font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    #[allow(dead_code)]
    pub fn set_image(mut self, image: DynamicImage) -> Self {
        self.image = Some(image);
        self
    }

    #[allow(dead_code)]
    pub fn build(self) -> DynamicImage {
        // cannot use "if let" here, because variables would be moved
        if self.text.is_some() && self.image.is_some() {
            let c = ImageTextComponent {
                height: self.height,
                width: self.width,
                image: self.image.unwrap(),
                scale: self.scale,
                font_size: self.font_size,
                text: self.text.unwrap(),
            };
            return c.render();
        } else if let Some(text) = self.text {
            let c = TextComponent {
                height: self.height,
                width: self.width,
                font_size: self.font_size,
                text,
            };
            return c.render();
        } else if let Some(image) = self.image {
            let c = ImageComponent {
                height: self.height,
                width: self.width,
                scale: self.scale,
                image,
            };
            return c.render();
        } else {
            return create_error_image();
        }
    }
}

// Component that just displays an image
struct ImageComponent {
    height: usize,
    width: usize,
    scale: f32,
    image: DynamicImage,
}

impl Component for ImageComponent {
    fn render(&self) -> DynamicImage {
        let new_h = (self.height as f32 * (self.scale * 0.01)) as u32;
        let new_w = (self.width as f32 * (self.scale * 0.01)) as u32;

        let image = self
            .image
            .resize_to_fill(new_w, new_h, image::imageops::FilterType::Nearest);

        let mut base_image = RgbImage::new(self.height as u32, self.width as u32);

        let free_x = self.width - image.width() as usize;
        let free_y = self.height - image.height() as usize;
        imageops::overlay(
            &mut base_image,
            &image.to_rgb8(),
            (free_x / 2) as i64,
            (free_y / 2) as i64,
        );

        image::DynamicImage::ImageRgb8(base_image)
    }
}

// Component that just displays text
struct TextComponent {
    height: usize,
    width: usize,
    font_size: f32,
    text: String,
}

impl Component for TextComponent {
    fn render(&self) -> DynamicImage {
        let mut image = RgbImage::new(self.width as u32, self.height as u32);

        let scale = Scale::uniform(self.font_size);
        let font = &GLOBAL_FONT.get().unwrap();

        let v_metrics = font.v_metrics(scale);
        let height = (v_metrics.ascent - v_metrics.descent + v_metrics.line_gap).round() as i32;

        // start at y = 10
        let mut y_pos = 10;

        for line in self.text.split("\n") {
            draw_text_mut(
                &mut image,
                Rgb([255, 255, 255]),
                10,
                y_pos,
                scale,
                &GLOBAL_FONT.get().unwrap(),
                &line,
            );
            y_pos += height;
        }

        image::DynamicImage::ImageRgb8(image)
    }
}

// Component that displays image and text
struct ImageTextComponent {
    height: usize,
    width: usize,
    image: DynamicImage,
    scale: f32,
    font_size: f32,
    text: String,
}

impl Component for ImageTextComponent {
    fn render(&self) -> DynamicImage {
        let new_h = (self.height as f32 * (self.scale * 0.01)) as u32;
        let new_w = (self.width as f32 * (self.scale * 0.01)) as u32;

        let image = self
            .image
            .resize_to_fill(new_w, new_h, image::imageops::FilterType::Nearest);

        let mut base_image = RgbImage::new(self.height as u32, self.width as u32);

        let font = &GLOBAL_FONT.get().unwrap();
        let font_scale = Scale::uniform(self.font_size);

        // TODO: allow new line
        draw_text_mut(
            &mut base_image,
            Rgb([255, 255, 255]),
            0,
            0,
            font_scale,
            font,
            &self.text,
        );
        // position at the middle
        let free_space = self.width - image.width() as usize;
        // TODO: allow padding to be manually set
        imageops::overlay(
            &mut base_image,
            &image.to_rgb8(),
            (free_space / 2) as i64,
            self.height as i64 - image.height() as i64 - 5,
        );

        image::DynamicImage::ImageRgb8(base_image)
    }
}
