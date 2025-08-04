use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::{image, cli::Repository};

struct AppState {
    http_client: reqwest::Client,
}

pub async fn run(address: Option<String>) {
    let client = reqwest::Client::builder()
        .user_agent("livecards-server")
        .build()
        .unwrap();

    let app_state = Arc::new(AppState {
        http_client: client,
    });

    let app = Router::new()
        .route("/:owner/:repo", get(handler))
        .with_state(app_state);

    let addr = address
        .unwrap_or_else(|| "127.0.0.1:8000".to_string())
        .parse::<SocketAddr>()
        .unwrap();

    println!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handler(
    State(state): State<Arc<AppState>>,
    Path((owner, repo)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let repo_url = format!("https://api.github.com/repos/{}/{}", owner, repo);

    let repo: Repository = state
        .http_client
        .get(&repo_url)
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .json()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut temp_path = std::env::temp_dir();
    temp_path.push(format!("{}-{}.png", owner, repo.name));

    image::generate_image(
        &repo.name,
        &repo.description.unwrap_or_default(),
        &repo.language.unwrap_or_default(),
        &repo.stargazers_count.to_string(),
        &repo.forks_count.to_string(),
        &temp_path.to_string_lossy(),
    ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let image_data = tokio::fs::read(&temp_path)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tokio::fs::remove_file(&temp_path).await.ok();

    Ok((
        [(axum::http::header::CONTENT_TYPE, "image/png")],
        image_data,
    )
        .into_response())
}
