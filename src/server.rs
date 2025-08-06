//! HTTP server for generating repository cards on demand.
//!
//! Provides a web API endpoint for generating PNG cards dynamically with rate limiting.

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use socket2::{Domain, Socket, Type};
use std::{
    collections::HashSet,
    env,
    io::Cursor,
    net::{IpAddr, Ipv4Addr},
    num::ParseIntError,
};
use std::{
    net::AddrParseError,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use std::{net::SocketAddrV6, path::Path as StdPath};
use std::{
    net::{Ipv6Addr, SocketAddr},
    str::FromStr,
};
use terrors::OneOf;
use tokio::signal;
use tokio::time::timeout;
use tracing::{info, instrument};

use crate::{
    encode::Encoder,
    github,
    image::{self, ImageFormat},
    ratelimit::{RateLimitConfig, RateLimitResult, RateLimiter},
};
use once_cell::sync::Lazy;

/// Lazy-loaded healthcheck token from environment variable
static HEALTHCHECK_TOKEN: Lazy<Option<String>> = Lazy::new(|| env::var("HEALTHCHECK_TOKEN").ok());

/// Lazy-loaded hostname that should bypass healthcheck authorization
static HEALTHCHECK_HOST_BYPASS: Lazy<Option<String>> =
    Lazy::new(|| env::var("HEALTHCHECK_HOST_BYPASS").ok());

/// Error response structure for JSON error responses
#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
    status: u16,
}

/// Health check response structure
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    timestamp: u64,
    uptime_seconds: u64,
    version: String,
    components: ComponentStatus,
}

/// Component status for health checks
#[derive(Debug, Serialize)]
struct ComponentStatus {
    rate_limiter: RateLimiterHealth,
    github_api: GitHubApiHealth,
}

/// Rate limiter health status
#[derive(Debug, Serialize)]
struct RateLimiterHealth {
    status: String,
    global_tokens_remaining: u32,
    global_tokens_max: u32,
    active_ip_count: u32,
    utilization_percent: f32,
}

/// GitHub API health status
#[derive(Debug, Serialize)]
struct GitHubApiHealth {
    status: String,
    circuit_breaker_open: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_error: Option<String>,
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
pub struct ImageQuery {
    #[serde(rename = "scale")]
    pub scale: Option<String>,
    #[serde(rename = "s")]
    pub s: Option<String>,
}

/// Application state containing the rate limiter and startup time
#[derive(Clone, Debug)]
struct AppState {
    rate_limiter: RateLimiter,
    startup_time: Instant,
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

/// Checks if the system is configured to bind to IPv6 only.
///
/// This function creates a temporary IPv6 socket and checks the `IPV6_V6ONLY`
/// socket option. This is a cross-platform way to determine if binding to an
/// IPv6 socket will also bind to the corresponding IPv4 address.
fn is_ipv6_only() -> std::io::Result<bool> {
    let socket = Socket::new(Domain::IPV6, Type::STREAM, None)?;
    socket.only_v6()
}

/// Starts the HTTP server with graceful shutdown on multiple addresses.
///
/// # Arguments
/// * `addresses` - Vector of server addresses to bind to
///
/// # Returns
/// * `None` - Server was interrupted (Ctrl+C)
/// * `Some(Err)` - Server encountered an error
pub async fn start_server(mut addresses: Vec<SocketAddr>) -> Option<Result<(), anyhow::Error>> {
    if addresses.is_empty() {
        return Some(Err(anyhow::Error::msg("No addresses provided")));
    }

    {
        // Check for duplicate addresses
        let mut seen = HashSet::new();
        for addr in &addresses {
            if !seen.insert(addr) {
                return Some(Err(anyhow::Error::msg(format!(
                    "Explicit duplicate address found: {}",
                    addr
                ))));
            }
        }
    }

    // If we are binding to an IPv6 address, and the system is not configured
    // to bind to IPv6 only, then we should filter out any IPv4 addresses on
    // the same port.
    if addresses.iter().any(|a| a.is_ipv6()) {
        if let Ok(false) = is_ipv6_only() {
            let ipv6_ports: HashSet<u16> = addresses
                .iter()
                .filter(|a| a.is_ipv6())
                .map(|a| a.port())
                .collect();

            addresses.retain(|a| {
                if a.is_ipv4() && ipv6_ports.contains(&a.port()) {
                    tracing::warn!(
                        "Ignoring IPv4 address {} because it conflicts with an IPv6 address on the same port due to IPv6 dual-stack.",
                        a
                    );
                    false
                } else {
                    true
                }
            });
        }
    }

    let rate_limiter = RateLimiter::new(RateLimitConfig::default());
    let app_state = AppState {
        rate_limiter,
        startup_time: Instant::now(),
    };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/{owner}/{repo}", get(handler))
        .route("/status", get(status_handler))
        .route("/health", get(health_handler))
        .layer(middleware::from_fn(add_server_header))
        .with_state(app_state);

    // Bind to all addresses and collect listeners
    let mut listeners = Vec::new();
    for address in &addresses {
        match tokio::net::TcpListener::bind(address).await {
            Ok(listener) => listeners.push((address, listener)),
            Err(e) => {
                return Some(Err(anyhow::Error::msg(format!(
                    "Failed to bind to address '{}': {}",
                    address, e
                ))));
            }
        }
    }

    // Create axum servers for each listener
    let mut servers = Vec::new();
    for (address, listener) in listeners {
        let server = axum::serve(
            listener,
            app.clone()
                .into_make_service_with_connect_info::<SocketAddr>(),
        );
        servers.push((address, server));
    }

    info!(
        addresses = ?addresses,
        "Server starting on {} address(es), press Ctrl+C to shut down.",
        addresses.len()
    );

    // Setup shutdown broadcast channel for coordinated graceful shutdown
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);

