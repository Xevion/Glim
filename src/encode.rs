//! Image encoding support for different formats.
//!
//! This module provides encoders for PNG, WebP, JPEG, and SVG formats
//! with consistent error handling and result types.

use crate::errors::{GlimError, ImageError, Result};
use image::{Rgba, RgbaImage};
use std::io::Write;
use std::time::Duration;
use tracing::instrument;

/// Timing information for encoding operations
#[derive(Debug, Clone)]
pub struct EncodingTiming {
    pub rasterization: Duration,
    pub encoding: Duration,
    pub total: Duration,
}

impl EncodingTiming {
    pub fn new() -> Self {
        Self {
            rasterization: Duration::ZERO,
            encoding: Duration::ZERO,
            total: Duration::ZERO,
        }
    }
}

/// Helper function to rasterize SVG and convert to RgbaImage.
/// This eliminates code duplication across encoders.
fn rasterize_svg_to_rgba(
    rasterizer: &crate::image::Rasterizer,
    svg_data: &str,
    scale: Option<f64>,
) -> Result<RgbaImage> {
    let pixmap = rasterizer.render_with_scale(svg_data, scale)?;

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
    /// * `scale` - Optional scale factor for the image
    ///
    /// # Returns
    /// Result with timing information indicating success or failure
    fn encode(
        &self,
        svg_data: &str,
        writer: &mut dyn Write,
        scale: Option<f64>,
    ) -> Result<EncodingTiming>;
}

/// PNG encoder using the resvg library.
#[derive(Debug, Default)]
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
    #[instrument(skip(self, writer, svg_data))]
    fn encode(
        &self,
        svg_data: &str,
        writer: &mut dyn Write,
        scale: Option<f64>,
    ) -> Result<EncodingTiming> {
        // Rasterization timing
        let rasterize_start = std::time::Instant::now();
        let pixmap = self.rasterizer.render_with_scale(svg_data, scale)?;
        let rasterize_duration = rasterize_start.elapsed();

        // PNG encoding timing
        let encode_start = std::time::Instant::now();
        let mut png_encoder = png::Encoder::new(writer, pixmap.width(), pixmap.height());
        png_encoder.set_color(png::ColorType::Rgba);
        png_encoder.set_depth(png::BitDepth::Eight);

        let mut png_writer = png_encoder
            .write_header()
            .map_err(|e| GlimError::Image(ImageError::PngWrite(e.to_string())))?;

        png_writer
            .write_image_data(pixmap.data())
            .map_err(|e| GlimError::Image(ImageError::PngWrite(e.to_string())))?;

        png_writer
            .finish()
            .map_err(|e| GlimError::Image(ImageError::PngWrite(e.to_string())))?;
        let encode_duration = encode_start.elapsed();

        let total_duration = rasterize_duration + encode_duration;

        tracing::debug!(
            scale = ?scale,
            width = pixmap.width(),
            height = pixmap.height(),
            rasterization_duration = ?rasterize_duration,
            encoding_duration = ?encode_duration,
            total_duration = ?total_duration,
            "PNG encoding completed"
        );

        if total_duration.as_millis() > 1000 {
            tracing::warn!(
                scale = ?scale,
                width = pixmap.width(),
                height = pixmap.height(),
                rasterization_duration = ?rasterize_duration,
                encoding_duration = ?encode_duration,
                total_duration = ?total_duration,
                "Slow PNG encoding"
            );
        }

        Ok(EncodingTiming {
            rasterization: rasterize_duration,
            encoding: encode_duration,
            total: total_duration,
        })
    }
}

/// WebP encoder using the image crate.
#[derive(Debug, Default)]
pub struct WebPEncoder;

impl WebPEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for WebPEncoder {
    #[instrument(skip(writer, svg_data))]
    fn encode(
        &self,
        svg_data: &str,
        writer: &mut dyn Write,
        scale: Option<f64>,
    ) -> Result<EncodingTiming> {
        let rasterize_start = std::time::Instant::now();
        let img = rasterize_svg_to_rgba(&crate::image::Rasterizer::new(), svg_data, scale)?;
        let rasterize_duration = rasterize_start.elapsed();

        let encode_start = std::time::Instant::now();
        // Encode as WebP
        img.write_with_encoder(image::codecs::webp::WebPEncoder::new_lossless(writer))
            .map_err(|e| GlimError::Image(ImageError::WebPWrite(e.to_string())))?;
        let encode_duration = encode_start.elapsed();

        Ok(EncodingTiming {
            rasterization: rasterize_duration,
            encoding: encode_duration,
            total: rasterize_duration + encode_duration,
        })
    }
}

/// JPEG encoder using the image crate.
#[derive(Debug, Default)]
pub struct JpegEncoder;

