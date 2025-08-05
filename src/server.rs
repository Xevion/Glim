//! HTTP server for generating repository cards on demand.
//!
//! Provides a web API endpoint for generating PNG cards dynamically with rate limiting.

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::signal;
use tokio::time::timeout;
use tracing::{info, instrument};

use crate::{
    encode::Encoder,
    github, image,
    ratelimit::{RateLimitConfig, RateLimitResult, RateLimiter},
};
use std::path::Path as StdPath;

/// Error response structure for JSON error responses
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
    status: u16,
}

/// SVG input data for repository cards
#[derive(Debug, Clone)]
struct SvgInputData {
    name: String,
    description: String,
    language: String,
    stars: String,
    forks: String,
}

impl SvgInputData {
    fn new(
        name: String,
        description: String,
        language: String,
        stars: String,
        forks: String,
    ) -> Self {
        Self {
            name,
            description,
            language,
            stars,
            forks,
        }
    }
}

/// Query parameters for image generation
#[derive(Debug, Deserialize)]
struct ImageQuery {
    #[serde(rename = "scale")]
    scale: Option<String>,
    #[serde(rename = "s")]
    s: Option<String>,
}

/// Application state containing the rate limiter
#[derive(Clone, Debug)]
struct AppState {
    rate_limiter: RateLimiter,
}

/// Middleware to add Server header to all responses
async fn add_server_header(request: axum::extract::Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    // Get version from Cargo.toml
    let version = env!("CARGO_PKG_VERSION");
    let server_header = format!("glim/{}", version);

    if let Ok(header_value) = axum::http::HeaderValue::from_str(&server_header) {
        response
            .headers_mut()
            .insert(axum::http::header::SERVER, header_value);
    }

    response
}

/// Starts the HTTP server with graceful shutdown.
///
/// # Arguments
/// * `address` - Server address (e.g., "127.0.0.1:8000")
pub async fn start_server(address: String) {
    let rate_limiter = RateLimiter::new(RateLimitConfig::default());
    let app_state = AppState { rate_limiter };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/{owner}/{repo}", get(handler))
        .route("/status", get(status_handler))
        .route("/health", get(health_handler))
        .layer(middleware::from_fn(add_server_header))
        .with_state(app_state);

    let addr = match address.parse::<SocketAddr>() {
        Ok(addr) => addr,
        Err(e) => {
            tracing::error!("Invalid address '{}': {}", address, e);
            return;
        }
    };

    info!("Listening on http://{}", addr);

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("Failed to bind to address '{}': {}", addr, e);
            return;
        }
    };

    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    );

    let graceful = server.with_graceful_shutdown(shutdown_signal());

    info!("Server starting, press Ctrl+C to shut down.");

    if let Err(e) = graceful.await {
        tracing::error!("Server error: {}", e);
    }
}

/// Listens for the shutdown signal (Ctrl+C).
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Ctrl+C received, starting graceful shutdown.");
        },
        _ = terminate => {
            info!("Terminate signal received, starting graceful shutdown.");
        },
    }

    match timeout(Duration::from_secs(2), async {
        // TODO: Future cleanup logic
    })
    .await
    {
        Ok(_) => info!("Graceful shutdown complete."),
        Err(_) => tracing::warn!("Graceful shutdown timed out after 2 seconds."),
    }
}

/// Handles index route - redirects to example repository.
///
/// Endpoint: GET /
/// Returns: Temporary redirect to /Xevion/glim
#[instrument]
async fn index_handler() -> Redirect {
    Redirect::temporary("/Xevion/glim")
}

