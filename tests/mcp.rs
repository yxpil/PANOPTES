//! End-to-end MCP protocol tests against the in-process router, backed by a
//! fake `ScreenBackend` — CI never touches the real screen and never injects
//! input.

use std::net::SocketAddr;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use panoptes::backend::ScreenBackend;
use panoptes::coords::MonitorGeom;
use panoptes::input::Btn;
use panoptes::state::AppState;

/// Fixed fake geometry: a Retina primary (1512x982 pts, capture 3024x1964)
/// plus a 1x secondary to the left.
fn fake_monitors() -> Vec<MonitorGeom> {
    vec![
        MonitorGeom {
            index: 0,
            x: 0,
            y: 0,
            width: 1512,
            height: 982,
            scale_factor: 2.0,
            is_primary: true,
        },
        MonitorGeom {
            index: 1,
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
            is_primary: false,
        },
    ]
}

/// Records every call and returns canned, deterministic payloads.
struct FakeBackend {
    calls: Mutex<Vec<String>>,
}

impl FakeBackend {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, what: &str) {
        self.calls.lock().expect("lock").push(what.to_string());
    }
}

/// A tiny deterministic PNG (8x4 blue) as base64 — real bytes so the image
/// content item is plausible.
const FAKE_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAECAIAAABgpJnbAAAAPElEQVR4nGNgYGD4z8DAwMDAwMDwn4GBgYGJgYGBiYGBgYmBgYGJgYmBiYGJgYmBiYGJgYmBiQEAAAP//AAwAAQ==";

impl ScreenBackend for FakeBackend {
    fn screen_info(&self) -> Result<Value, String> {
        self.record("screen_info");
        Ok(json!({ "monitors": fake_monitors().len() }))
    }

    fn screenshot(
        &self,
        monitor: Option<usize>,
        region: Option<(u32, u32, u32, u32)>,
        include_image: bool,
    ) -> Result<panoptes::backend::Shot, String> {
        self.record(format!("screenshot {monitor:?} {region:?}").as_str());
        if monitor == Some(99) {
            return Err("monitor 99 does not exist".into());
        }
        let json = json!({
            "path": "/tmp/fake-shot.png",
            "image_px": { "width": 3024, "height": 1964 },
            "logical": { "width": 1512, "height": 982 },
            "scale_factor": 2.0,
            "monitor": { "index": 0, "name": "Fake", "origin": [0, 0], "is_primary": true },
            "region_px": null,
            "pixel_to_logical": "mouse tools take global coordinates",
        });
        Ok(panoptes::backend::Shot {
            json,
            png_b64: include_image.then(|| FAKE_PNG_B64.to_string()),
        })
    }

    fn mouse_position(&self) -> Result<Value, String> {
        self.record("mouse_position");
        Ok(json!({ "x": 100, "y": 50 }))
    }

    fn mouse_move(&self, x: i32, y: i32, relative: bool) -> Result<Value, String> {
        self.record(format!("mouse_move {x} {y} {relative}").as_str());
        Ok(json!({ "x": x, "y": y }))
    }

    fn mouse_click(
        &self,
        button: Btn,
        clicks: u32,
        pre_move: Option<(i32, i32)>,
    ) -> Result<Value, String> {
        self.record(format!("mouse_click {button:?} {clicks} {pre_move:?}").as_str());
        Ok(json!({ "button": format!("{button:?}").to_lowercase(), "clicks": clicks }))
    }

    fn mouse_drag(
        &self,
        button: Btn,
        from: (i32, i32),
        to: (i32, i32),
        duration_ms: u64,
        steps: u32,
    ) -> Result<Value, String> {
        self.record(
            format!("mouse_drag {button:?} {from:?} {to:?} {duration_ms} {steps}").as_str(),
        );
        Ok(json!({ "from": [from.0, from.1], "to": [to.0, to.1] }))
    }

    fn mouse_scroll(&self, amount: i32, horizontal: bool) -> Result<Value, String> {
        self.record(format!("mouse_scroll {amount} {horizontal}").as_str());
        Ok(json!({ "amount": amount }))
    }

    fn key_type(&self, text: &str) -> Result<Value, String> {
        self.record("key_type");
        if text == "__fail" {
            return Err("typing failed: fake".into());
        }
        Ok(json!({ "typed_chars": text.chars().count() }))
    }

    fn key_press(&self, combo: &str) -> Result<Value, String> {
        self.record(format!("key_press {combo}").as_str());
        Ok(json!({ "keys": combo }))
    }
}

