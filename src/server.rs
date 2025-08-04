use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use std::net::SocketAddr;

use crate::{github, image};

pub async fn run(address: Option<String>) {
    let app = Router::new().route("/:owner/:repo", get(handler));

    let addr = address
        .unwrap_or_else(|| "127.0.0.1:8000".to_string())
        .parse::<SocketAddr>()
        .unwrap();

    println!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[axum::debug_handler]
async fn handler(Path((owner, repo_name)): Path<(String, String)>) -> Result<Response, StatusCode> {
    let repo_path = format!("{}/{}", owner, repo_name);
    let repo = github::get_repository_info(&repo_path, None)
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
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

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