impl JpegEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for JpegEncoder {
    #[instrument(skip(writer, svg_data))]
    fn encode(
        &self,
        svg_data: &str,
        writer: &mut dyn Write,
        scale: Option<f64>,
    ) -> Result<EncodingTiming> {
        let rasterize_start = std::time::Instant::now();
        let img = rasterize_svg_to_rgba(&crate::image::Rasterizer::new(), svg_data, scale)?;
        let rasterize_duration = rasterize_start.elapsed();

        let encode_start = std::time::Instant::now();
        // Convert RGBA to RGB for JPEG encoding
        let rgb_img = image::DynamicImage::ImageRgba8(img).into_rgb8();

        // Encode as JPEG
        rgb_img
            .write_with_encoder(image::codecs::jpeg::JpegEncoder::new(writer))
            .map_err(|e| GlimError::Image(ImageError::JpegWrite(e.to_string())))?;
        let encode_duration = encode_start.elapsed();

        Ok(EncodingTiming {
            rasterization: rasterize_duration,
            encoding: encode_duration,
            total: rasterize_duration + encode_duration,
        })
    }
}

/// SVG encoder that just returns the SVG data as-is.
#[derive(Debug, Default)]
pub struct SvgEncoder;

impl SvgEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for SvgEncoder {
    #[instrument(skip(writer, svg_data))]
    fn encode(
        &self,
        svg_data: &str,
        writer: &mut dyn Write,
        _scale: Option<f64>,
    ) -> Result<EncodingTiming> {
        let encode_start = std::time::Instant::now();
        writer
            .write_all(svg_data.as_bytes())
            .map_err(|e| GlimError::Image(ImageError::SvgWrite(e.to_string())))?;
        let encode_duration = encode_start.elapsed();

        Ok(EncodingTiming {
            rasterization: Duration::ZERO,
            encoding: encode_duration,
            total: encode_duration,
        })
    }
}

/// AVIF encoder using the image crate.
#[derive(Debug, Default)]
pub struct AvifEncoder;

impl AvifEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for AvifEncoder {
    #[instrument(skip(writer, svg_data))]
    fn encode(
        &self,
        svg_data: &str,
        writer: &mut dyn Write,
        scale: Option<f64>,
    ) -> Result<EncodingTiming> {
        let rasterize_start = std::time::Instant::now();
        let img = rasterize_svg_to_rgba(&crate::image::Rasterizer::new(), svg_data, scale)?;
        let rasterize_duration = rasterize_start.elapsed();

        let encode_start = std::time::Instant::now();
        // Encode as AVIF with maximum speed settings (speed 10, quality 60)
        img.write_with_encoder(image::codecs::avif::AvifEncoder::new_with_speed_quality(
            writer, 10, 60,
        ))
        .map_err(|e| GlimError::Image(ImageError::AvifWrite(e.to_string())))?;
        let encode_duration = encode_start.elapsed();

        Ok(EncodingTiming {
            rasterization: rasterize_duration,
            encoding: encode_duration,
            total: rasterize_duration + encode_duration,
        })
    }
}

/// GIF encoder using the image crate.
/// Note: GIF encoding is not currently supported in the image crate.
#[derive(Debug, Default)]
pub struct GifEncoder;

impl GifEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for GifEncoder {
    #[instrument(skip(_svg_data, _writer))]
    fn encode(
        &self,
        _svg_data: &str,
        _writer: &mut dyn Write,
        _scale: Option<f64>,
    ) -> Result<EncodingTiming> {
        // GIF encoding is not currently supported
        Err(GlimError::Image(ImageError::GifWrite(
            "GIF encoding is not implemented".to_string(),
        )))
    }
}

/// ICO encoder using the image crate.
#[derive(Debug, Default)]
pub struct IcoEncoder;

impl IcoEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl Encoder for IcoEncoder {
    #[instrument(skip(writer, svg_data))]
    fn encode(
        &self,
        svg_data: &str,
        writer: &mut dyn Write,
        scale: Option<f64>,
    ) -> Result<EncodingTiming> {
        let rasterize_start = std::time::Instant::now();
        let img = rasterize_svg_to_rgba(&crate::image::Rasterizer::new(), svg_data, scale)?;
        let rasterize_duration = rasterize_start.elapsed();

        let encode_start = std::time::Instant::now();
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
            .map_err(|e| GlimError::Image(ImageError::IcoWrite(e.to_string())))?;
        let encode_duration = encode_start.elapsed();

        Ok(EncodingTiming {
            rasterization: rasterize_duration,
            encoding: encode_duration,
            total: rasterize_duration + encode_duration,
        })
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
    fn encode(
        &self,
        svg_data: &str,
        writer: &mut dyn Write,
        scale: Option<f64>,
    ) -> Result<EncodingTiming> {
        match self {
            EncoderType::Png(encoder) => encoder.encode(svg_data, writer, scale),
            EncoderType::WebP(encoder) => encoder.encode(svg_data, writer, scale),
            EncoderType::Jpeg(encoder) => encoder.encode(svg_data, writer, scale),
            EncoderType::Svg(encoder) => encoder.encode(svg_data, writer, scale),
            EncoderType::Avif(encoder) => encoder.encode(svg_data, writer, scale),
            EncoderType::Gif(encoder) => encoder.encode(svg_data, writer, scale),
            EncoderType::Ico(encoder) => encoder.encode(svg_data, writer, scale),
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