fn spawn_server(control_authorized: bool) -> SocketAddr {
    let state = AppState {
        control_authorized,
        data_dir: std::env::temp_dir().join("panoptes-test"),
        backend: Arc::new(FakeBackend::new()),
    };
    let (tx, rx) = mpsc::channel::<SocketAddr>();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind");
            let addr = listener.local_addr().expect("addr");
            tx.send(addr).expect("send addr");
            axum::serve(listener, panoptes::mcp::mcp_router(state))
                .await
                .expect("serve");
        });
    });
    rx.recv().expect("server address")
}

/// POST one JSON-RPC message; returns (status, Mcp-Session-Id, parsed body).
fn rpc(base: &str, path: &str, body: Value, session: Option<&str>) -> (u16, Option<String>, Value) {
    let mut req = ureq::post(&format!("{base}{path}"));
    if let Some(sid) = session {
        req = req.set("Mcp-Session-Id", sid);
    }
    let resp = req.send_json(body).expect("request");
    let status = resp.status();
    let session = resp.header("mcp-session-id").map(|s| s.to_string());
    let body = resp.into_json().expect("json body");
    (status, session, body)
}

fn initialize(base: &str, path: &str) -> String {
    let (status, session, body) = rpc(
        base,
        path,
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "BIT", "version": "0"}}
        }),
        None,
    );
    assert_eq!(status, 200);
    let sid = session.expect("initialize must return a session id");
    assert!(sid.starts_with("mcp-"), "session id: {sid}");
    assert_eq!(body["result"]["serverInfo"]["name"], "panoptes");
    assert_eq!(body["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(
        body["result"]["capabilities"]["tools"]["listChanged"],
        false
    );
    sid
}

#[test]
fn full_flow_initialize_tools_list_and_screenshot() {
    let addr = spawn_server(false);
    let base = format!("http://{addr}");

    // 1. Handshake on the root path (BIT discovery probes the root).
    let sid = initialize(&base, "/");

    // 2. initialized notification → 202, empty body.
    let resp = ureq::post(&format!("{base}/mcp"))
        .set("Mcp-Session-Id", &sid)
        .send_json(json!({"jsonrpc": "2.0", "id": 99, "method": "notifications/initialized"}))
        .expect("notification");
    assert_eq!(resp.status(), 202);
    assert!(resp.into_string().unwrap_or_default().is_empty());

    // 3. tools/list → all 9 tools (gated ones stay listed).
    let (status, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}}),
        Some(&sid),
    );
    assert_eq!(status, 200);
    let tools = body["result"]["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 9, "9 tools: {tools:?}");
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "screenshot",
        "screen_info",
        "mouse_position",
        "mouse_move",
        "mouse_click",
        "mouse_drag",
        "mouse_scroll",
        "key_type",
        "key_press",
    ] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }
    for tool in tools {
        assert!(tool["inputSchema"]["type"] == "object");
    }
    let shot = tools.iter().find(|t| t["name"] == "screenshot").unwrap();
    assert_eq!(
        shot["inputSchema"]["properties"]["monitor"]["type"],
        "integer"
    );

    // 4. tools/call screenshot → text JSON parses; no image item by default.
    let (status, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
               "params": {"name": "screenshot", "arguments": {"monitor": 0}}}),
        Some(&sid),
    );
    assert_eq!(status, 200);
    assert_eq!(body["result"]["isError"], false);
    let content = body["result"]["content"].as_array().expect("content");
    assert_eq!(content.len(), 1, "default: text only");
    let payload: Value =
        serde_json::from_str(content[0]["text"].as_str().expect("text")).expect("json text");
    assert_eq!(payload["path"], "/tmp/fake-shot.png");
    assert_eq!(payload["scale_factor"], 2.0);

    // 5. tools/call screenshot return_format=image → text + image items.
    let (_, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
               "params": {"name": "screenshot",
                          "arguments": {"return_format": "image"}}}),
        Some(&sid),
    );
    assert_eq!(body["result"]["isError"], false);
    let content = body["result"]["content"].as_array().expect("content");
    assert_eq!(content.len(), 2, "text + image");
    assert_eq!(content[0]["type"], "text");
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
    assert!(!content[1]["data"].as_str().unwrap().is_empty());
}

