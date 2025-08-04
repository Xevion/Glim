//! Image encoding support for different formats.
//!
//! This module provides encoders for PNG, WebP, JPEG, and SVG formats
//! with consistent error handling and result types.

use crate::errors::{ImageError, LivecardsError, Result};
use image::{Rgba, RgbaImage};
use std::io::Write;
use tracing::instrument;

/// Helper function to rasterize SVG and convert to RgbaImage.
/// This eliminates code duplication across encoders.
fn rasterize_svg_to_rgba(svg_data: &str) -> Result<RgbaImage> {
    let rasterizer = crate::image::Rasterizer::new();
    let pixmap = rasterizer.render(svg_data)?;

    let width = pixmap.width();
    let height = pixmap.height();
    let mut img = RgbaImage::new(width, height);

    // Copy pixel data from pixmap to image buffer
    let data = pixmap.data();
    for (i, pixel) in img.pixels_mut().enumerate() {
        let offset = i * 4;
        if offset + 3 < data.len() {
            *pixel = Rgba([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
        }
    }

    Ok(img)
}

/// Supported image formats for encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    WebP,
    Jpeg,
    Svg,
    Avif,
    Gif,
    Ico,
}

impl ImageFormat {
    /// Get the MIME type for this format.
    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::WebP => "image/webp",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Svg => "image/svg+xml",
            ImageFormat::Avif => "image/avif",
            ImageFormat::Gif => "image/gif",
            ImageFormat::Ico => "image/x-icon",
        }
    }

    /// Get the file extension for this format.
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::WebP => "webp",
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Svg => "svg",
            ImageFormat::Avif => "avif",
            ImageFormat::Gif => "gif",
            ImageFormat::Ico => "ico",
        }
    }
}

/// Encoder trait for different image formats.
pub trait Encoder {
    /// Encode the given SVG data to the target format.
    ///
    /// # Arguments
    /// * `svg_data` - The SVG data to encode
    /// * `writer` - Output writer for the encoded data
    ///
    /// # Returns
    /// Result indicating success or failure
    fn encode(&self, svg_data: &str, writer: &mut dyn Write) -> Result<()>;
}

/// PNG encoder using the resvg library.
#[derive(Debug)]
pub struct PngEncoder {
    rasterizer: crate::image::Rasterizer,
}

impl PngEncoder {
    pub fn new() -> Self {
        Self {
            rasterizer: crate::image::Rasterizer::new(),
        }
    }
}

impl Encoder for PngEncoder {
    #[instrument(skip(self, writer))]
    fn encode(&self, svg_data: &str, writer: &mut dyn Write) -> Result<()> {
        let pixmap = self.rasterizer.render(svg_data)?;

        let mut png_encoder = png::Encoder::new(writer, pixmap.width(), pixmap.height());
        png_encoder.set_color(png::ColorType::Rgba);
        png_encoder.set_depth(png::BitDepth::Eight);

        let mut png_writer = png_encoder
            .write_header()
            .map_err(|e| LivecardsError::Image(ImageError::PngWrite(e.to_string())))?;

        png_writer
            .write_image_data(pixmap.data())
            .map_err(|e| LivecardsError::Image(ImageError::PngWrite(e.to_string())))?;

        png_writer
            .finish()
            .map_err(|e| LivecardsError::Image(ImageError::PngWrite(e.to_string())))?;

        Ok(())
    }
}

impl Default for PngEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// WebP encoder using the image crate.
#[derive(Debug)]
pub struct WebPEncoder;

impl WebPEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for WebPEncoder {
    #[instrument(skip(writer))]
    fn encode(&self, svg_data: &str, writer: &mut dyn Write) -> Result<()> {
        let img = rasterize_svg_to_rgba(svg_data)?;

        // Encode as WebP
        img.write_with_encoder(image::codecs::webp::WebPEncoder::new_lossless(writer))
            .map_err(|e| LivecardsError::Image(ImageError::WebPWrite(e.to_string())))?;

        Ok(())
    }
}

impl Default for WebPEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// JPEG encoder using the image crate.
#[derive(Debug)]
pub struct JpegEncoder;

impl JpegEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for JpegEncoder {
    #[instrument(skip(writer))]
    fn encode(&self, svg_data: &str, writer: &mut dyn Write) -> Result<()> {
        let img = rasterize_svg_to_rgba(svg_data)?;

        // Convert RGBA to RGB for JPEG encoding
        let rgb_img = image::DynamicImage::ImageRgba8(img).into_rgb8();

        // Encode as JPEG
        rgb_img
            .write_with_encoder(image::codecs::jpeg::JpegEncoder::new(writer))
            .map_err(|e| LivecardsError::Image(ImageError::JpegWrite(e.to_string())))?;

        Ok(())
    }
}

impl Default for JpegEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// SVG encoder that just returns the SVG data as-is.
#[derive(Debug)]
pub struct SvgEncoder;

impl SvgEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for SvgEncoder {
    #[instrument(skip(writer))]
    fn encode(&self, svg_data: &str, writer: &mut dyn Write) -> Result<()> {
        writer
            .write_all(svg_data.as_bytes())
            .map_err(|e| LivecardsError::Image(ImageError::SvgWrite(e.to_string())))?;
        Ok(())
    }
}

impl Default for SvgEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// AVIF encoder using the image crate.
#[derive(Debug)]
pub struct AvifEncoder;

