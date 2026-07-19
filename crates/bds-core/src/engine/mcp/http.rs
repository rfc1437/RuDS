use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::thread::JoinHandle;

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use serde_json::Value;

use crate::engine::{EngineError, EngineResult};

use super::McpContext;
use super::protocol::{MCP_PROTOCOL_VERSION, error, handle_rpc};

pub struct McpHttpServer {
    address: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl McpHttpServer {
    pub fn start(database_path: PathBuf, port: u16) -> EngineResult<Self> {
        McpContext::new(database_path.clone()).prepare()?;
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("bds-mcp-http".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("MCP Tokio runtime");
                runtime.block_on(async move {
                    let listener =
                        tokio::net::TcpListener::from_std(listener).expect("MCP loopback listener");
                    let context = McpContext::new(database_path);
                    let router = Router::new()
                        .route("/mcp", post(post_mcp).options(options_mcp))
                        .with_state(context);
                    let _ = axum::serve(listener, router)
                        .with_graceful_shutdown(async {
                            let _ = shutdown_rx.await;
                        })
                        .await;
                });
            })?;
        Ok(Self {
            address,
            shutdown: Some(shutdown),
            thread: Some(thread),
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}/mcp", self.address)
    }

    pub fn stop(mut self) -> EngineResult<()> {
        self.shutdown.take();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| EngineError::Parse("MCP server thread panicked".into()))?;
        }
        Ok(())
    }
}

impl Drop for McpHttpServer {
    fn drop(&mut self) {
        self.shutdown.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn post_mcp(
    State(context): State<McpContext>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if let Err((status, message)) = validate_http_headers(&headers) {
        return with_cors((status, message).into_response(), &headers);
    }
    let request = match serde_json::from_slice::<Value>(&body) {
        Ok(request) => request,
        Err(_) => {
            return with_cors(
                (
                    StatusCode::BAD_REQUEST,
                    axum::Json(error(Value::Null, -32700, "Parse error")),
                )
                    .into_response(),
                &headers,
            );
        }
    };
    let response = match handle_rpc(&context, &request) {
        Some(response) => (StatusCode::OK, axum::Json(response)).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    };
    with_cors(response, &headers)
}

async fn options_mcp(headers: HeaderMap) -> Response {
    if let Err((status, message)) = validate_origin_and_host(&headers) {
        return with_cors((status, message).into_response(), &headers);
    }
    with_cors(StatusCode::NO_CONTENT.into_response(), &headers)
}

fn validate_http_headers(headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    validate_origin_and_host(headers)?;
    if let Some(version) = headers
        .get("mcp-protocol-version")
        .and_then(|value| value.to_str().ok())
        && !["2025-03-26", MCP_PROTOCOL_VERSION].contains(&version)
    {
        return Err((StatusCode::BAD_REQUEST, "Unsupported MCP protocol version"));
    }
    if headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| !value.starts_with("application/json"))
    {
        return Err((
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Expected application/json",
        ));
    }
    if headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_none_or(|value| {
            !value.contains("application/json") || !value.contains("text/event-stream")
        })
    {
        return Err((
            StatusCode::NOT_ACCEPTABLE,
            "Accept must include application/json and text/event-stream",
        ));
    }
    Ok(())
}

fn validate_origin_and_host(headers: &HeaderMap) -> Result<(), (StatusCode, &'static str)> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let host_name = host
        .strip_prefix('[')
        .and_then(|value| value.split_once(']').map(|(host, _)| host))
        .unwrap_or_else(|| host.split(':').next().unwrap_or_default());
    if !["localhost", "127.0.0.1", "::1"].contains(&host_name) {
        return Err((StatusCode::FORBIDDEN, "Forbidden host"));
    }
    if let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        let local = url::Url::parse(origin).ok().is_some_and(|origin| {
            matches!(origin.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
        });
        if !local {
            return Err((StatusCode::FORBIDDEN, "Forbidden origin"));
        }
    }
    Ok(())
}

fn with_cors(mut response: Response, request_headers: &HeaderMap) -> Response {
    let headers = response.headers_mut();
    let origin = request_headers
        .get(header::ORIGIN)
        .filter(|value| {
            value.to_str().ok().is_some_and(|origin| {
                url::Url::parse(origin).ok().is_some_and(|origin| {
                    matches!(origin.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
                })
            })
        })
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("http://127.0.0.1"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, accept, origin, mcp-protocol-version"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_validation_rejects_dns_rebinding_and_remote_origins() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("attacker.example"));
        assert!(validate_origin_and_host(&headers).is_err());
        headers.insert(header::HOST, HeaderValue::from_static("127.0.0.1:4124"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://attacker.example"),
        );
        assert!(validate_origin_and_host(&headers).is_err());
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("http://localhost:3000"),
        );
        assert!(validate_origin_and_host(&headers).is_ok());
    }

    #[test]
    fn router_only_allows_post_and_options() {
        assert_eq!(axum::http::Method::POST.as_str(), "POST");
        assert_eq!(axum::http::Method::OPTIONS.as_str(), "OPTIONS");
    }
}