#[test]
fn control_gate_locked_and_unlocked() {
    // Locked: gated tool refuses with an isError result (HTTP still 200).
    let locked = spawn_server(false);
    let base = format!("http://{locked}");
    let sid = initialize(&base, "/mcp");
    let (status, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
               "params": {"name": "mouse_click", "arguments": {"x": 1, "y": 2}}}),
        Some(&sid),
    );
    assert_eq!(status, 200);
    assert_eq!(body["result"]["isError"], true);
    let text = body["result"]["content"][0]["text"]
        .as_str()
        .expect("error text");
    assert!(
        text.contains("--yes-i-can-control-this-machine"),
        "text: {text}"
    );
    assert!(text.contains("PANOPTES_I_CAN_CONTROL=yes"), "text: {text}");

    // Screenshot is read-only: works on a locked server.
    let (_, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call",
               "params": {"name": "screen_info", "arguments": {}}}),
        Some(&sid),
    );
    assert_eq!(body["result"]["isError"], false);

    // Unlocked: the click goes through.
    let unlocked = spawn_server(true);
    let base = format!("http://{unlocked}");
    let sid = initialize(&base, "/mcp");
    let (_, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call",
               "params": {"name": "mouse_click", "arguments": {"button": "right", "clicks": 2}}}),
        Some(&sid),
    );
    assert_eq!(body["result"]["isError"], false);
    let payload: Value =
        serde_json::from_str(body["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["button"], "right");
    assert_eq!(payload["clicks"], 2);
}

#[test]
fn backend_failure_becomes_is_error_result() {
    let addr = spawn_server(false);
    let base = format!("http://{addr}");
    let sid = initialize(&base, "/mcp");
    let (status, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call",
               "params": {"name": "screenshot", "arguments": {"monitor": 99}}}),
        Some(&sid),
    );
    assert_eq!(status, 200, "tool failures must stay HTTP 200");
    assert_eq!(body["result"]["isError"], true);
    let text = body["result"]["content"][0]["text"]
        .as_str()
        .expect("error text");
    assert!(text.contains("monitor 99"), "text: {text}");
}

#[test]
fn protocol_errors_ping_and_notifications() {
    // Unlocked: this test exercises argument parsing, not the control gate.
    let addr = spawn_server(true);
    let base = format!("http://{addr}");
    let sid = initialize(&base, "/");

    // ping → empty result object.
    let (status, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 8, "method": "ping", "params": {}}),
        Some(&sid),
    );
    assert_eq!(status, 200);
    assert_eq!(body["result"], json!({}));

    // Unknown tool → -32602.
    let (_, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": "nope"}}),
        Some(&sid),
    );
    assert_eq!(body["error"]["code"], -32602);

    // Invalid arguments → isError with the serde message.
    let (_, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 10, "method": "tools/call",
               "params": {"name": "key_type", "arguments": {"wrong": 1}}}),
        Some(&sid),
    );
    assert_eq!(body["result"]["isError"], true);
    assert!(
        body["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("invalid arguments"),
        "text: {body:?}"
    );

    // Unknown method → -32601.
    let (_, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 11, "method": "bogus/x"}),
        Some(&sid),
    );
    assert_eq!(body["error"]["code"], -32601);

    // Malformed JSON body → -32700.
    let resp = ureq::post(&format!("{base}/mcp"))
        .set("content-type", "application/json")
        .send_string("{not json")
        .expect("request");
    let body: Value = resp.into_json().expect("json");
    assert_eq!(body["error"]["code"], -32700);

    // Version negotiation: unknown client version falls back to the latest.
    let (_, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 12, "method": "initialize",
               "params": {"protocolVersion": "1999-01-01", "capabilities": {}, "clientInfo": {"name": "x"}}}),
        None,
    );
    assert_eq!(body["result"]["protocolVersion"], "2025-06-18");

    // BIT sends 2024-11-05 → echoed back.
    let (_, _, body) = rpc(
        &base,
        "/mcp",
        json!({"jsonrpc": "2.0", "id": 13, "method": "initialize",
               "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "x"}}}),
        None,
    );
    assert_eq!(body["result"]["protocolVersion"], "2024-11-05");
}

#[test]
fn health_reports_authorization_state() {
    let addr = spawn_server(true);
    let body: Value = ureq::get(&format!("http://{addr}/health"))
        .call()
        .expect("health")
        .into_json()
        .expect("json");
    assert_eq!(body["ok"], true);
    assert_eq!(body["service"], "panoptes");
    assert_eq!(body["tools"], 9);
    assert_eq!(body["control_authorized"], true);
}