impl AvifEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for AvifEncoder {
    #[instrument(skip(writer))]
    fn encode(&self, svg_data: &str, writer: &mut dyn Write) -> Result<()> {
        let img = rasterize_svg_to_rgba(svg_data)?;

        // Encode as AVIF with maximum speed settings (speed 10, quality 60)
        img.write_with_encoder(image::codecs::avif::AvifEncoder::new_with_speed_quality(
            writer, 10, 60,
        ))
        .map_err(|e| LivecardsError::Image(ImageError::AvifWrite(e.to_string())))?;

        Ok(())
    }
}

impl Default for AvifEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// GIF encoder using the image crate.
/// Note: GIF encoding is not currently supported in the image crate.
#[derive(Debug)]
pub struct GifEncoder;

impl GifEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for GifEncoder {
    #[instrument(skip(_svg_data, _writer))]
    fn encode(&self, _svg_data: &str, _writer: &mut dyn Write) -> Result<()> {
        // GIF encoding is not currently supported
        Err(LivecardsError::Image(ImageError::GifWrite(
            "GIF encoding not supported".to_string(),
        )))
    }
}

impl Default for GifEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// ICO encoder using the image crate.
#[derive(Debug)]
pub struct IcoEncoder;

impl IcoEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for IcoEncoder {
    #[instrument(skip(writer))]
    fn encode(&self, svg_data: &str, writer: &mut dyn Write) -> Result<()> {
        let img = rasterize_svg_to_rgba(svg_data)?;

        // Resize image to fit ICO requirements (max 256x256)
        let width = img.width();
        let height = img.height();
        let max_size = 256;
        let resized_img = if width > max_size || height > max_size {
            let scale = max_size as f32 / width.max(height) as f32;
            let new_width = (width as f32 * scale) as u32;
            let new_height = (height as f32 * scale) as u32;
            image::DynamicImage::ImageRgba8(img).resize(
                new_width,
                new_height,
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            image::DynamicImage::ImageRgba8(img)
        };

        // Encode as ICO
        resized_img
            .write_with_encoder(image::codecs::ico::IcoEncoder::new(writer))
            .map_err(|e| LivecardsError::Image(ImageError::IcoWrite(e.to_string())))?;

        Ok(())
    }
}

impl Default for IcoEncoder {
    fn default() -> Self {
        Self::new()
    }
}

/// Enum to hold different encoder types.
#[derive(Debug)]
pub enum EncoderType {
    Png(PngEncoder),
    WebP(WebPEncoder),
    Jpeg(JpegEncoder),
    Svg(SvgEncoder),
    Avif(AvifEncoder),
    Gif(GifEncoder),
    Ico(IcoEncoder),
}

impl Encoder for EncoderType {
    fn encode(&self, svg_data: &str, writer: &mut dyn Write) -> Result<()> {
        match self {
            EncoderType::Png(encoder) => encoder.encode(svg_data, writer),
            EncoderType::WebP(encoder) => encoder.encode(svg_data, writer),
            EncoderType::Jpeg(encoder) => encoder.encode(svg_data, writer),
            EncoderType::Svg(encoder) => encoder.encode(svg_data, writer),
            EncoderType::Avif(encoder) => encoder.encode(svg_data, writer),
            EncoderType::Gif(encoder) => encoder.encode(svg_data, writer),
            EncoderType::Ico(encoder) => encoder.encode(svg_data, writer),
        }
    }
}

/// Factory function to create an encoder for the specified format.
pub fn create_encoder(format: ImageFormat) -> EncoderType {
    match format {
        ImageFormat::Png => EncoderType::Png(PngEncoder::new()),
        ImageFormat::WebP => EncoderType::WebP(WebPEncoder::new()),
        ImageFormat::Jpeg => EncoderType::Jpeg(JpegEncoder::new()),
        ImageFormat::Svg => EncoderType::Svg(SvgEncoder::new()),
        ImageFormat::Avif => EncoderType::Avif(AvifEncoder::new()),
        ImageFormat::Gif => EncoderType::Gif(GifEncoder::new()),
        ImageFormat::Ico => EncoderType::Ico(IcoEncoder::new()),
    }
}

/// Generate an image in the specified format.
///
/// # Arguments
/// * `name` - Repository name
/// * `description` - Repository description
/// * `language` - Primary programming language
/// * `stars` - Star count as string
/// * `forks` - Fork count as string
/// * `format` - Target image format
/// * `writer` - Output writer for the encoded data
///
/// # Returns
/// Result indicating success or failure
#[instrument(skip(writer))]
pub fn generate_image<W: Write>(
    name: &str,
    description: &str,
    language: &str,
    stars: &str,
    forks: &str,
    format: ImageFormat,
    mut writer: W,
) -> Result<()> {
    let svg_template = include_str!("../card.svg");
    let wrapped_description = crate::image::wrap_text(description, 65);
    let language_color =
        crate::colors::get_color(language).unwrap_or_else(|| "#f1e05a".to_string());

    let formatted_stars = crate::image::format_count(stars);
    let formatted_forks = crate::image::format_count(forks);

    let svg_filled = svg_template
        .replace("{{name}}", name)
        .replace("{{description}}", &wrapped_description)
        .replace("{{language}}", language)
        .replace("{{language_color}}", &language_color)
        .replace("{{stars}}", &formatted_stars)
        .replace("{{forks}}", &formatted_forks);

    let encoder = create_encoder(format);
    encoder.encode(&svg_filled, &mut writer)
}
