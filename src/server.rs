//! HTTP server for generating repository cards on demand.
//!
//! Provides a web API endpoint for generating PNG cards dynamically with rate limiting.

use axum::{
    extract::{ConnectInfo, Path, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use std::io::Cursor;
use std::net::SocketAddr;
use tracing::{info, instrument};

use crate::{
    errors::GitHubError,
    github, image,
    ratelimit::{RateLimitConfig, RateLimitResult, RateLimiter},
};

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
    let server_header = format!("livecards/{}", version);

    if let Ok(header_value) = axum::http::HeaderValue::from_str(&server_header) {
        response
            .headers_mut()
            .insert(axum::http::header::SERVER, header_value);
    }

    response
}

/// Starts the HTTP server.
///
/// # Arguments
/// * `address` - Optional server address (defaults to "127.0.0.1:8000")
pub async fn run(address: Option<String>) {
    // Initialize rate limiter with default configuration
    let rate_limiter = RateLimiter::new(RateLimitConfig::default());
    let app_state = AppState { rate_limiter };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/{owner}/{repo}", get(handler))
        .route("/status", get(status_handler))
        .route("/health", get(health_handler))
        .layer(middleware::from_fn(add_server_header))
        .with_state(app_state);

    let addr = address
        .unwrap_or_else(|| "127.0.0.1:8000".to_string())
        .parse::<SocketAddr>()
        .unwrap();

    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}

/// Handles index route - redirects to example repository.
///
/// Endpoint: GET /
/// Returns: Temporary redirect to /Xevion/livecards
#[instrument]
async fn index_handler() -> Redirect {
    Redirect::temporary("/Xevion/livecards")
}

/// Handles status route - returns rate limiter status.
///
/// Endpoint: GET /status
/// Returns: JSON with current rate limiter status
async fn status_handler(State(state): State<AppState>) -> Result<Response, StatusCode> {
    let status = state.rate_limiter.status().await;
    let json = status.to_string();

    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
        .into_response())
}

/// Handles health check route - returns simple OK response.
///
/// Endpoint: GET /health
/// Returns: 200 OK with "OK" text
async fn health_handler() -> Result<Response, StatusCode> {
    Ok(([(axum::http::header::CONTENT_TYPE, "text/plain")], "OK").into_response())
}

/// Handles HTTP requests for repository cards with rate limiting.
///
/// Endpoint: GET /:owner/:repo
/// Returns: PNG image of the repository card
#[axum::debug_handler]
async fn handler(
    Path((owner, repo_name)): Path<(String, String)>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let client_ip = addr.ip();

    // Check rate limit
    match state.rate_limiter.check_rate_limit(client_ip).await {
        RateLimitResult::Allowed => {
            // Continue with request processing
        }
        RateLimitResult::GlobalLimitExceeded => {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        RateLimitResult::IpLimitExceeded => {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
    }

    let repo_path = format!("{}/{}", owner, repo_name);
    let repo = github::get_repository_info(&repo_path, None)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get repository info: {}", e);
            match e {
                crate::errors::LivecardsError::GitHub(github_error) => github_error.into(),
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            }
        })?;

    let mut buffer = Cursor::new(Vec::new());

    image::generate_image(
        &repo.name,
        &repo.description.unwrap_or_default(),
        &repo.language.unwrap_or_default(),
        &repo.stargazers_count.to_string(),
        &repo.forks_count.to_string(),
        &mut buffer,
    )
    .map_err(|e| {
        tracing::error!("Failed to generate image: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/png")],
        buffer.into_inner(),
    )
        .into_response())
}
