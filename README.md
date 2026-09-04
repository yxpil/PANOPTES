# PANOPTES

Screen operation MCP server for AI agents — screenshots plus mouse and keyboard
control, served over JSON-RPC 2.0 on Streamable HTTP.

[![Release](https://img.shields.io/github/v/release/yxpil/PANOPTES?style=flat-square)](https://github.com/yxpil/PANOPTES/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/yxpil/PANOPTES/total?style=flat-square)](https://github.com/yxpil/PANOPTES/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-black?style=flat-square)](./LICENSE)
[![CI](https://github.com/yxpil/PANOPTES/actions/workflows/ci.yml/badge.svg?style=flat-square)](https://github.com/yxpil/PANOPTES/actions/workflows/ci.yml)
[![Platform](https://img.shields.io/badge/platform-windows%20%7C%20macos%20%7C%20linux-black?style=flat-square)](https://github.com/yxpil/PANOPTES)

## About

PANOPTES gives an AI agent **eyes and hands on the machine**: 9 MCP (Model Context
Protocol) tools that capture the screen and drive the mouse and keyboard. Point an MCP
client — [BIT](https://github.com/yxpil/bit), the desktop AI agent hub, or any other
Streamable-HTTP MCP client — at one URL and the agent can:

- **see** — `screenshot` captures a monitor or region to a PNG (optionally embedded as
  an MCP image content item), `screen_info` lists monitors with geometry and scale
- **act** — `mouse_move` `mouse_click` `mouse_drag` `mouse_scroll` `key_type` `key_press`
  drive the cursor, buttons, wheel and keyboard in global coordinates
- **aim** — every screenshot reports the mapping from image pixels to global mouse
  coordinates, so the agent can look at a pixel and click exactly there, even on Retina /
  HiDPI displays where the capture is 2x the logical size

One static binary, no Python, no Node, no browser. Protocol compatibility was verified
against BIT's MCP client (initialize handshake, `Mcp-Session-Id`, `tools/list`,
`tools/call`), and it speaks `2024-11-05`, `2025-03-26` and `2025-06-18` protocol
versions.

### Control authorization (important)

The seven control tools (`mouse_position`, `mouse_move`, `mouse_click`, `mouse_drag`,
`mouse_scroll`, `key_type`, `key_press`) move the cursor, click, type and scroll **on the
machine running the server**. They are locked until the **server operator** acknowledges:

```bash
panoptes serve --yes-i-can-control-this-machine
# or: PANOPTES_I_CAN_CONTROL=yes panoptes serve
```

- Without the acknowledgement, a direct call returns an `isError` result (HTTP 200)
  explaining the flag, so the agent can read the reason and ask the operator to unlock.
- `screenshot` and `screen_info` are read-only and always available — the screenshot-only
  mode never needs the Accessibility permission.
- Only run the unlocked server on a machine you are allowed to control.

## Install

Download a prebuilt binary from [Releases](https://github.com/yxpil/PANOPTES/releases/latest):

| Platform | File |
| --- | --- |
| Linux x86_64 | `panoptes-v0.1.0-x86_64-unknown-linux-gnu.tar.gz` |
| macOS Apple Silicon | `panoptes-v0.1.0-aarch64-apple-darwin.tar.gz` |
| Windows x86_64 | `panoptes-v0.1.0-x86_64-pc-windows-msvc.zip` |

```bash
tar xzf panoptes-v0.1.0-aarch64-apple-darwin.tar.gz
sudo mv panoptes /usr/local/bin/   # or anywhere on PATH
panoptes --version
```

Build from source:

```bash
cargo install --git https://github.com/yxpil/PANOPTES
```

## Quick start

```bash
# 1. See what would be exposed (JSON, no server started)
panoptes tools

# 2. Start the MCP server — screenshots only (default 127.0.0.1:8757)
panoptes serve

# 3. Start with the control tools unlocked
panoptes serve --yes-i-can-control-this-machine
```

The endpoint answers on `http://127.0.0.1:8757/` and `/mcp`. Health check:

```bash
curl http://127.0.0.1:8757/health
# {"control_authorized":false,"data_dir":"/home/me/.panoptes","ok":true,"service":"panoptes","tools":9,"version":"0.1.0"}
```

Every tool is also a CLI subcommand (handy for smoke tests); piped stdin JSON is merged
over the CLI args, stdin wins:

```bash
panoptes screenshot                        # capture the primary monitor → PNG path
panoptes screen-info | panoptes tools --pretty
echo '{"amount":5}' | panoptes mouse-scroll --yes-i-can-control-this-machine
```

## MCP protocol

JSON-RPC 2.0 over Streamable HTTP. Responses are `application/json`.

```bash
# 1. initialize
curl -s -D headers.txt -X POST http://127.0.0.1:8757/mcp \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"me","version":"0"}}}'
# serverInfo: {"name":"panoptes","version":"0.1.0"}; capture Mcp-Session-Id from the headers

# 2. notifications/initialized (same session id)
# 3. tools/list — array of {name, description, inputSchema}
# 4. tools/call
curl -s -X POST http://127.0.0.1:8757/mcp -H "Mcp-Session-Id: $SID" \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"screenshot","arguments":{"return_format":"image"}}}'
# {"content":[{"text":"{\"path\":\"…\"}","type":"text"},
#             {"data":"iVBORw0…","mimeType":"image/png","type":"image"}],"isError":false}
```

Tool results are returned as `content: [{type:"text", ...}, …]`. Backend failures and
gate violations come back as `isError: true` results (not transport errors) so the agent
can read the reason and self-correct.

### Tool inventory

| Tool | What it does | Control-gated |
| --- | --- | --- |
| `screenshot` | Capture a monitor/region to a PNG; optional `return_format:"image"` embeds the PNG | — |
| `screen_info` | Monitors: index, name, origin, size, scale factor, primary flag | — |
| `mouse_position` | Current cursor position in global coordinates | yes |
| `mouse_move` | Move the cursor (absolute or relative), returns the post-move position | yes |
| `mouse_click` | Click left/right/middle, 1–3 clicks, optional pre-move to x,y | yes |
| `mouse_drag` | Press at from, interpolate to `to`, release | yes |
| `mouse_scroll` | Wheel clicks, vertical or horizontal, positive = down/right | yes |
| `key_type` | Type literal text into the focused window | yes |
| `key_press` | Press a key or chord (`cmd+c`, `enter`, `shift+tab`, `f5`, …) | yes |

`panoptes tools` prints the exact inventory (with `control_gated` flags) as JSON.

### Coordinate system

Mouse tools take **global logical coordinates**: origin at the primary monitor's
top-left, y growing down, one unit = one point (macOS) / one DIP (Windows) / one pixel
(X11). `screenshot` returns the captured image size, the monitor geometry and the scale
ratio, so a pixel found in the image maps to a cursor position with
`logical = origin + pixel / scale` — the Retina 2x capture lands on the right spot.

## BIT integration

In BIT (desktop AI agent hub), open the MCP servers page and add `http://127.0.0.1:8757`
— or let BIT's discovery find it on that port. After the handshake BIT lists all 9 tools
and the agent can call them like any native tool; view the PNG returned by `screenshot`
with BIT's built-in image viewer. Any MCP client speaking Streamable HTTP works the same
way.

## Platform notes

- **macOS** — grant **Screen Recording** (screenshots) and **Accessibility** (input) to
  the app that launches panoptes: System Settings → Privacy & Security. The enigo handle
  is created lazily on the first input call, so a screenshot-only server runs without the
  Accessibility permission.
- **Linux** — X11 sessions (enigo/xcap do not support Wayland input; Wayland capture may
  work depending on the compositor).
- **Windows** — works out of the box; the first input call may require an interactive
  session.

## Wiki

Full protocol reference, tool schemas and examples: [the wiki](https://github.com/yxpil/PANOPTES/wiki).

## License

Apache-2.0 — see [LICENSE](./LICENSE).
