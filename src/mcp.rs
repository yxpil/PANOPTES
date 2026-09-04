//! MCP (Model Context Protocol) server over Streamable HTTP — hand-rolled
//! JSON-RPC 2.0 on axum, wire-compatible with BIT's MCP client
//! (see github.com/yxpil/bit `src-tauri/src/mcp.rs`).
//!
//! Contract implemented here (verified against BIT's client):
//! - `initialize` → result `{protocolVersion, capabilities:{tools:{listChanged:false}}, serverInfo}`
//!   plus an `Mcp-Session-Id` response header (echoed back by clients).
//! - `notifications/*` (or any id-less message) → HTTP 202, empty body.
//! - `tools/list` → `{tools:[{name, description, inputSchema}]}` (single page).
//! - `tools/call` → `{content:[{type:"text", text:<json string>}, …], isError}` —
//!   tool failures are 200 + `isError:true`, never transport errors. Text is
//!   always non-empty (BIT's client only concatenates text items).
//! - unknown method → JSON-RPC error -32601; `ping` → empty result.
//!
//! Sessions are issued for spec compliance but not tracked server-side: every
//! request is independent (no server-side session state to expire).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::Response;
use serde_json::{json, Value};

use crate::dispatch;
use crate::state::AppState;
use crate::tools::tools;

/// Protocol versions we can speak; we echo the client's choice when possible.
const MCP_VERSIONS: [&str; 3] = ["2024-11-05", "2025-03-26", "2025-06-18"];

static SESSION_SEQ: AtomicU64 = AtomicU64::new(0);

/// Mirror BIT's `gen_mcp_session_id`: monotonic, unique per process.
fn gen_session_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SESSION_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("mcp-{nanos:x}-{:x}-{seq:x}", std::process::id())
}

fn negotiate_version(client: &str) -> &'static str {
    MCP_VERSIONS
        .iter()
        .find(|v| **v == client)
        .copied()
        .unwrap_or(MCP_VERSIONS[MCP_VERSIONS.len() - 1])
}

fn rpc_ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_err(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn json_response(status: StatusCode, body: Option<Value>, session: Option<&str>) -> Response {
    let mut builder = Response::builder().status(status);
    if let Some(sid) = session {
        builder = builder.header("Mcp-Session-Id", sid);
    }
    match body {
        Some(v) => builder
            .header("content-type", "application/json")
            .body(axum::body::Body::from(v.to_string()))
            .expect("static response"),
        None => builder
            .body(axum::body::Body::empty())
            .expect("static response"),
    }
}

/// MCP JSON-RPC entry point, mounted on both `/` (BIT discovery probes the root)
/// and `/mcp` (the canonical Streamable HTTP path).
pub async fn rpc_entry(
    State(state): State<Arc<AppState>>,
    _uri: Uri,
    _headers: HeaderMap,
    body: Bytes,
) -> Response {
    let msg: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return json_response(
                StatusCode::OK,
                Some(rpc_err(Value::Null, -32700, &format!("parse error: {e}"))),
                None,
            );
        }
    };

    let method = msg
        .get("method")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_string();
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let is_notification = msg.get("id").is_none() || method.starts_with("notifications/");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);

    if is_notification {
        return json_response(StatusCode::ACCEPTED, None, None);
    }

    match method.as_str() {
        "initialize" => {
            let client_ver = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let session = gen_session_id();
            let result = json!({
                "protocolVersion": negotiate_version(client_ver),
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": {
                    "name": "panoptes",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            });
            json_response(StatusCode::OK, Some(rpc_ok(id, result)), Some(&session))
        }
        "ping" => json_response(StatusCode::OK, Some(rpc_ok(id, json!({}))), None),
        "tools/list" => {
            let tools: Vec<Value> = state
                .exposed_tools()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                    })
                })
                .collect();
            json_response(
                StatusCode::OK,
                Some(rpc_ok(id, json!({ "tools": tools }))),
                None,
            )
        }
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            if state.find_tool(&name).is_none() {
                return json_response(
                    StatusCode::OK,
                    Some(rpc_err(id, -32602, &format!("tool not found: '{name}'"))),
                    None,
                );
            }
            let outcome = dispatch::call(&state, &name, args).await;
            json_response(
                StatusCode::OK,
                Some(rpc_ok(
                    id,
                    json!({ "content": outcome.content, "isError": outcome.is_error }),
                )),
                None,
            )
        }
        other => json_response(
            StatusCode::OK,
            Some(rpc_err(id, -32601, &format!("method not found: '{other}'"))),
            None,
        ),
    }
}

/// Human-facing count of registered tools (for `/health`).
pub fn registered_tool_count() -> usize {
    tools().len()
}

async fn health(State(state): State<Arc<AppState>>) -> Response {
    json_response(
        StatusCode::OK,
        Some(json!({
            "ok": true,
            "service": "panoptes",
            "version": env!("CARGO_PKG_VERSION"),
            "tools": state.exposed_tools().count(),
            "control_authorized": state.control_authorized,
            "data_dir": state.data_dir.display().to_string(),
        })),
        None,
    )
}

/// Build the HTTP router: MCP JSON-RPC on `/` and `/mcp`, health on `/health`.
pub fn mcp_router(state: AppState) -> axum::Router {
    axum::Router::new()
        .route("/health", axum::routing::get(health))
        .route("/", axum::routing::post(rpc_entry))
        .route("/mcp", axum::routing::post(rpc_entry))
        .with_state(Arc::new(state))
}
