//! Injection / robustness tests for PANOPTES argument handling.
//!
//! PANOPTES is a screen-control tool: `key_type` *types* whatever it is given,
//! `mouse_*` moves the real cursor. There is no web page to XSS into, no
//! database, no shell-out and no filesystem path taken from the request. Its
//! untrusted-input surface is therefore the JSON argument parser in `dispatch`.
//!
//! These tests prove two things against the real `dispatch_sync` funnel (backed
//! by a recording fake `ScreenBackend`, so nothing touches the real screen):
//!   1. hostile *text* payloads are delivered byte-for-byte to the backend as
//!      literal keystrokes — never shell-expanded, never HTML-escaped (there is
//!      nothing to render);
//!   2. hostile / mistyped *control parameters* (wrong JSON types, unknown
//!      buttons / axes / formats, one-sided coordinates) are rejected as clean
//!      `isError` results rather than panicking or being silently coerced.

use std::sync::Mutex;

use panoptes::backend::{ScreenBackend, Shot};
use panoptes::input::Btn;
use serde_json::{json, Value};

/// A fake backend that records the exact arguments handed to it, so a test can
/// assert that hostile strings survive the funnel unchanged.
struct RecBackend {
    typed: Mutex<Vec<String>>,
}

impl RecBackend {
    fn new() -> Self {
        Self {
            typed: Mutex::new(Vec::new()),
        }
    }
}

impl ScreenBackend for RecBackend {
    fn screen_info(&self) -> Result<Value, String> {
        Ok(json!({ "monitors": 1 }))
    }
    fn screenshot(
        &self,
        _monitor: Option<usize>,
        _region: Option<(u32, u32, u32, u32)>,
        _include_image: bool,
    ) -> Result<Shot, String> {
        Ok(Shot {
            json: json!({ "path": "/fake.png" }),
            png_b64: None,
        })
    }
    fn mouse_position(&self) -> Result<Value, String> {
        Ok(json!({ "x": 0, "y": 0 }))
    }
    fn mouse_move(&self, x: i32, y: i32, relative: bool) -> Result<Value, String> {
        Ok(json!({ "x": x, "y": y, "relative": relative }))
    }
    fn mouse_click(
        &self,
        button: Btn,
        clicks: u32,
        _pre_move: Option<(i32, i32)>,
    ) -> Result<Value, String> {
        Ok(json!({ "button": format!("{button:?}").to_lowercase(), "clicks": clicks }))
    }
    fn mouse_drag(
        &self,
        _button: Btn,
        from: (i32, i32),
        to: (i32, i32),
        _duration_ms: u64,
        _steps: u32,
    ) -> Result<Value, String> {
        Ok(json!({ "from": [from.0, from.1], "to": [to.0, to.1] }))
    }
    fn mouse_scroll(&self, amount: i32, horizontal: bool) -> Result<Value, String> {
        Ok(json!({ "amount": amount, "horizontal": horizontal }))
    }
    fn key_type(&self, text: &str) -> Result<Value, String> {
        self.typed.lock().unwrap().push(text.to_string());
        Ok(json!({ "typed_chars": text.chars().count() }))
    }
    fn key_press(&self, combo: &str) -> Result<Value, String> {
        Ok(json!({ "keys": combo }))
    }
}

fn text_of(out: &panoptes::dispatch::CallOutcome) -> &str {
    out.content[0]["text"].as_str().expect("text content")
}

#[test]
fn hostile_text_is_typed_verbatim_not_shell_expanded() {
    let backend = RecBackend::new();
    let payload = "<img src=x onerror=alert(1)>; rm -rf / # $(whoami) `id`";
    let out = panoptes::dispatch::dispatch_sync(
        &backend,
        "key_type",
        json!({ "text": payload }),
    );
    assert!(!out.is_error, "text typing must succeed: {}", text_of(&out));
    // The backend received the exact payload — no expansion, no escaping.
    assert_eq!(
        backend.typed.lock().unwrap().last().map(String::as_str),
        Some(payload)
    );
    // The reported char count matches the payload (multibyte safe).
    assert_eq!(out.payload.unwrap()["typed_chars"], payload.chars().count() as u64);
}

#[test]
fn string_for_numeric_coordinate_is_graceful_error() {
    let backend = RecBackend::new();
    let out = panoptes::dispatch::dispatch_sync(
        &backend,
        "mouse_move",
        json!({ "x": "100", "y": 50 }),
    );
    assert!(out.is_error);
    assert!(
        text_of(&out).contains("invalid arguments"),
        "{}",
        text_of(&out)
    );
}

#[test]
fn click_requires_both_or_neither_coordinates() {
    let backend = RecBackend::new();
    // Only x, no y -> rejected, not silently defaulted.
    let out = panoptes::dispatch::dispatch_sync(
        &backend,
        "mouse_click",
        json!({ "x": 10 }),
    );
    assert!(out.is_error);
    assert!(text_of(&out).contains("both x and y"));
}

#[test]
fn unknown_button_string_is_rejected() {
    let backend = RecBackend::new();
    let out = panoptes::dispatch::dispatch_sync(
        &backend,
        "mouse_click",
        json!({ "x": 1, "y": 2, "button": "triple" }),
    );
    assert!(out.is_error);
    assert!(text_of(&out).contains("unknown button"));
}

#[test]
fn unknown_scroll_axis_is_rejected() {
    let backend = RecBackend::new();
    let out = panoptes::dispatch::dispatch_sync(
        &backend,
        "mouse_scroll",
        json!({ "amount": 3, "axis": "diagonal" }),
    );
    assert!(out.is_error);
    assert!(text_of(&out).contains("unknown axis"));
}

#[test]
fn unknown_screenshot_format_is_rejected() {
    let backend = RecBackend::new();
    let out = panoptes::dispatch::dispatch_sync(
        &backend,
        "screenshot",
        json!({ "return_format": "pdf" }),
    );
    assert!(out.is_error);
    assert!(text_of(&out).contains("unknown return_format"));
}

#[test]
fn unknown_tool_is_clean_error_not_panic() {
    let backend = RecBackend::new();
    let out = panoptes::dispatch::dispatch_sync(&backend, "delete_everything", json!({}));
    assert!(out.is_error);
    assert!(text_of(&out).contains("tool not found"));
}
