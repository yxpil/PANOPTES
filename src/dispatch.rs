//! Single dispatch funnel shared by the MCP server and the CLI: parse
//! arguments → enforce the control gate → run the backend call → build the
//! MCP content array.

use serde::Deserialize;
use serde_json::{json, Value};

use crate::input::Btn;
use crate::state::AppState;

/// What one tool call produced: MCP content items, the error flag and the
/// parsed payload (for the CLI to print).
pub struct CallOutcome {
    pub content: Vec<Value>,
    pub is_error: bool,
    pub payload: Option<Value>,
}

impl CallOutcome {
    fn ok(payload: Value, extra_content: Option<Value>) -> Self {
        let text = payload.to_string();
        let mut content = vec![json!({ "type": "text", "text": text })];
        if let Some(extra) = extra_content {
            content.push(extra);
        }
        Self {
            content,
            is_error: false,
            payload: Some(payload),
        }
    }

    fn err(message: String) -> Self {
        Self {
            content: vec![json!({ "type": "text", "text": message })],
            is_error: true,
            payload: None,
        }
    }
}

#[derive(Deserialize)]
struct ScreenshotArgs {
    #[serde(default)]
    monitor: Option<usize>,
    #[serde(default)]
    region: Option<Region>,
    #[serde(default)]
    return_format: Option<String>,
}

#[derive(Deserialize)]
struct Region {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
struct MoveArgs {
    x: i32,
    y: i32,
    #[serde(default)]
    relative: bool,
}

#[derive(Deserialize)]
struct ClickArgs {
    #[serde(default)]
    button: Option<String>,
    #[serde(default)]
    clicks: Option<u32>,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
}

#[derive(Deserialize)]
struct DragArgs {
    from_x: i32,
    from_y: i32,
    to_x: i32,
    to_y: i32,
    #[serde(default)]
    button: Option<String>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    steps: Option<u32>,
}

#[derive(Deserialize)]
struct ScrollArgs {
    #[serde(default)]
    amount: Option<i32>,
    #[serde(default)]
    axis: Option<String>,
}

#[derive(Deserialize)]
struct KeyTypeArgs {
    text: String,
}

#[derive(Deserialize)]
struct KeyPressArgs {
    keys: String,
}

/// The gate refusal: an `isError` result (HTTP 200), never a transport error,
/// so the model can read the reason and ask the operator to unlock.
fn gate_refusal(tool: &str) -> CallOutcome {
    CallOutcome::err(format!(
        "tool '{tool}' controls this machine and requires the server to be started with \
         --yes-i-can-control-this-machine (or PANOPTES_I_CAN_CONTROL=yes). Input tools move the \
         cursor, click, type and scroll on this machine."
    ))
}

fn parse_button(args: &ClickArgs) -> Result<Btn, String> {
    match &args.button {
        Some(s) => Btn::parse(s),
        None => Ok(Btn::Left),
    }
}

fn parse_button_str(s: Option<&str>) -> Result<Btn, String> {
    match s {
        Some(s) => Btn::parse(s),
        None => Ok(Btn::Left),
    }
}

/// Dispatch one tool call. Runs the (blocking) backend call on a blocking
/// thread with an 8 s cap — BIT's MCP client times out at 10 s.
pub async fn call(state: &AppState, name: &str, args: Value) -> CallOutcome {
    let Some(tool) = state.find_tool(name) else {
        return CallOutcome::err(format!("tool not found: '{name}'"));
    };
    if tool.control_gated && !state.control_authorized {
        return gate_refusal(name);
    }
    let backend = state.backend.clone();
    let name = name.to_string();
    let run = tokio::task::spawn_blocking(move || dispatch_sync(&*backend, &name, args));
    match tokio::time::timeout(std::time::Duration::from_secs(8), run).await {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(e)) => CallOutcome::err(format!("tool task failed: {e}")),
        Err(_) => CallOutcome::err("tool timed out after 8s".into()),
    }
}