    // Spawn server tasks with graceful shutdown capability
    let mut handles: Vec<tokio::task::JoinHandle<Result<(), anyhow::Error>>> = Vec::new();
    for (address, server) in servers {
        let address_clone = *address;
        let mut shutdown_rx = shutdown_tx.subscribe();

        // Configure graceful shutdown to wait for broadcast signal
        let graceful = server.with_graceful_shutdown(async move {
            let _ = shutdown_rx.recv().await;
        });

        // Spawn each server in its own task
        let handle = tokio::spawn(async move {
            if let Err(e) = graceful.await {
                tracing::error!("Server error on {}: {}", address_clone, e);
                return Err(anyhow::Error::msg(e.to_string()));
            }
            Ok(())
        });
        handles.push(handle);
    }

    // Wait for either all servers to complete or shutdown signal
    let server_future = async {
        // Wait for all handles and fail fast if any server fails
        for handle in handles {
            if let Err(e) = handle.await {
                return Some(Err(anyhow::Error::msg(e.to_string())));
            }
        }
        None
    };

    tokio::select! {
        result = server_future => result,
        _ = shutdown_signal() => {
            let _ = shutdown_tx.send(());
            None // Interrupt occurred
        }
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

/// Check if the request is authorized for health check access.
///
/// Authorization logic:
/// - Configured hostname bypass: bypass authorization when coming from HEALTHCHECK_HOST_BYPASS
/// - In debug mode: allow access if no token configured, validate if configured
/// - In release mode: require valid token if HEALTHCHECK_TOKEN is configured
/// - Token can be provided via Authorization Bearer header or 'token' query parameter
fn is_health_check_authorized(headers: &HeaderMap, query: &HealthQuery) -> bool {
    // Check if this is a request from a configured bypass hostname
    if let Some(bypass_hostname) = HEALTHCHECK_HOST_BYPASS.as_ref() {
        if let Some(host_header) = headers.get("host") {
            if let Ok(host_str) = host_header.to_str() {
                if host_str == bypass_hostname {
                    return true; // Allow healthchecks from configured hostname to bypass authorization
                }
            }
        }
    }

    let expected_token = match HEALTHCHECK_TOKEN.as_ref() {
        Some(token) => token,
        None => {
            // No token configured
            if cfg!(debug_assertions) {
                return true; // Allow access in debug mode
            } else {
                return false; // Deny access in release mode
            }
        }
    };

    // Token is configured, validate it
    // Check Authorization Bearer header first
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return token == expected_token;
            }
        }
    }

