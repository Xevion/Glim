//! Centralized error handling for the livecards application.
//!
//! This module provides a unified error type that consolidates all
//! application errors into a single enum for better error handling.

use thiserror::Error;

/// Unified error type for the livecards application.
#[allow(clippy::enum_variant_names)]
#[derive(Error, Debug)]
pub enum LivecardsError {
    /// GitHub API related errors
    #[error("GitHub API error: {0}")]
    GitHub(#[from] GitHubError),

    /// Image generation errors
    #[error("Image generation error: {0}")]
    Image(#[from] ImageError),

    /// Server/HTTP related errors
    #[error("Server error: {0}")]
    Server(#[from] ServerError),

    /// CLI/argument parsing errors
    #[error("CLI error: {0}")]
    Cli(#[from] CliError),

    /// General I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Network/HTTP client errors
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// JSON serialization/deserialization errors
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// SVG processing errors
    #[error("SVG error: {0}")]
    Svg(#[from] usvg::Error),

    /// Image rasterization errors
    #[error("Rasterization error: {0}")]
    Rasterization(String),

    /// Font loading errors
    #[error("Font error: {0}")]
    Font(String),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Rate limiting errors
    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),

    /// Validation errors
    #[error("Validation error: {0}")]
    Validation(String),
}

/// GitHub API specific errors
#[derive(Error, Debug, Clone)]
pub enum GitHubError {
    /// Repository not found (404)
    #[error("Repository not found")]
    NotFound,

    /// Rate limit exceeded (403)
    #[error("GitHub API rate limit exceeded")]
    RateLimited,

    /// API error with status code
    #[error("GitHub API error: {0}")]
    ApiError(u16),

    /// Network or parsing error
    #[error("Network error while contacting GitHub API")]
    NetworkError,

    /// Invalid repository format
    #[error("Invalid repository format: {0}")]
    InvalidFormat(String),

    /// Authentication error
    #[error("Authentication failed: {0}")]
    AuthError(String),
}

/// Image generation specific errors
#[derive(Error, Debug)]
pub enum ImageError {
    /// Failed to create pixmap
    #[error("Failed to create pixmap")]
    PixmapCreation,

    /// Failed to render SVG
    #[error("Failed to render SVG: {0}")]
    SvgRendering(String),

    /// Failed to write PNG
    #[error("Failed to write PNG: {0}")]
    PngWrite(String),

    /// Invalid image dimensions
    #[error("Invalid image dimensions: {0}x{1}")]
    InvalidDimensions(u32, u32),

    /// Font loading error
    #[error("Font loading error: {0}")]
    FontError(String),
}

/// Server/HTTP specific errors
#[derive(Error, Debug)]
pub enum ServerError {
    /// Failed to bind to address
    #[error("Failed to bind to address: {0}")]
    BindError(String),

    /// Failed to start server
    #[error("Failed to start server: {0}")]
    StartError(String),

    /// Invalid address format
    #[error("Invalid address format: {0}")]
    InvalidAddress(String),

    /// Server shutdown error
    #[error("Server shutdown error: {0}")]
    ShutdownError(String),
}

/// CLI/argument parsing specific errors
#[derive(Error, Debug)]
pub enum CliError {
    /// Missing required argument
    #[error("Missing required argument: {0}")]
    MissingArgument(String),

    /// Invalid argument value
    #[error("Invalid argument value: {0}")]
    InvalidArgument(String),

    /// Missing output file
    #[error("Missing output file")]
    MissingOutput,

    /// Invalid output path
    #[error("Invalid output path: {0}")]
    InvalidOutputPath(String),

    /// Invalid log level
    #[error("Invalid log level: {0}")]
    InvalidLogLevel(String),
}

/// Type alias for Result using the unified error type
pub type Result<T> = std::result::Result<T, LivecardsError>;

/// Convert GitHub API errors to HTTP status codes
impl From<GitHubError> for axum::http::StatusCode {
    fn from(error: GitHubError) -> Self {
        match error {
            GitHubError::NotFound => axum::http::StatusCode::NOT_FOUND,
            GitHubError::RateLimited => axum::http::StatusCode::TOO_MANY_REQUESTS,
            GitHubError::ApiError(403) => axum::http::StatusCode::TOO_MANY_REQUESTS,
            GitHubError::ApiError(401) => axum::http::StatusCode::UNAUTHORIZED,
            GitHubError::ApiError(_) => axum::http::StatusCode::BAD_GATEWAY,
            GitHubError::NetworkError => axum::http::StatusCode::BAD_GATEWAY,
            GitHubError::InvalidFormat(_) => axum::http::StatusCode::BAD_REQUEST,
            GitHubError::AuthError(_) => axum::http::StatusCode::UNAUTHORIZED,
        }
    }
}
