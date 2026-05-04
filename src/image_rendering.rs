// multiple functions for rendering various images/buttons
//
use crate::GLOBAL_FONT;
use async_trait::async_trait;
use image::imageops;
use image::{io::Reader, DynamicImage, ImageBuffer, Rgb, RgbImage};
use imageproc::drawing::draw_text_mut;
use rusttype::Scale;
use std::{
    io,
    path::{Path, PathBuf},
};
use tracing::trace;

/// Retrieve an image from a path
#[allow(dead_code)]
pub fn retrieve_image(path: &Path) -> io::Result<DynamicImage> {
    trace!("Retrieving image from filesystem");
    Ok(Reader::open(path)?
        .decode()
        .expect("Unable to decode image"))
}

pub mod cache {
    use super::retrieve_image;
    use base64::engine::{general_purpose, Engine};
    use dirs::cache_dir;
    use image::imageops::FilterType;
    use image::io::Reader as ImageReader;
    use image::DynamicImage;
    use ring::digest;
    use std::path::PathBuf;
    use tracing::trace;

    /// Loads an image from the system or retrieves it from the cache. If
    /// the provided image is not already in the cache it will be inserted.
    #[allow(dead_code)]
    pub async fn load_image(path: PathBuf, resolution: (usize, usize)) -> Option<DynamicImage> {
        // hash the image
        let mut image = tokio::task::spawn_blocking(move || retrieve_image(&path))
            .await
            .unwrap()
            .ok()?;

        let image_hash = hash_image(image.as_bytes());

        if let Some(image) = get_image_from_cache(&image_hash, resolution) {
            trace!("Cached image is available");
            return Some(image);
        }

        // TODO prevent multiple buttons from resizing the same image at the same time (performance
        // improvement)
        let image = tokio::task::spawn_blocking(move || {
            trace!("Resizing image");
            image = image.resize_exact(
                resolution.0 as u32,
                resolution.1 as u32,
                FilterType::Lanczos3,
            );
            trace!("Resizing finished");
            let mut path = match cache_dir() {
                Some(dir) => dir,
                None => return None, // System does not provide cache
            };
            path.push("microdeck");
            path.push(image_cache_file_name(&image_hash, resolution));

            image.save(path).ok()?;
            Some(image)
        })
        .await
        .unwrap()?;
        Some(image.into())
    }

    /// Does the same thing as [load_image] but the images aspect ratio is preserved to fit in the specified resolution. Also see [image::DynamicImage::resize_to_fill]
    // TODO: Duplicated code from load_image
    pub async fn load_image_fill(
        path: PathBuf,
        resolution: (usize, usize),
    ) -> Option<DynamicImage> {
        // hash the image
        let mut image = tokio::task::spawn_blocking(move || retrieve_image(&path))
            .await
            .unwrap()
            .ok()?;

        let image_hash = hash_image(image.as_bytes());

        if let Some(image) = get_image_from_cache(&image_hash, resolution) {
            trace!("Cached image is available");
            return Some(image);
        }

        // TODO prevent multiple buttons from resizing the same image at the same time (performance
        // improvement)
        let image = tokio::task::spawn_blocking(move || {
            trace!("Resizing image");
            image = image.resize_to_fill(
                resolution.0 as u32,
                resolution.1 as u32,
                FilterType::Lanczos3,
            );
            trace!("Resizing finished");
            let mut path = match cache_dir() {
                Some(dir) => dir,
                None => return None, // System does not provide cache
            };
            path.push("microdeck");
            path.push(image_cache_file_name(&image_hash, resolution));

            image.save(path).ok()?;
            Some(image)
        })
        .await
        .unwrap()?;
        Some(image.into())
    }

    /// File name for a cached image
    ///
    /// `<hash>-<height>x<width>`
    pub fn image_cache_file_name(image_hash: &str, resolution: (usize, usize)) -> String {
        format!("{}-{}x{}.png", image_hash, resolution.0, resolution.1)
    }

    pub fn hash_image(data: &[u8]) -> String {
        let mut context = digest::Context::new(&digest::SHA256);
        context.update(data);
        let hash = context.finish();
        general_purpose::STANDARD.encode(hash)
    }

