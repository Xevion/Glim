//! HTTP server for generating repository cards on demand.
//!
//! Provides a web API endpoint for generating PNG cards dynamically.

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::io::Cursor;
use std::net::SocketAddr;
use tracing::{info, instrument};

use crate::{github, image};

/// Starts the HTTP server.
///
/// # Arguments
/// * `address` - Optional server address (defaults to "127.0.0.1:8000")
pub async fn run(address: Option<String>) {
    let app = Router::new().route("/:owner/:repo", get(handler));

    let addr = address
        .unwrap_or_else(|| "127.0.0.1:8000".to_string())
        .parse::<SocketAddr>()
        .unwrap();

    info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Handles HTTP requests for repository cards.
///
/// Endpoint: GET /:owner/:repo
/// Returns: PNG image of the repository card
#[instrument]
#[axum::debug_handler]
async fn handler(Path((owner, repo_name)): Path<(String, String)>) -> Result<Response, StatusCode> {
    let repo_path = format!("{}/{}", owner, repo_name);
    let repo = github::get_repository_info(&repo_path, None)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get repository info: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
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
