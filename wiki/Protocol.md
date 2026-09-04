# PANOPTES MCP Protocol / PANOPTES MCP 协议

PANOPTES implements MCP (Model Context Protocol) over **Streamable HTTP**: JSON-RPC 2.0 requests POSTed to `/` or `/mcp`, responses as `application/json`. Session handling follows the MCP spec via the `Mcp-Session-Id` header. Protocol versions negotiated at `initialize`: `2024-11-05`, `2025-03-26`, `2025-06-18`.

PANOPTES 通过 **Streamable HTTP** 实现 MCP（Model Context Protocol）：JSON-RPC 2.0 请求 POST 到 `/` 或 `/mcp`，响应为 `application/json`。会话由 `Mcp-Session-Id` 头管理，遵循 MCP 规范。`initialize` 时协商协议版本：`2024-11-05`、`2025-03-26`、`2025-06-18`。

## Endpoints / 端点

| Endpoint / 端点 | Method / 方法 | Auth / 认证 | Description / 说明 |
| --- | --- | --- | --- |
| `/health` | GET | none / 无 | `{"control_authorized":false,"data_dir":"…","ok":true,"service":"panoptes","tools":9,"version":"0.1.0"}` |
| `/` and `/mcp` | POST | none (loopback bind / 仅回环绑定) | JSON-RPC 2.0 entry / JSON-RPC 2.0 入口 |

## Handshake / 握手

```json
// 1. request
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{
  "protocolVersion":"2025-03-26","capabilities":{},
  "clientInfo":{"name":"bit","version":"1.0"}}}

// 2. response — echo of the negotiated version + Mcp-Session-Id: mcp-<hex> header
{"jsonrpc":"2.0","id":1,"result":{
  "protocolVersion":"2025-03-26",
  "capabilities":{"tools":{"listChanged":false}},
  "serverInfo":{"name":"panoptes","version":"0.1.0"}}}

// 3. notification (same session id) → 202 Accepted, empty body
{"jsonrpc":"2.0","method":"notifications/initialized"}
```

An unknown client version falls back to the latest server version (`2025-06-18`). / 未知客户端版本回退到服务器最新版本（`2025-06-18`）。

## Methods / 方法

| Method / 方法 | Params / 参数 | Result / 结果 |
| --- | --- | --- |
| `initialize` | `protocolVersion`, `capabilities`, `clientInfo` | `protocolVersion`, `capabilities`, `serverInfo` |
| `notifications/initialized` | — | `202 Accepted`, empty body / 空体 |
| `ping` | — | `{}` |
| `tools/list` | — | `{"tools":[{name, description, inputSchema}, …]}` |
| `tools/call` | `name`, `arguments` | `{"content":[{"type":"text","text":"…"}, …],"isError":false}` |

## Error codes / 错误码

| Code / 码 | Meaning / 含义 |
| --- | --- |
| `-32700` | body is not valid JSON / 请求体非法 JSON |
| `-32602` | `tools/call` with an unknown tool name / 未知工具名 |
| `-32601` | unknown method / 未知方法 |

Everything else — backend failures, gate refusals, invalid arguments — comes back as **HTTP 200 with `isError: true`** in the result, so the model can read the reason and self-correct. / 其余一切——后端失败、门禁拒绝、参数非法——均以 **HTTP 200 + `isError: true`** 返回，模型可读取原因并自我修正。

## Control gate / 控制门禁

The 7 control tools refuse to run unless the server was started with `--yes-i-can-control-this-machine` (or `PANOPTES_I_CAN_CONTROL=yes`). The refusal is an `isError` result that names both switches:

除非服务器以 `--yes-i-can-control-this-machine`（或 `PANOPTES_I_CAN_CONTROL=yes`）启动，否则 7 个控制工具拒绝运行。拒绝信息是一个 `isError` 结果，注明两种授权方式：

```json
{"content":[{"text":"tool 'mouse_click' controls this machine and requires the server to be \
started with --yes-i-can-control-this-machine (or PANOPTES_I_CAN_CONTROL=yes). …","type":"text"}],
 "isError":true}
```

`screenshot` / `screen_info` are read-only and never gated. / `screenshot` / `screen_info` 只读，永不受门控。

## Tool reference / 工具参考

### screenshot

Capture the primary monitor by default; `monitor` selects a display, `region` crops in captured-image pixels. `return_format: "image"` additionally embeds the PNG as an MCP image content item.

默认截取主显示器；`monitor` 选择显示器，`region` 以截图像素坐标裁剪。`return_format: "image"` 会把 PNG 作为 MCP 图像内容项内嵌返回。

```json
// arguments / 参数
{"monitor": 0, "region": {"x": 100, "y": 80, "width": 400, "height": 300}, "return_format": "text"}

// result text (parsed) / 结果文本（解析后）
{"path":"/Users/me/.panoptes/shots/shot-1788532001634416000.png",
 "image_px":{"width":2940,"height":1912},
 "logical":{"width":1470,"height":956},
 "scale_factor":2.0,
 "monitor":{"index":0,"name":"Display #41059","origin":[0,0],"is_primary":true},
 "region_px":null,
 "pixel_to_logical":"mouse tools take global coordinates: logical = monitor.origin + (region.origin + pixel) / scale_factor"}
```

### screen_info