    // Fallback to query parameter
    if let Some(query_token) = &query.token {
        return query_token == expected_token;
    }

    false
}

/// Query parameters for health check endpoint
#[derive(Debug, Deserialize)]
struct HealthQuery {
    token: Option<String>,
}

/// Handles health check route - returns comprehensive health status.
///
/// Endpoint: GET /health
/// Returns: JSON with detailed system health information including:
/// - Service status and uptime
/// - Rate limiter status
/// - GitHub API connectivity
/// - Component health checks
///
/// Authentication:
/// - Debug mode: always accessible
/// - Release mode: requires HEALTHCHECK_TOKEN via Authorization Bearer or token query param
#[instrument(skip(state, headers, query))]
async fn health_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HealthQuery>,
) -> Response {
    // Check authorization
    if !is_health_check_authorized(&headers, &query) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Unauthorized".to_string(),
                message: "Valid authentication token required".to_string(),
                status: 401,
            }),
        )
            .into_response();
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let uptime = state.startup_time.elapsed().as_secs();

    // Check rate limiter health
    let rate_limit_status = state.rate_limiter.status().await;
    let utilization = if rate_limit_status.global_tokens_max > 0 {
        100.0
            - (rate_limit_status.global_tokens_remaining as f32
                / rate_limit_status.global_tokens_max as f32
                * 100.0)
    } else {
        0.0
    };

    let rate_limiter_health = RateLimiterHealth {
        status: if rate_limit_status.global_tokens_remaining > 0 {
            "healthy"
        } else {
            "degraded"
        }
        .to_string(),
        global_tokens_remaining: rate_limit_status.global_tokens_remaining,
        global_tokens_max: rate_limit_status.global_tokens_max,
        active_ip_count: rate_limit_status.active_ip_count,
        utilization_percent: utilization,
    };

    // Check GitHub API health
    let github_client = &github::GITHUB_CLIENT;
    let circuit_breaker_open = github_client.disabled();

    // Perform a lightweight GitHub API check if token is available and circuit breaker is closed
    let (github_status, last_error) = if circuit_breaker_open {
        ("degraded", Some("Circuit breaker is open".to_string()))
    } else {
        // Try a quick validation call
        match tokio::time::timeout(
            Duration::from_secs(2),
            github_client.fetch_repository_info("torvalds/linux"),
        )
        .await
        {
            Ok(Ok(_)) => ("healthy", None),
            Ok(Err(e)) => ("degraded", Some(e.to_string())),
            Err(_) => ("degraded", Some("Token validation timeout".to_string())),
        }
    };

    let github_health = GitHubApiHealth {
        status: github_status.to_string(),
        circuit_breaker_open,
        last_error,
    };

    // Determine overall status
    let overall_status = if github_status == "healthy" && rate_limiter_health.status == "healthy" {
        "healthy"
    } else if github_status == "degraded" || rate_limiter_health.status == "degraded" {
        "degraded"
    } else {
        "warning"
    };

    let health_response = HealthResponse {
        status: overall_status.to_string(),
        timestamp: now,
        uptime_seconds: uptime,
        version: env!("CARGO_PKG_VERSION").to_string(),
        components: ComponentStatus {
            rate_limiter: rate_limiter_health,
            github_api: github_health,
        },
    };

    let status_code = match overall_status {
        "healthy" => StatusCode::OK,
        "warning" => StatusCode::OK,
        "degraded" => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };

    (status_code, Json(health_response)).into_response()
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
    let (actual_repo_name, format) = {
        let (actual_repo_name, format) = parse_repo_name_and_format(&repo_name);
        (actual_repo_name, format.unwrap_or(ImageFormat::Png))
    };

    let repo_path = format!("{}/{}", owner, actual_repo_name);

    // Start GitHub API timing
    let github_start = Instant::now();
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
    let github_api_duration = github_start.elapsed();

    tracing::debug!(
        owner = &owner,
        repo = &actual_repo_name,
        duration = ?github_api_duration,
        "GitHub API request completed"
    );

    // Start overall image generation timing
    let total_start = Instant::now();

    // Create SVG input data
    let svg_data = SvgInputData::new(
        repo.name,
        repo.description.unwrap_or_default(),
        repo.language.unwrap_or_default(),
        repo.stargazers_count.to_string(),
        repo.forks_count.to_string(),
    );

    // Format the SVG template with timing
    let svg_start = Instant::now();
    let formatted_svg = format_svg_template(&svg_data);
    let svg_template_duration = svg_start.elapsed();

    tracing::debug!(
        owner = &owner,
        repo = &actual_repo_name,
        duration = ?svg_template_duration,
        "SVG template rendered"
    );

    // Parse scale parameter
    let scale = parse_scale_parameter(&query);

    // Encode the image with timing
    let mut buffer = Cursor::new(Vec::new());
    let encoder = crate::encode::create_encoder(format);

    let encoding_timing = encoder
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

    tracing::debug!(
        owner = &owner,
        repo = &actual_repo_name,
        format = ?format,
        scale = ?scale,
        rasterization_duration = ?encoding_timing.rasterization,
        encoding_duration = ?encoding_timing.encoding,
        "Image encoding completed"
    );

    // Calculate total timing and create breakdown
    let total_duration = total_start.elapsed();
    let mut timing = ImageGenerationTiming::new();
    timing.github_api = github_api_duration;
    timing.svg_template = svg_template_duration;
    timing.rasterization = encoding_timing.rasterization;
    timing.encoding = encoding_timing.encoding;
    timing.total = total_duration;

    // Log detailed timing breakdown
    timing.log_timing_breakdown(&owner, &actual_repo_name, &format, scale);

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
pub fn parse_repo_name_and_format(repo_name: &str) -> (String, Option<ImageFormat>) {
    let path = StdPath::new(repo_name);

    if let Some(extension) = path.extension() {
        if let Some(extension_str) = extension.to_str() {
            if let Some(format) = image::parse_extension(extension_str) {
                // Valid extension found, remove it from repo name
                let actual_repo_name = path.with_extension("").to_string_lossy().to_string();
                return (actual_repo_name, Some(format));
            }
        }
    }

    // No valid extension found or unsupported extension - treat as part of repo name
    // This allows repositories like "vercel/next.js" to work normally
    (repo_name.to_string(), None)
}

