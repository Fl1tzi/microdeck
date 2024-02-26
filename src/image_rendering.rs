// multiple functions for rendering various images/buttons
//
use crate::GLOBAL_FONT;
use image::imageops;
use image::{io::Reader, DynamicImage, ImageBuffer, Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;
use rusttype::Scale;
use std::{io, path::Path};
use tracing::trace;

/// Retrieve an image from a path
#[allow(dead_code)]
pub fn retrieve_image(path: &Path) -> io::Result<DynamicImage> {
    trace!("Retrieving image from filesystem");
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
    text_color: [u8; 3],
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
            font_size: 16.0,
            text: None,
            // black
            text_color: [255, 255, 255],
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
    pub fn set_text_color(mut self, text_color: [u8; 3]) -> Self {
        self.text_color = text_color;
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
                text_color: self.text_color,
                text: self.text.unwrap(),
            };
            return c.render();
        } else if let Some(text) = self.text {
            let c = TextComponent {
                height: self.height,
                width: self.width,
                font_size: self.font_size,
                text_color: self.text_color,
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

/// Component that just displays an image
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

/// Component that just displays text
struct TextComponent {
    height: usize,
    width: usize,
    font_size: f32,
    text_color: [u8; 3],
    text: String,
}

impl Component for TextComponent {
    fn render(&self) -> DynamicImage {
        let mut image = RgbImage::new(self.width as u32, self.height as u32);

        let font_scale = Scale::uniform(self.font_size);
        let text = wrap_text(self.height as u32, font_scale, &self.text);

        draw_text_on_image(&text, &mut image, Rgb(self.text_color), font_scale);

        image::DynamicImage::ImageRgb8(image)
    }
}

/// Component that displays image and text
struct ImageTextComponent {
    height: usize,
    width: usize,
    image: DynamicImage,
    scale: f32,
    font_size: f32,
    text_color: [u8; 3],
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

        let font_scale = Scale::uniform(self.font_size);
        let text = wrap_text(self.height as u32, font_scale, &self.text);
        draw_text_on_image(&text, &mut base_image, Rgb(self.text_color), font_scale);
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

fn draw_text_on_image(text: &String, image: &mut RgbImage, color: Rgb<u8>, font_scale: Scale) {
    let font = &GLOBAL_FONT.get().unwrap();
    let v_metrics = font.v_metrics(font_scale);

    let line_height = (v_metrics.ascent - v_metrics.descent + v_metrics.line_gap).round() as i32;
    let mut y_pos = 0;

    for line in text.split('\n') {
        draw_text_mut(image, color, 0, y_pos, font_scale, font, &line);
        y_pos += line_height
    }
}

/// This functions adds '\n' to the line endings. It does not wrap
/// words but characters.
pub fn wrap_text(max_width: u32, font_size: Scale, text: &String) -> String {
    let font = &GLOBAL_FONT.get().unwrap();

    let mut new_text: Vec<char> = Vec::new();
    let mut line_size = 0.0;

    for character in text.chars() {
        let h_size = font.glyph(character).scaled(font_size).h_metrics();
        let complete_width = h_size.advance_width + h_size.left_side_bearing;
        if (line_size + complete_width) as u32 > max_width {
            new_text.push('\n');
            line_size = 0.0;
        }
        new_text.push(character);
        line_size += h_size.advance_width + h_size.left_side_bearing;
    }
    String::from_iter(new_text)
}
