//! Image generation for repository cards.
//!
//! This module handles SVG template processing and PNG rasterization
//! to create beautiful repository cards with dynamic content.

use crate::colors;
use anyhow::Result;
use resvg::{tiny_skia, usvg};
use std::io::Write;
use tracing::instrument;

/// SVG to PNG rasterizer with font support.
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
fn wrap_text(text: &str, width: usize) -> String {
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
        fontdb.load_fonts_dir("src/fonts");
        Self { font_db: fontdb }
    }

    #[instrument(skip(self))]
    pub fn render(&self, svg_data: &str) -> Result<tiny_skia::Pixmap, anyhow::Error> {
        let options = usvg::Options {
            fontdb: std::sync::Arc::new(self.font_db.clone()),
            ..Default::default()
        };

        let tree = usvg::Tree::from_str(svg_data, &options)?;

        let pixmap_size = tree.size().to_int_size();
        let mut pixmap = tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
            .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap"))?;

        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );

        Ok(pixmap)
    }
}

/// Generates a PNG repository card image.
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
    mut writer: W,
) -> Result<()> {
    let svg_template = include_str!("../card.svg");
    let wrapped_description = wrap_text(description, 65);
    let language_color = colors::get_color(language).unwrap_or_else(|| "#f1e05a".to_string());

    let formatted_stars = format_count(stars);
    let formatted_forks = format_count(forks);

    let svg_filled = svg_template
        .replace("{{name}}", name)
        .replace("{{description}}", &wrapped_description)
        .replace("{{language}}", language)
        .replace("{{language_color}}", &language_color)
        .replace("{{stars}}", &formatted_stars)
        .replace("{{forks}}", &formatted_forks);

    let rasterizer = Rasterizer::new();
    let pixmap = rasterizer.render(&svg_filled)?;

    let mut png_encoder = png::Encoder::new(&mut writer, pixmap.width(), pixmap.height());
    png_encoder.set_color(png::ColorType::Rgba);
    png_encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = png_encoder.write_header()?;
    png_writer.write_image_data(pixmap.data())?;
    png_writer.finish()?;

    Ok(())
}

/// Formats a number string to show thousands with "k" suffix.
///
/// # Arguments
/// * `count` - The count as a string
///
/// # Returns
/// Formatted string (e.g., "1200" -> "1.2k", "820" -> "820")
fn format_count(count: &str) -> String {
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
