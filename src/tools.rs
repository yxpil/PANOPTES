//! Static registry of PANOPTES tools. Every tool executes in-process against
//! a [`crate::backend::ScreenBackend`] (no child satellites, unlike SECFORGE).

use serde_json::{json, Value};

/// One PANOPTES capability exposed over MCP and mirrored as a CLI subcommand.
#[derive(Debug, Clone)]
pub struct ToolDef {
    /// MCP tool name.
    pub name: &'static str,
    /// English description surfaced in `tools/list`.
    pub description: &'static str,
    /// JSON Schema for the `arguments` object.
    pub input_schema: Value,
    /// Requires the server to be started with `--yes-i-can-control-this-machine`.
    pub control_gated: bool,
}

/// Build a JSON-Schema object from `(name, type, description, required)` tuples.
pub fn schema(fields: &[(&str, &str, &str, bool)]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, ty, desc, req) in fields {
        properties.insert(
            (*name).to_string(),
            json!({ "type": ty, "description": desc }),
        );
        if *req {
            required.push(json!(name));
        }
    }
    let mut schema = json!({ "type": "object", "properties": properties });
    if !required.is_empty() {
        schema["required"] = Value::Array(required);
    }
    schema
}

/// All tools panoptes can expose, in stable order. Gated tools are always
/// *listed* — the gate is enforced at call time so the model learns how to
/// request authorization from the refusal text.
pub fn tools() -> &'static [ToolDef] {
    static TOOLS: std::sync::OnceLock<Vec<ToolDef>> = std::sync::OnceLock::new();
    TOOLS.get_or_init(build_tools)
}

fn build_tools() -> Vec<ToolDef> {
    vec![
        // ── observation (read-only, never gated) ────────────────────────────
        ToolDef {
            name: "screenshot",
            description: "Capture the screen (or one monitor, or a region) to a PNG file and return \
                          its path plus the mapping from image pixels to mouse coordinates. View the \
                          file with the built-in view_image tool to actually see it.",
            input_schema: schema(&[
                ("monitor", "integer", "0-based monitor index (default: primary monitor)", false),
                ("region", "object", "Crop rect {x, y, width, height} in captured-image pixels", false),
                ("return_format", "string", "'text' (default) returns the path only; 'image' also embeds the PNG as an MCP image content item", false),
            ]),
            control_gated: false,
        },
        ToolDef {
            name: "screen_info",
            description: "List monitors: index, name, global origin, size, scale factor, primary flag.",
            input_schema: schema(&[]),
            control_gated: false,
        },
        // ── control (gated: moves the cursor / clicks / types) ──────────────
        ToolDef {
            name: "mouse_position",
            description: "Current cursor position in global coordinates (what mouse_move expects).",
            input_schema: schema(&[]),
            control_gated: true,
        },
        ToolDef {
            name: "mouse_move",
            description: "Move the cursor to global coordinates (x, y) — the coordinate space \
                          returned by mouse_position and the one screenshot's pixel_to_logical \
                          formula maps onto. Returns the post-move position for verification.",
            input_schema: schema(&[
                ("x", "integer", "Target x in global coordinates", true),
                ("y", "integer", "Target y in global coordinates", true),
                ("relative", "boolean", "Move relative to the current position instead (default false)", false),
            ]),
            control_gated: true,
        },
        ToolDef {
            name: "mouse_click",
            description: "Click a mouse button (optionally moving to x, y first). clicks=2 sends a \
                          double click; clicks=3 a triple click.",
            input_schema: schema(&[
                ("button", "string", "'left' (default) | 'right' | 'middle'", false),
                ("clicks", "integer", "1 (default) | 2 | 3", false),
                ("x", "integer", "Optional: move to this global x before clicking", false),
                ("y", "integer", "Optional: move to this global y before clicking", false),
            ]),
            control_gated: true,
        },
        ToolDef {
            name: "mouse_drag",
            description: "Press a button at 'from', interpolate the cursor to 'to' over duration_ms, \
                          then release. Useful for selections, sliders and window moves.",
            input_schema: schema(&[
                ("from_x", "integer", "Start x in global coordinates", true),
                ("from_y", "integer", "Start y in global coordinates", true),
                ("to_x", "integer", "End x in global coordinates", true),
                ("to_y", "integer", "End y in global coordinates", true),
                ("button", "string", "'left' (default) | 'right' | 'middle'", false),
                ("duration_ms", "integer", "Total drag time in ms, 1-5000 (default 500)", false),
                ("steps", "integer", "Interpolation steps, 1-100 (default 10)", false),
            ]),
            control_gated: true,
        },
        ToolDef {
            name: "mouse_scroll",
            description: "Scroll the wheel under the cursor. amount is in wheel clicks; positive \
                          scrolls down (vertical) or right (horizontal) as produced by a normal \
                          wheel, negative the other way.",
            input_schema: schema(&[
                ("amount", "integer", "Wheel clicks, may be negative (default 3)", false),
                ("axis", "string", "'vertical' (default) | 'horizontal'", false),
            ]),
            control_gated: true,
        },
        ToolDef {
            name: "key_type",
            description: "Type literal text into the focused window (Unicode, as if pasted via the \
                          keyboard).",
            input_schema: schema(&[("text", "string", "Text to type", true)]),
            control_gated: true,
        },
        ToolDef {
            name: "key_press",
            description: "Press one key or a chord like 'cmd+shift+t'. Modifier aliases: cmd/meta/\
                          win, ctrl/control, alt/option, shift. Named keys: enter/return, tab, esc, \
                          space, up/down/left/right, f1-f12, home, end, pageup, pagedown, delete, \
                          backspace.",
            input_schema: schema(&[("keys", "string", "Key or '+'-separated chord, e.g. 'cmd+c'", true)]),
            control_gated: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_shape_and_gating() {
        let tools = tools();
        assert_eq!(tools.len(), 9);
        let gated: Vec<&str> = tools
            .iter()
            .filter(|t| t.control_gated)
            .map(|t| t.name)
            .collect();
        assert_eq!(
            gated,
            vec![
                "mouse_position",
                "mouse_move",
                "mouse_click",
                "mouse_drag",
                "mouse_scroll",
                "key_type",
                "key_press",
            ]
        );
        for t in tools {
            assert_eq!(t.input_schema["type"], "object");
            assert!(!t.description.is_empty());
        }
    }

    #[test]
    fn schema_marks_required_fields() {
        let s = schema(&[("a", "string", "A", true), ("b", "integer", "B", false)]);
        assert_eq!(s["required"], json!(["a"]));
        assert_eq!(s["properties"]["b"]["type"], "integer");
    }
}