/// Handles status route - returns rate limiter status.
///
/// Endpoint: GET /status
/// Returns: JSON with current rate limiter status
async fn status_handler(State(state): State<AppState>) -> Response {
    let status = state.rate_limiter.status().await;
    let json = status.to_string();
    (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
        .into_response()
}

/// Handles health check route - returns simple OK response.
///
/// Endpoint: GET /health
/// Returns: 200 OK with "OK" text
async fn health_handler() -> Response {
    ([(axum::http::header::CONTENT_TYPE, "text/plain")], "OK").into_response()
}

/// Handles HTTP requests for repository cards with rate limiting.
///
/// Endpoint: GET /:owner/:repo or GET /:owner/:repo.:extension
/// Returns: Image in the requested format (PNG by default)
async fn handler(
    Path((owner, repo_name)): Path<(String, String)>,
    Query(query): Query<ImageQuery>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    let client_ip = addr.ip();

    // Check rate limit
    match state.rate_limiter.check_rate_limit(client_ip).await {
        RateLimitResult::Allowed => {
            // Continue with request processing
        }
        RateLimitResult::GlobalLimitExceeded => {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorResponse {
                    error: "rate_limit_exceeded".to_string(),
                    message: "Global rate limit exceeded".to_string(),
                    status: 429,
                }),
            ));
        }
        RateLimitResult::IpLimitExceeded => {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                Json(ErrorResponse {
                    error: "rate_limit_exceeded".to_string(),
                    message: "IP rate limit exceeded".to_string(),
                    status: 429,
                }),
            ));
        }
    }

    // Parse format from repo_name (e.g., "repo.png" -> format PNG, "repo" -> format PNG)
    let (actual_repo_name, format) = parse_repo_name_and_format(&repo_name);

    let repo_path = format!("{}/{}", owner, actual_repo_name);
    let repo = github::GITHUB_CLIENT
        .get_repository_info(&repo_path)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get repository info: {}", e);
            let status_code = match &e {
                crate::errors::GlimError::GitHub(github_error) => github_error.clone().into(),
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            (
                status_code,
                Json(ErrorResponse {
                    error: "repository_error".to_string(),
                    message: format!("Failed to get repository info: {}", e),
                    status: status_code.as_u16(),
                }),
            )
        })?;

    // Start timing for image generation
    let start_time = std::time::Instant::now();

    // Create SVG input data
    let svg_data = SvgInputData::new(
        repo.name,
        repo.description.unwrap_or_default(),
        repo.language.unwrap_or_default(),
        repo.stargazers_count.to_string(),
        repo.forks_count.to_string(),
    );

    // Format the SVG template
    let formatted_svg = format_svg_template(&svg_data);

    // Parse scale parameter
    let scale = parse_scale_parameter(&query);

    // Encode the image
    let mut buffer = Cursor::new(Vec::new());
    let encoder = crate::encode::create_encoder(format);

    encoder
        .encode(&formatted_svg, &mut buffer, scale)
        .map_err(|e| {
            tracing::error!("Failed to generate image: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "image_generation_error".to_string(),
                    message: format!("Failed to generate image: {}", e),
                    status: 500,
                }),
            )
        })?;

    // Calculate timing
    let duration = start_time.elapsed();
    let duration_ms = duration.as_millis();

    tracing::debug!(
        "Image generation completed in {}ms for {}/{} (format: {:?}, scale: {:?})",
        duration_ms,
        owner,
        actual_repo_name,
        format,
        scale
    );

    if duration_ms > 1000 {
        tracing::warn!(
            "Image generation took {}ms (>1000ms) for {}/{} (format: {:?}, scale: {:?})",
            duration_ms,
            owner,
            actual_repo_name,
            format,
            scale
        );
    }

    Ok((
        [(axum::http::header::CONTENT_TYPE, format.mime_type())],
        buffer.into_inner(),
    )
        .into_response())
}

/// Parses the repository name and format from the path.
///
/// # Arguments
/// * `repo_name` - The repository name which may include an extension
///
/// # Returns
/// Tuple of (actual_repo_name, format)
pub fn parse_repo_name_and_format(repo_name: &str) -> (String, crate::encode::ImageFormat) {
    let path = StdPath::new(repo_name);

    if let Some(extension) = path.extension() {
        if let Some(extension_str) = extension.to_str() {
            if let Some(format) = image::parse_extension(extension_str) {
                // Valid extension found, remove it from repo name
                let actual_repo_name = path.with_extension("").to_string_lossy().to_string();
                return (actual_repo_name, format);
            }
        }
    }

    // No valid extension found or unsupported extension - treat as part of repo name
    // This allows repositories like "vercel/next.js" to work normally
    (repo_name.to_string(), crate::encode::ImageFormat::Png)
}