```json
{"monitors":[{"index":0,"name":"Display #41059","origin":[0,0],"width":1470,"height":956,
              "scale_factor":2.0,"is_primary":true},
             {"index":1,"name":"Display #41060","origin":[-1920,0],"width":1920,"height":1080,
              "scale_factor":1.0,"is_primary":false}],
 "coordinate_note":"screenshot pixels are (monitor size * scale_factor); mouse tools take global logical coordinates"}
```

### mouse_position

```json
// result / 结果
{"x":6,"y":-920,"note":"global coordinates — exactly what mouse_move / mouse_click expect"}
```

Negative coordinates mean the cursor is on a monitor left of / above the primary. / 负坐标表示光标位于主显示器左侧/上方的显示器上。

### mouse_move

```json
// arguments / 参数
{"x": 200, "y": 300, "relative": false}
// result / 结果
{"x":200,"y":300}
```

### mouse_click

```json
// arguments / 参数
{"button": "left", "clicks": 2, "x": 200, "y": 300}
// result / 结果
{"button":"left","clicks":2,"position":[200,300]}
```

`button`: `left` (default) / `right` / `middle`; `clicks`: 1–3 (a real OS multi-click); `x`+`y`: optional pre-move (both or neither). / `button`：`left`（默认）/ `right` / `middle`；`clicks`：1–3（真实系统多击）；`x`+`y`：可选点击前移动（成对出现或不传）。

### mouse_drag

```json
// arguments / 参数
{"from_x": 100, "from_y": 100, "to_x": 500, "to_y": 400, "button": "left", "duration_ms": 500, "steps": 10}
// result / 结果
{"from":[100,100],"to":[500,400],"duration_ms":500,"steps":10,"end_position":[500,400]}
```

Press at `from`, interpolate to `to` over `duration_ms` (1–5000), release. `steps` is clamped to 1–100. Useful for selections, sliders and window moves. / 在 `from` 按下，`duration_ms`（1–5000）内插值移动到 `to`，然后释放。`steps` 限 1–100。适用于选择文本、拖动滑块与移动窗口。

### mouse_scroll

```json
// arguments / 参数
{"amount": 3, "axis": "vertical"}
// result / 结果
{"amount":3,"axis":"vertical"}
```

`amount` in wheel clicks, may be negative; positive = down (vertical) / right (horizontal). / `amount` 为滚轮格数，可为负；正值 = 向下（垂直）/ 向右（水平）。

### key_type

```json
// arguments / 参数
{"text": "Hello, 世界"}
// result / 结果
{"typed_chars":9}
```

Types literal text (Unicode) into the focused window. / 向焦点窗口输入字面文本（Unicode）。

### key_press

```json
// arguments / 参数
{"keys": "cmd+shift+t"}
// result / 结果
{"keys":"cmd+shift+t"}
```

Modifier aliases: `cmd`/`command`/`meta`/`win`/`super`, `ctrl`/`control`, `alt`/`option`/`opt`, `shift`. Named keys: `enter`/`return`, `tab`, `esc`/`escape`, `space`, `backspace`, `delete`/`del`, `home`, `end`, `pageup`, `pagedown`, `up`/`down`/`left`/`right` (also `arrowup`…), `f1`–`f12`. Single characters type as themselves (`a`, `1`, `-`). Modifiers are pressed first and released in reverse order; a bare modifier (`"keys":"cmd"`) is clicked. / 修饰键别名如上；命名键如上；单字符直接输入。修饰键先按下、逆序释放；裸修饰键（`"keys":"cmd"`）按单击处理。

## Coordinate mapping / 坐标换算

Mouse tools take **global logical coordinates**. For a pixel `(px, py)` found in a screenshot:

鼠标工具使用**全局逻辑坐标**。截图中发现的像素 `(px, py)` 换算如下：

```
logical = monitor.origin + (region.origin + (px, py)) / scale_factor
```

- `monitor.origin` — the `[x, y]` the `screenshot` result reports / `screenshot` 结果中的 `[x, y]`
- `region.origin` — the clamped `region_px` `{x, y}` when a region was cropped, else `[0, 0]` / 裁剪时为钳制后的 `region_px` `{x, y}`，否则 `[0, 0]`
- `scale_factor` — derived from the captured image size vs the monitor's logical size (self-calibrating, so a Retina 2x capture maps correctly even when the OS scale factor is misreported) / 由截图像素尺寸与显示器逻辑尺寸推导（自校准，即使系统缩放系数误报，Retina 2x 截图也能正确映射）

Worked example / 完整示例: Retina primary `logical 1470×956`, capture `2940×1912` → `scale_factor 2.0`. Pixel `(1200, 600)` → cursor `(600, 300)`.

## CLI mirror / CLI 镜像

Every tool is a CLI subcommand running the same dispatch funnel (parse → gate → backend). Piped stdin JSON merges over CLI args, stdin wins. / 每个工具都是 CLI 子命令，走同一分发管线（解析 → 门禁 → 后端）。管道 stdin JSON 与 CLI 参数合并，stdin 优先。

```bash
panoptes screenshot --monitor 0 --with-image --pretty
echo '{"keys":"cmd+c"}' | panoptes key-press --yes-i-can-control-this-machine
panoptes tools          # live inventory with control_gated flags, as JSON
```

Tool failures exit with code 2 and the reason on stderr. / 工具失败以退出码 2 结束，原因输出到 stderr。