/// Parses the scale parameter from query parameters.
///
/// # Arguments
/// * `query` - The query parameters
///
/// # Returns
/// Optional scale factor (None if not provided or invalid)
pub fn parse_scale_parameter(query: &ImageQuery) -> Option<f64> {
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
    let svg_template = {
        #[cfg(debug_assertions)]
        {
            tracing::debug!("Loading card.svg from current directory");

            // Load at runtime to allow for hot reloading
            std::fs::read_to_string("card.svg").unwrap_or_else(|_| {
                tracing::warn!(
                    "Failed to load card.svg from current directory, using embedded template"
                );
                include_str!("../card.svg").to_string()
            })
        }

        #[cfg(not(debug_assertions))]
        {
            // Load at compile time as it generally won't be changing
            include_str!("../card.svg")
        }
    };
    let wrapped_description = crate::image::wrap_text(&data.description, 65);
    let language_color =
        crate::colors::get_color(&data.language).unwrap_or_else(|| "#f1e05a".to_string());

    let formatted_stars = crate::image::format_count(&data.stars);
    let formatted_forks = crate::image::format_count(&data.forks);

    svg_template
        .replace("{{name}}", &data.name)
        .replace("{{description}}", &wrapped_description)
        .replace("{{language}}", &data.language)
        .replace("{{language_color}}", &language_color)
        .replace("{{stars}}", &formatted_stars)
        .replace("{{forks}}", &formatted_forks)
}

/// Detailed timing breakdown for image generation phases
#[derive(Debug)]
struct ImageGenerationTiming {
    github_api: Duration,
    svg_template: Duration,
    rasterization: Duration,
    encoding: Duration,
    total: Duration,
}

impl ImageGenerationTiming {
    fn new() -> Self {
        Self {
            github_api: Duration::ZERO,
            svg_template: Duration::ZERO,
            rasterization: Duration::ZERO,
            encoding: Duration::ZERO,
            total: Duration::ZERO,
        }
    }

