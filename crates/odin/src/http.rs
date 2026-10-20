use crate::mcp::OdinMcp;
use anyhow::{Result, bail};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use odin_engine::Engine;
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc, time::Duration};

#[derive(Clone)]
struct Auth {
    token: Arc<String>,
    hosts: Arc<Vec<String>>,
    origins: Arc<Vec<String>>,
}
pub async fn serve(engine: Arc<Engine>, address: SocketAddr, token: String) -> Result<()> {
    if !address.ip().is_loopback() {
        bail!(
            "This release binds only loopback. Remote deployment requires a separately reviewed authenticated gateway."
        );
    }
    if token.len() < 32 || token.len() > 512 || !token.bytes().all(|b| b.is_ascii_graphic()) {
        bail!("HTTP bearer token must contain 32..512 visible ASCII bytes");
    }
    let listener = tokio::net::TcpListener::bind(address).await?;
    let address = listener.local_addr()?;
    let host = address.to_string();
    let localhost = format!("localhost:{}", address.port());
    let auth = Auth {
        token: Arc::new(token),
        hosts: Arc::new(vec![host.clone(), localhost.clone()]),
        origins: Arc::new(vec![
            format!("http://{host}"),
            format!("http://{localhost}"),
        ]),
    };
    let cancel = tokio_util::sync::CancellationToken::new();
    let service_engine = engine.clone();
    let mcp = StreamableHttpService::new(
        move || {
            Ok(OdinMcp {
                engine: service_engine.clone(),
            })
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(cancel.child_token()),
    );
    let projects_engine = engine.clone();
    let audit_engine = engine.clone();
    let status_engine = engine.clone();
    let export_engine = engine.clone();
    let protected = Router::new()
        .nest_service("/mcp", mcp)
        .route(
            "/api/metadata",
            get(|| async { Json(crate::metadata::build_metadata()) }),
        )
        .route(
            "/api/status",
            get(move || {
                let engine = status_engine.clone();
                async move { Json(status_snapshot(&engine).await) }
            }),
        )
        .route(
            "/api/projects",
            get(move || {
                let engine = projects_engine.clone();
                async move { Json(engine.project_summaries()) }
            }),
        )
        .route(
            "/api/audit/{project}",
            get(move |Path(project): Path<String>| {
                let engine = audit_engine.clone();
                async move {
                    match engine.project(&project).and_then(|p| p.store.recent(100)) {
                        Ok(value) => (StatusCode::OK, Json(redact_audit(value))),
                        Err(error) => (
                            StatusCode::BAD_REQUEST,
                            Json(json!({"error":{"code":error.code}})),
                        ),
                    }
                }
            }),
        )
        .route(
            "/api/audit/{project}/export",
            get(move |Path(project): Path<String>| {
                let engine = export_engine.clone();
                async move {
                    match engine.project(&project).and_then(|p| p.store.recent(100)) {
                        Ok(value) => {
                            let records = redact_audit(value)["entries"]
                                .as_array()
                                .cloned()
                                .unwrap_or_default();
                            let body = records
                                .iter()
                                .map(Value::to_string)
                                .collect::<Vec<_>>()
                                .join("\n")
                                + "\n";
                            (
                                StatusCode::OK,
                                [
                                    (header::CONTENT_TYPE, "application/x-ndjson"),
                                    (
                                        header::CONTENT_DISPOSITION,
                                        "attachment; filename=odin-audit.jsonl",
                                    ),
                                ],
                                body,
                            )
                                .into_response()
                        }
                        Err(error) => (
                            StatusCode::BAD_REQUEST,
                            Json(json!({"error":{"code":error.code}})),
                        )
                            .into_response(),
                    }
                }
            }),
        )
        .route_layer(middleware::from_fn_with_state(auth, authorize));
    let router = Router::new()
        .route(
            "/",
            get(|| async {
                let mut response = Html(include_str!("../../../dashboard/index.html")).into_response();
                response.headers_mut().insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
                response.headers_mut().insert(header::X_CONTENT_TYPE_OPTIONS, header::HeaderValue::from_static("nosniff"));
                response.headers_mut().insert(header::REFERRER_POLICY, header::HeaderValue::from_static("no-referrer"));
                response.headers_mut().insert(header::CONTENT_SECURITY_POLICY, header::HeaderValue::from_static("default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"));
                response
            }),
        )
        .merge(protected);
    eprintln!("Odin HTTP on http://{address}; /mcp and /api require the configured bearer token");
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancel.cancel();
        })
        .await?;
    Ok(())
}
async fn status_snapshot(engine: &Arc<Engine>) -> Value {
    let mut projects = Vec::new();
    for (id, project) in &engine.projects {
        let capabilities = engine
            .call(
                id.clone(),
                "get_capabilities".into(),
                json!({"probe":false}),
            )
            .await;
        let (languages, error) = match capabilities {
            Ok(value) => (value["languages"].as_array().map(|items| items.iter().map(|item| json!({"id":item["id"], "configured":item["configured"], "ready":item["ready"]})).collect::<Vec<_>>()).unwrap_or_default(), Value::Null),
            Err(error) => (Vec::new(), json!({"code":error.code})),
        };
        projects.push(json!({"id":id, "allow_write":project.config.allow_write, "allow_shell":project.config.allow_shell,
            "allow_debug":project.config.allow_debug, "languages":languages, "status_error":error,
            "debug_profiles":project.config.debuggers.keys().collect::<Vec<_>>(), "debug_protocol_validation":"NOT_RUN"}));
    }
    json!({"build":crate::metadata::build_metadata(),"projects":projects,"tool_count":engine.tools().len(),
        "model_token_usage":"UNKNOWN","resource_usage":"UNKNOWN","backends_probed":false,"dashboard_read_only":true})
}
fn redact_audit(value: Value) -> Value {
    let entries = value["entries"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let mut output = serde_json::Map::new();
                    for key in [
                        "at_ms",
                        "project",
                        "tool",
                        "ok",
                        "elapsed_ms",
                        "error_code",
                        "trace_id",
                    ] {
                        if let Some(value) = item.get(key) {
                            output.insert(key.into(), value.clone());
                        }
                    }
                    Value::Object(output)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({"entries":entries,"limit":100,"raw_content_included":false})
}
async fn authorize(State(auth): State<Auth>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !auth.hosts.iter().any(|h| h == host) {
        return (StatusCode::FORBIDDEN, "Unexpected Host").into_response();
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        if !origin
            .to_str()
            .ok()
            .is_some_and(|o| auth.origins.iter().any(|a| a == o))
        {
            return (StatusCode::FORBIDDEN, "Cross-origin access denied").into_response();
        }
    }
    let supplied = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !tokens_match(supplied.as_bytes(), auth.token.as_bytes()) {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer realm=\"odin-local\"")],
            "Bearer token required",
        )
            .into_response();
    }
    let (parts, body) = request.into_parts();
    let body = match tokio::time::timeout(Duration::from_secs(10), to_bytes(body, 8 * 1024 * 1024))
        .await
    {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(_)) => return (StatusCode::PAYLOAD_TOO_LARGE, "Request too large").into_response(),
        Err(_) => return (StatusCode::REQUEST_TIMEOUT, "Request body timed out").into_response(),
    };
    let mut response = next.run(Request::from_parts(parts, Body::from(body))).await;
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store"),
    );
    response
}
fn tokens_match(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (x, y) in a.iter().zip(b) {
        difference |= x ^ y;
    }
    difference == 0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tokens_are_compared_exactly() {
        assert!(tokens_match(b"abc", b"abc"));
        assert!(!tokens_match(b"abc", b"abd"));
        assert!(!tokens_match(b"a", b"aa"));
    }
    #[test]
    fn audit_export_uses_a_metadata_allowlist() {
        let clean = redact_audit(
            json!({"entries":[{"tool":"read_file","ok":true,"elapsed_ms":3,"content":"secret", "arguments":{"token":"secret"}, "input_sha256":"fingerprint"}]}),
        );
        assert_eq!(clean["entries"][0]["tool"], "read_file");
        assert!(clean["entries"][0].get("content").is_none());
        assert!(!clean.to_string().contains("secret"));
    }
}