/// Same funnel without the async wrapper (CLI path runs inside tokio anyway,
/// but tests and CLI reuse this synchronous core).
pub fn dispatch_sync(
    backend: &dyn crate::backend::ScreenBackend,
    name: &str,
    args: Value,
) -> CallOutcome {
    match name {
        "screenshot" => parse_run::<ScreenshotArgs, _>(args, |a| {
            let include_image = a
                .return_format
                .as_deref()
                .map(|f| f.eq_ignore_ascii_case("image"))
                .unwrap_or(false);
            if let Some(f) = &a.return_format {
                if !f.eq_ignore_ascii_case("image") && !f.eq_ignore_ascii_case("text") {
                    return CallOutcome::err(format!(
                        "unknown return_format '{f}' (expected 'text' or 'image')"
                    ));
                }
            }
            backend
                .screenshot(
                    a.monitor,
                    a.region.map(|r| (r.x, r.y, r.width, r.height)),
                    include_image,
                )
                .map(|shot| {
                    CallOutcome::ok(
                    shot.json,
                    shot.png_b64.map(|data| {
                        json!({ "type": "image", "data": data, "mimeType": "image/png" })
                    }),
                )
                })
                .unwrap_or_else(CallOutcome::err)
        }),
        "screen_info" => run(backend.screen_info()),
        "mouse_position" => run(backend.mouse_position()),
        "mouse_move" => {
            parse_run::<MoveArgs, _>(args, |a| run(backend.mouse_move(a.x, a.y, a.relative)))
        }
        "mouse_click" => parse_run::<ClickArgs, _>(args, |a| {
            let pre_move = match (a.x, a.y) {
                (Some(x), Some(y)) => Some((x, y)),
                (None, None) => None,
                _ => return CallOutcome::err("mouse_click needs both x and y (or neither)".into()),
            };
            match parse_button(&a) {
                Ok(btn) => run(backend.mouse_click(btn, a.clicks.unwrap_or(1), pre_move)),
                Err(e) => CallOutcome::err(e),
            }
        }),
        "mouse_drag" => {
            parse_run::<DragArgs, _>(args, |a| match parse_button_str(a.button.as_deref()) {
                Ok(btn) => run(backend.mouse_drag(
                    btn,
                    (a.from_x, a.from_y),
                    (a.to_x, a.to_y),
                    a.duration_ms.unwrap_or(500),
                    a.steps.unwrap_or(10),
                )),
                Err(e) => CallOutcome::err(e),
            })
        }
        "mouse_scroll" => parse_run::<ScrollArgs, _>(args, |a| {
            let horizontal = match a.axis.as_deref() {
                None | Some("vertical") => false,
                Some("horizontal") => true,
                Some(other) => {
                    return CallOutcome::err(format!(
                        "unknown axis '{other}' (expected 'vertical' or 'horizontal')"
                    ))
                }
            };
            run(backend.mouse_scroll(a.amount.unwrap_or(3), horizontal))
        }),
        "key_type" => parse_run::<KeyTypeArgs, _>(args, |a| run(backend.key_type(&a.text))),
        "key_press" => parse_run::<KeyPressArgs, _>(args, |a| run(backend.key_press(&a.keys))),
        other => CallOutcome::err(format!("tool not found: '{other}'")),
    }
}

fn run(result: Result<Value, String>) -> CallOutcome {
    match result {
        Ok(payload) => CallOutcome::ok(payload, None),
        Err(e) => CallOutcome::err(e),
    }
}

fn parse_run<T: serde::de::DeserializeOwned, F>(args: Value, f: F) -> CallOutcome
where
    F: FnOnce(T) -> CallOutcome,
{
    match serde_json::from_value::<T>(args) {
        Ok(parsed) => f(parsed),
        Err(e) => CallOutcome::err(format!("invalid arguments: {e}")),
    }
}