    fn log_timing_breakdown(
        &self,
        owner: &str,
        repo: &str,
        format: &crate::encode::ImageFormat,
        scale: Option<f64>,
    ) {
        let total_ms = self.total.as_millis();

        tracing::debug!(
            owner = owner,
            repo = repo,
            format = ?format,
            scale = ?scale,
            github_api_duration = ?self.github_api,
            svg_template_duration = ?self.svg_template,
            rasterization_duration = ?self.rasterization,
            encoding_duration = ?self.encoding,
            total_duration = ?self.total,
            "Image generation completed"
        );

        if total_ms > 1000 {
            tracing::warn!(
                owner = owner,
                repo = repo,
                format = ?format,
                scale = ?scale,
                github_api_duration = ?self.github_api,
                svg_template_duration = ?self.svg_template,
                rasterization_duration = ?self.rasterization,
                encoding_duration = ?self.encoding,
                total_duration = ?self.total,
                "Slow image generation"
            );
        }
    }
}

/// Parse the address components from a string, allowing for either a full address (host:port), just a host, or just a port.
///
/// This function does not apply any kind of defaulting, and will return an error if the address is invalid.
///
/// # Examples
///
/// ```
/// use glim::server::parse_address_components;
///
/// // Full socket address
/// let result = parse_address_components("127.0.0.1:8080");
/// // Returns Ok(OneOf::A(SocketAddr))
///
/// // Just an IPv4 address (colon can be omitted)
/// let result = parse_address_components("192.168.1.1:");
/// // Returns Ok(OneOf::B(IpAddr))
///
/// // Just an IPv6 address (colon can be omitted)
/// let result = parse_address_components("[::1]");
/// // Returns Ok(OneOf::B(IpAddr))
///
/// // Just a port number (colon can be omitted)
/// let result = parse_address_components(":3000");
/// // Returns Ok(OneOf::C(u16))
///
/// // Invalid input
/// let result = parse_address_components("invalid");
/// // Returns Err(...)
/// ```
#[allow(clippy::type_complexity)]
pub fn parse_address_components(
    input: &str,
) -> Result<OneOf<(SocketAddr, IpAddr, u16)>, OneOf<(anyhow::Error, AddrParseError, ParseIntError)>>
{
    // Check if it's an ipv6 address before trying to split
    if input.starts_with('[') {
        // Does it look like an ipv6 address without a port?
        let no_port = input.ends_with(']') || input.ends_with("]:");

        // If so, parse it as an ipv6 address
        if no_port {
            return match Ipv6Addr::from_str(input) {
                Ok(addr) => Ok(OneOf::new(IpAddr::V6(addr))),
                Err(e) => Err(OneOf::new(e)),
            };
        }

        // Otherwise, we'll assume it's an ipv6 address with a port
        return match SocketAddrV6::from_str(input) {
            Ok(addr) => Ok(OneOf::new(SocketAddr::V6(addr))),
            Err(e) => Err(OneOf::new(e)),
        };
    }

    let (host, port) = match input.split_once(':') {
        Some((host, port)) => {
            // Check the length of each component
            (
                if !host.is_empty() { Some(host) } else { None },
                if !port.is_empty() { Some(port) } else { None },
            )
        }
        None => {
            // If there's no colon, we need to figure out if it's a host or a port
            if input.contains('.') {
                // It's probably an ipv4 address
                (Some(input), None)
            } else {
                // Assume it's a port
                (None, Some(input))
            }
        }
    };

    // Now just parse the components individually or together, and return the appropriate type
    match (host, port) {
        (Some(host), Some(port)) => {
            let host = host.parse::<Ipv4Addr>().map_err(OneOf::new)?;
            let port = port.parse::<u16>().map_err(OneOf::new)?;
            Ok(OneOf::new(SocketAddr::from((host, port))))
        }
        (Some(host), None) => {
            let host = host.parse::<Ipv4Addr>().map_err(OneOf::new)?;
            Ok(OneOf::new(IpAddr::V4(host)))
        }
        (None, Some(port)) => {
            let port = port.parse::<u16>().map_err(OneOf::new)?;
            Ok(OneOf::new(port))
        }
        (None, None) => Err(OneOf::new(anyhow::Error::msg(format!(
            "Invalid address: {}",
            input,
        )))),
    }
}
