//! Image generation for repository cards.
//!
//! This module handles SVG template processing and multi-format encoding
//! to create beautiful repository cards with dynamic content.

use crate::encode::{self, ImageFormat};
use crate::errors::{ImageError, LivecardsError, Result};
use resvg::{tiny_skia, usvg};
use std::io::Write;
use tracing::instrument;

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

    #[instrument(skip(self))]
    pub fn render(&self, svg_data: &str) -> Result<tiny_skia::Pixmap> {
        let options = usvg::Options {
            fontdb: std::sync::Arc::new(self.font_db.clone()),
            ..Default::default()
        };

        let tree = usvg::Tree::from_str(svg_data, &options)
            .map_err(|e| LivecardsError::Image(ImageError::SvgRendering(e.to_string())))?;

        let pixmap_size = tree.size().to_int_size();
        let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
            .ok_or_else(|| LivecardsError::Image(ImageError::PixmapCreation))?;

        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );

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

/// Generates a repository card image in the specified format.
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
pub fn generate_image_with_format<W: Write>(
    name: &str,
    description: &str,
    language: &str,
    stars: &str,
    forks: &str,
    format: ImageFormat,
    writer: W,
) -> Result<()> {
    encode::generate_image(name, description, language, stars, forks, format, writer)
}

/// Generates a PNG repository card image (legacy function for backward compatibility).
///
/// # Arguments
/// * `name` - Repository name
/// * `description` - Repository description
/// * `language` - Primary programming language
/// * `stars` - Star count as string
/// * `forks` - Fork count as string
/// * `writer` - Output writer for PNG data
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
    writer: W,
) -> Result<()> {
    generate_image_with_format(
        name,
        description,
        language,
        stars,
        forks,
        ImageFormat::Png,
        writer,
    )
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