    /// Try to retrieve an image from the cache. Will return None if
    /// the image was not cached yet (or is not accessible)
    /// or if the system does not provide a [dirs::cache_dir].
    #[allow(dead_code)]
    pub fn get_image_from_cache(
        image_hash: &str,
        resolution: (usize, usize),
    ) -> Option<DynamicImage> {
        let mut path = match cache_dir() {
            Some(dir) => dir,
            None => return None, // System does not provide cache
        };

        path.push("microdeck");
        path.push(image_cache_file_name(image_hash, resolution));

        Some(ImageReader::open(path).ok()?.decode().ok()?)
    }
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

#[async_trait]
trait Component {
    async fn render(self) -> DynamicImage;
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
    image: Option<PathBuf>,
    folder_icon: bool,
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
            folder_icon: false,
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
    pub fn set_image(mut self, image: PathBuf) -> Self {
        self.image = Some(image);
        self
    }

    pub fn set_folder_icon(mut self) -> Self {
        self.folder_icon = true;
        self
    }

    #[allow(dead_code)]
    pub async fn build(self) -> DynamicImage {
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
            return c.render().await;
        } else if let Some(text) = self.text {
            // needs at least text
            if self.folder_icon {
                let c = FolderIconComponent {
                    height: self.height,
                    width: self.width,
                    text: Some(text),
                    font_size: self.font_size,
                    text_color: self.text_color,
                };
                return c.render().await;
            }
            let c = TextComponent {
                height: self.height,
                width: self.width,
                font_size: self.font_size,
                text_color: self.text_color,
                text,
            };
            return c.render().await;
        } else if let Some(image) = self.image {
            let c = ImageComponent {
                height: self.height,
                width: self.width,
                scale: self.scale,
                image,
            };
            return c.render().await;
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
    image: PathBuf,
}

#[async_trait]
impl Component for ImageComponent {
    async fn render(self) -> DynamicImage {
        let new_h = (self.height as f32 * (self.scale * 0.01)) as u32;
        let new_w = (self.width as f32 * (self.scale * 0.01)) as u32;

        let image = cache::load_image_fill(self.image, (new_h as usize, new_w as usize))
            .await
            .expect("Image not available");

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

#[async_trait]
impl Component for TextComponent {
    async fn render(self) -> DynamicImage {
        let image = RgbImage::new(self.width as u32, self.height as u32);

        let font_scale = Scale::uniform(self.font_size);
        let text = wrap_text(self.height as u32, font_scale, &self.text);
        let text_color = Rgb(self.text_color);

        let image = tokio::task::spawn_blocking(move || {
            draw_text_on_image(text, image, text_color, font_scale)
        })
        .await
        .unwrap();

        image::DynamicImage::ImageRgb8(image)
    }
}

/// Component that displays image and text
struct ImageTextComponent {
    height: usize,
    width: usize,
    image: PathBuf,
    scale: f32,
    font_size: f32,
    text_color: [u8; 3],
    text: String,
}

#[async_trait]
impl Component for ImageTextComponent {
    async fn render(self) -> DynamicImage {
        let new_h = (self.height as f32 * (self.scale * 0.01)) as u32;
        let new_w = (self.width as f32 * (self.scale * 0.01)) as u32;

        let image = cache::load_image_fill(self.image, (new_h as usize, new_w as usize))
            .await
            .expect("Image not available");

        let base_image = RgbImage::new(self.height as u32, self.width as u32);

        let font_scale = Scale::uniform(self.font_size);
        let text = wrap_text(self.height as u32, font_scale, &self.text);
        let text_color = Rgb(self.text_color);
        let mut base_image = tokio::task::spawn_blocking(move || {
            draw_text_on_image(text, base_image, text_color, font_scale)
        })
        .await
        .unwrap();
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

struct FolderIconComponent {
    height: usize,
    width: usize,
    text: Option<String>,
    font_size: f32,
    text_color: [u8; 3],
}

#[async_trait]
impl Component for FolderIconComponent {
    async fn render(self) -> DynamicImage {
        let w = self.width as u32;
        let h = self.height as u32;

        let mut img = RgbImage::new(w, h);

        let body_color = Rgb([255u8, 176, 28]);
        let tab_color = Rgb([255u8, 198, 70]);
        let shadow_color = Rgb([200u8, 130, 0]);
        let highlight = Rgb([255u8, 220, 120]);

        let font_scale = Scale::uniform(self.font_size);

        // Reserve space at the bottom for text if needed
        let text_area_h = if self.text.is_some() {
            (self.font_size * 1.4) as u32
        } else {
            0
        };

        let folder_h = h - text_area_h;

        let body_x = (w as f32 * 0.05) as u32;
        let body_y = (folder_h as f32 * 0.40) as u32;
        let body_w = (w as f32 * 0.90) as u32;
        let body_h = (folder_h as f32 * 0.50) as u32;
        let tab_w = (w as f32 * 0.35) as u32;
        let tab_h = (folder_h as f32 * 0.12) as u32;
        let tab_y = body_y - tab_h;
        let radius = (body_h as f32 * 0.10) as u32;

        // Shadow
        draw_rounded_rect(
            &mut img,
            body_x + 2,
            body_y + 4,
            body_w,
            body_h,
            radius,
            shadow_color,
        );
        // Body
        draw_rounded_rect(&mut img, body_x, body_y, body_w, body_h, radius, body_color);
        // Tab
        fill_rect_img(&mut img, body_x, tab_y, tab_w, tab_h + 4, tab_color);
        // Highlight stripe
        fill_rect_img(
            &mut img,
            body_x + radius,
            body_y + 2,
            body_w - radius * 2,
            (body_h as f32 * 0.08) as u32,
            highlight,
        );

        // Draw text below the folder
        if let Some(text) = self.text {
            let text_color = Rgb(self.text_color);
            let wrapped = wrap_text(w, font_scale, &text);
            img = tokio::task::spawn_blocking(move || {
                let font = &GLOBAL_FONT.get().unwrap();
                let v_metrics = font.v_metrics(font_scale);
                let line_height =
                    (v_metrics.ascent - v_metrics.descent + v_metrics.line_gap).round() as i32;
                let mut y_pos = folder_h as i32;
                for line in wrapped.split('\n') {
                    draw_text_mut(&mut img, text_color, 0, y_pos, font_scale, font, line);
                    y_pos += line_height;
                }
                img
            })
            .await
            .unwrap();
        }

        DynamicImage::ImageRgb8(img)
    }
}

fn draw_text_on_image(
    text: String,
    image: RgbImage,
    color: Rgb<u8>,
    font_scale: Scale,
) -> RgbImage {
    let mut image = image;
    let font = &GLOBAL_FONT.get().unwrap();
    let v_metrics = font.v_metrics(font_scale);

    let line_height = (v_metrics.ascent - v_metrics.descent + v_metrics.line_gap).round() as i32;
    let mut y_pos = 0;

    for line in text.split('\n') {
        draw_text_mut(&mut image, color, 0, y_pos, font_scale, font, &line);
        y_pos += line_height
    }
    image
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

fn fill_rect_img(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, color: Rgb<u8>) {
    for py in y..y + h {
        for px in x..x + w {
            if px < img.width() && py < img.height() {
                img.put_pixel(px, py, color);
            }
        }
    }
}

fn draw_rounded_rect(img: &mut RgbImage, x: u32, y: u32, w: u32, h: u32, r: u32, color: Rgb<u8>) {
    fill_rect_img(img, x + r, y, w - 2 * r, h, color);
    fill_rect_img(img, x, y + r, r, h - 2 * r, color);
    fill_rect_img(img, x + w - r, y + r, r, h - 2 * r, color);

    let r = r as i64;
    for dy in 0..r {
        for dx in 0..r {
            if dx * dx + dy * dy <= r * r {
                let (xi, yi, wi, hi) = (x as i64, y as i64, w as i64, h as i64);
                img.put_pixel((xi + r - 1 - dx) as u32, (yi + r - 1 - dy) as u32, color);
                img.put_pixel((xi + wi - r + dx) as u32, (yi + r - 1 - dy) as u32, color);
                img.put_pixel((xi + r - 1 - dx) as u32, (yi + hi - r + dy) as u32, color);
                img.put_pixel((xi + wi - r + dx) as u32, (yi + hi - r + dy) as u32, color);
            }
        }
    }
}
