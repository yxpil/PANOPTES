# PANOPTES Wiki / PANOPTES 维基

**PANOPTES** — Screen operation MCP server for AI agents around the [BIT](https://github.com/yxpil/bit) ecosystem: **9 MCP tools** that capture the screen and drive the mouse and keyboard, served over JSON-RPC 2.0 on Streamable HTTP. One URL gives an agent eyes (screenshots) and hands (cursor, buttons, wheel, keyboard) on the machine — with pixel-to-cursor coordinate mapping that works on Retina / HiDPI displays.

**PANOPTES** —— 面向 [BIT](https://github.com/yxpil/bit) 生态 AI 智能体的屏幕操作 MCP 服务器：**9 个 MCP 工具**，通过 JSON-RPC 2.0（Streamable HTTP）实现截屏与鼠标键盘控制。一个 URL 即可让智能体拥有"眼睛"（截图）与"双手"（光标、按键、滚轮、键盘），并内置像素到光标坐标的换算，Retina / HiDPI 显示器下也能精准点击。

- Repo / 仓库: <https://github.com/yxpil/PANOPTES>
- Releases / 发行版: <https://github.com/yxpil/PANOPTES/releases>
- Binary name / 二进制名: `panoptes`
- Default port / 默认端口: `8757`
- Endpoint / 端点: `http://127.0.0.1:8757/` and `/mcp`

> **⚠️ The 7 control tools (mouse_* / key_*) only run when the server is started with explicit authorization. / 7 个控制工具（mouse_* / key_*）只有在服务器显式授权启动后才会运行。**

---

## Tools / 工具

| Tool / 工具 | What it gives the agent / 提供能力 | Control-gated / 控制门控 |
| --- | --- | --- |
| `screenshot` | capture a monitor/region to PNG; optional embedded image content / 截取显示器或区域为 PNG，可选内嵌图像 | — |
| `screen_info` | monitors: index, name, origin, size, scale, primary / 显示器列表与几何信息 | — |
| `mouse_position` | current cursor position in global coordinates / 当前光标全局坐标 | yes |
| `mouse_move` | absolute or relative move, returns post-move position / 绝对/相对移动，返回移动后位置 | yes |
| `mouse_click` | left/right/middle, 1–3 clicks, optional pre-move / 左中右键单击到三击 | yes |
| `mouse_drag` | press at from, interpolate to to, release / 按下-插值拖动-释放 | yes |
| `mouse_scroll` | wheel clicks, vertical or horizontal / 滚轮，垂直或水平 | yes |
| `key_type` | type literal text into the focused window / 向焦点窗口输入文本 | yes |
| `key_press` | key or chord: `cmd+c`, `enter`, `shift+tab`, `f5`… / 单键或组合键 | yes |

Gated tools stay listed when locked; calling one returns an `isError` result that names the authorization flag, so the agent can ask the operator to unlock. / 门控工具在锁定时仍会列出；调用时返回 `isError` 结果并注明授权旗标，智能体可据此请求操作者解锁。

## Install / 安装

Grab a per-platform binary from the [latest release](https://github.com/yxpil/PANOPTES/releases/latest), or:

从 [最新 Release](https://github.com/yxpil/PANOPTES/releases/latest) 下载对应平台二进制，或：

```bash
cargo install --git https://github.com/yxpil/PANOPTES
```

## Quick start / 快速开始

```bash
# screenshots only / 仅截图
panoptes serve

# with control tools unlocked / 解锁控制工具
panoptes serve --yes-i-can-control-this-machine
# or / 或: PANOPTES_I_CAN_CONTROL=yes panoptes serve

curl http://127.0.0.1:8757/health
# {"control_authorized":false,"data_dir":"…","ok":true,"service":"panoptes","tools":9,"version":"0.1.0"}
```

Every tool is mirrored as a CLI subcommand; piped stdin JSON is merged over the CLI args (stdin wins). / 每个工具都有同名 CLI 子命令；管道 stdin JSON 会合并进 CLI 参数（stdin 优先）。

## Coordinate system / 坐标系

Mouse tools take **global logical coordinates**: origin at the primary monitor's top-left, y down, one unit = one point (macOS) / one DIP (Windows) / one pixel (X11). `screenshot` reports the captured image size, monitor origin and scale ratio, so a pixel `(px, py)` found in the image maps to the cursor position `logical = origin + (px, py) / scale`. On a Retina display the capture is 2x the logical size and clicks still land exactly.

鼠标工具使用**全局逻辑坐标**：原点在主显示器左上角，y 向下，一个单位 = 一个 point（macOS）/ 一个 DIP（Windows）/ 一个像素（X11）。`screenshot` 返回截图像素尺寸、显示器原点与缩放比，因此图像中的像素 `(px, py)` 换算为光标位置 `logical = origin + (px, py) / scale`。Retina 截图是逻辑尺寸的 2 倍，点击依然精准。

## Platform notes / 平台说明

- **macOS** — grant Screen Recording (screenshots) and Accessibility (input) to the launching app: System Settings → Privacy & Security. The input backend initializes lazily, so screenshot-only mode needs no Accessibility permission. / 在"系统设置 → 隐私与安全性"中为启动 panoptes 的应用授予"屏幕录制"（截图）与"辅助功能"（输入）。输入后端惰性初始化，纯截图模式无需辅助功能权限。
- **Linux** — X11 sessions (Wayland input is not supported by enigo). / 支持 X11 会话（enigo 不支持 Wayland 输入）。
- **Windows** — works out of the box. / 开箱即用。

## FAQ

**Is it safe to leave the server running? / 服务器一直开着安全吗？**
Yes — default bind is loopback-only (`127.0.0.1`), and the control tools refuse to run until the operator passes the authorization flag at startup. Screenshot tools are read-only. / 安全 —— 默认仅绑定回环地址（`127.0.0.1`），且控制工具在操作者未显式授权前拒绝运行。截图工具只读。

**Where do screenshots go? / 截图存到哪里？**
`<data_dir>/shots/` — `<data_dir>` is `--data-dir` / `PANOPTES_DATA_DIR` / `~/.panoptes`. The tool result includes the absolute path; BIT views it with its built-in image viewer. / 存于 `<data_dir>/shots/`，`<data_dir>` 取自 `--data-dir` / `PANOPTES_DATA_DIR` / `~/.panoptes`。工具结果含绝对路径，BIT 用内置图片查看器打开。

**Does the double-click work? / 支持双击吗？**
`mouse_click` with `clicks: 2` (or 3) sends a real OS multi-click, not two separate clicks. / `mouse_click` 的 `clicks: 2`（或 3）发送真实的系统多击事件，而非两次独立单击。

## Protocol

See [Protocol](https://github.com/yxpil/PANOPTES/wiki/Protocol) for the full MCP wire reference (handshake, methods, error codes, tool schemas, examples). / 完整 MCP 协议参考（握手、方法、错误码、工具 schema 与示例）见 [Protocol](https://github.com/yxpil/PANOPTES/wiki/Protocol)。
