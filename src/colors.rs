//! Language color mappings from GitHub Linguist.
//!
//! This module provides access to the official GitHub language colors
//! that are generated at build time from the GitHub Linguist project.

include!(concat!(env!("OUT_DIR"), "/colors.rs"));

/// Gets the color for a programming language.
///
/// # Arguments
/// * `lang` - Programming language name
///
/// # Returns
/// Hex color string if language is known, None otherwise
pub fn get_color(lang: &str) -> Option<String> {
    COLORS.get(lang).map(|s| s.to_string())
}