/// Parses the scale parameter from query parameters.
///
/// # Arguments
/// * `query` - The query parameters
///
/// # Returns
/// Optional scale factor (None if not provided or invalid)
fn parse_scale_parameter(query: &ImageQuery) -> Option<f64> {
    // Try 'scale' parameter first, then fallback to 's'
    let scale_str = query.scale.as_deref().or(query.s.as_deref())?;

    // Ignore if length is greater than 4 characters after trimming trailing zeros
    let trimmed = scale_str.trim_end_matches('0');
    if trimmed.len() > 10 {
        return None;
    }

    // Parse as f64, clamp between 0.1 and 3.5 in release, 0.1 and 100.0 in debug
    Some(scale_str.parse::<f64>().ok()?.clamp(0.1, {
        #[cfg(not(debug_assertions))]
        {
            3.5
        }
        #[cfg(debug_assertions)]
        {
            100.0
        }
    }))
}

/// Formats the SVG template with repository data.
///
/// # Arguments
/// * `data` - The SVG input data containing repository information
///
/// # Returns
/// Formatted SVG string
fn format_svg_template(data: &SvgInputData) -> String {
    let start_time = std::time::Instant::now();

    let svg_template = include_str!("../card.svg");
    let wrapped_description = crate::image::wrap_text(&data.description, 65);
    let language_color =
        crate::colors::get_color(&data.language).unwrap_or_else(|| "#f1e05a".to_string());

    let formatted_stars = crate::image::format_count(&data.stars);
    let formatted_forks = crate::image::format_count(&data.forks);

    let result = svg_template
        .replace("{{name}}", &data.name)
        .replace("{{description}}", &wrapped_description)
        .replace("{{language}}", &data.language)
        .replace("{{language_color}}", &language_color)
        .replace("{{stars}}", &formatted_stars)
        .replace("{{forks}}", &formatted_forks);

    let duration = start_time.elapsed();
    let duration_ms = duration.as_millis();

    tracing::debug!(
        "SVG template formatting completed in {}ms for repository: {}",
        duration_ms,
        data.name
    );

    if duration_ms > 1000 {
        tracing::warn!(
            "SVG template formatting took {}ms (>1000ms) for repository: {}",
            duration_ms,
            data.name
        );
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scale_parameter() {
        // Test valid scale parameters
        let query = ImageQuery {
            scale: Some("1.5".to_string()),
            s: None,
        };
        assert_eq!(parse_scale_parameter(&query), Some(1.5));

        let query = ImageQuery {
            scale: None,
            s: Some("2.0".to_string()),
        };
        assert_eq!(parse_scale_parameter(&query), Some(2.0));

        // Test fallback from scale to s
        let query = ImageQuery {
            scale: None,
            s: Some("1.2".to_string()),
        };
        assert_eq!(parse_scale_parameter(&query), Some(1.2));

        // Test invalid parameters
        let query = ImageQuery {
            scale: Some("0.05".to_string()), // Below minimum - gets clamped to 0.1
            s: None,
        };
        assert_eq!(parse_scale_parameter(&query), Some(0.1));

        let query = ImageQuery {
            scale: Some("12345678901".to_string()), // Too long after trimming (>10 chars)
            s: None,
        };
        assert_eq!(parse_scale_parameter(&query), None);

        let query = ImageQuery {
            scale: Some("abc".to_string()), // Invalid number
            s: None,
        };
        assert_eq!(parse_scale_parameter(&query), None);

        // Test no parameters
        let query = ImageQuery {
            scale: None,
            s: None,
        };
        assert_eq!(parse_scale_parameter(&query), None);
    }

    #[test]
    fn test_scale_parameter_length_validation() {
        // Test that trailing zeros are trimmed correctly
        let query = ImageQuery {
            scale: Some("1.2000".to_string()),
            s: None,
        };
        assert_eq!(parse_scale_parameter(&query), Some(1.2));

        // Test that long strings are rejected (>10 chars after trimming)
        let query = ImageQuery {
            scale: Some("1.2345678901".to_string()),
            s: None,
        };
        assert_eq!(parse_scale_parameter(&query), None);
    }
}
