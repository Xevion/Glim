//! Image generation for repository cards.
//!
//! This module handles SVG template processing and multi-format encoding
//! to create beautiful repository cards with dynamic content.

use crate::errors::{GlimError, ImageError, Result};
use resvg::{tiny_skia, usvg};
use tracing::instrument;

// Re-export ImageFormat for public use
pub use crate::encode::ImageFormat;

/// SVG to PNG rasterizer with font support.
#[derive(Debug)]
pub struct Rasterizer {
    font_db: usvg::fontdb::Database,
}

/// Wraps text to fit within a specified width.
///
/// # Arguments
/// * `text` - The text to wrap
/// * `width` - Maximum line width in characters
///
/// # Returns
/// SVG tspan elements with wrapped text
pub fn wrap_text(text: &str, width: usize) -> String {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.len() + word.len() + 1 > width {
            lines.push(current_line);
            current_line = String::new();
        }
        if !current_line.is_empty() {
            current_line.push(' ');
        }
        current_line.push_str(word);
    }
    lines.push(current_line);

    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            format!(
                r#"<tspan x="16" dy="{}em">{}</tspan>"#,
                (i as f32 * 1.9) - 0.5,
                line
            )
        })
        .collect::<String>()
}

impl Rasterizer {
    #[instrument]
    pub fn new() -> Self {
        let mut fontdb = usvg::fontdb::Database::new();
        fontdb.load_system_fonts();

        // Try multiple font paths for different environments
        let font_paths = ["src/fonts", "fonts"];
        for path in &font_paths {
            if std::path::Path::new(path).exists() {
                fontdb.load_fonts_dir(path);
                break;
            }
        }

        Self { font_db: fontdb }
    }

    #[instrument(skip(self, svg_data))]
    pub fn render(&self, svg_data: &str) -> Result<tiny_skia::Pixmap> {
        self.render_with_scale(svg_data, Some(1.0))
    }

    #[instrument(skip(self, svg_data))]
    pub fn render_with_scale(
        &self,
        svg_data: &str,
        scale: Option<f64>,
    ) -> Result<tiny_skia::Pixmap> {
        let start_time = std::time::Instant::now();

        let options = usvg::Options {
            fontdb: std::sync::Arc::new(self.font_db.clone()),
            ..Default::default()
        };

        let tree = usvg::Tree::from_str(svg_data, &options)
            .map_err(|e| GlimError::Image(ImageError::SvgRendering(e.to_string())))?;

        // Get the original SVG dimensions
        let original_size = tree.size().to_int_size();
        let original_width = original_size.width() as f32;
        let original_height = original_size.height() as f32;

        // Apply scale factor (minimum 0.1 = 10%)
        let scale_factor = scale.unwrap_or(1.0).max(0.1) as f32;

        // Calculate new dimensions with padding that scales with scale factor
        let base_padding = 20.0; // Base padding in pixels
        let padding = (base_padding * scale_factor).min(20.0); // Scale padding but cap at 20px
        let new_width = (original_width * scale_factor) + (2.0 * padding);
        let new_height = (original_height * scale_factor) + (2.0 * padding);

        let pixmap_width = new_width as u32;
        let pixmap_height = new_height as u32;

        let mut pixmap = tiny_skia::Pixmap::new(pixmap_width, pixmap_height).ok_or_else(|| {
            GlimError::Image(ImageError::PixmapCreation(
                "Failed to create pixmap".to_string(),
            ))
        })?;

        // Calculate the transform to center the scaled content with padding
        let content_scale = scale_factor;
        let translate_x = padding;
        let translate_y = padding;

        let render_ts = tiny_skia::Transform::from_translate(translate_x, translate_y)
            .pre_scale(content_scale, content_scale);

        resvg::render(&tree, render_ts, &mut pixmap.as_mut());

        let duration = start_time.elapsed();
        let duration_ms = duration.as_millis();

        tracing::debug!(
            "SVG rasterization completed in {}ms (scale: {:?}, original: {}x{}, output: {}x{})",
            duration_ms,
            scale,
            original_width,
            original_height,
            pixmap_width,
            pixmap_height
        );

        if duration_ms > 1000 {
            tracing::warn!(
                "SVG rasterization took {}ms (>1000ms) (scale: {:?}, original: {}x{}, output: {}x{})",
                duration_ms,
                scale,
                original_width,
                original_height,
                pixmap_width,
                pixmap_height
            );
        }

        Ok(pixmap)
    }
}

impl Default for Rasterizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Parses a file extension to determine the image format.
///
/// # Arguments
/// * `extension` - The file extension (e.g., "png", "webp", "jpg")
///
/// # Returns
/// Some(ImageFormat) if the extension is supported, None otherwise
pub fn parse_extension(extension: &str) -> Option<ImageFormat> {
    match extension.to_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "webp" => Some(ImageFormat::WebP),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "svg" => Some(ImageFormat::Svg),
        "avif" => Some(ImageFormat::Avif),
        "gif" => Some(ImageFormat::Gif),
        "ico" => Some(ImageFormat::Ico),
        _ => None,
    }
}

/// Formats a number string to show thousands with "k" suffix.
///
/// # Arguments
/// * `count` - The count as a string
///
/// # Returns
/// Formatted string (e.g., "1200" -> "1.2k", "820" -> "820")
pub fn format_count(count: &str) -> String {
    if let Ok(num) = count.parse::<u32>() {
        if num >= 1000 {
            let thousands = num as f64 / 1000.0;
            if thousands >= 10.0 {
                format!("{}k", (thousands as u32))
            } else {
                format!("{:.1}k", thousands)
            }
        } else {
            count.to_string()
        }
    } else {
        count.to_string()
    }
}
