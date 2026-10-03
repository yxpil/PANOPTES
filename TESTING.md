# PANOPTES 测试说明
- 测试完成：是（2026-10-04）
- 测试日期：2026-10-04
- 测试内容：单元覆盖 coords(坐标映射/Retina 缩放/区域钳制)、input(键位组合解析)、tools(静态注册表与控制门)；集成覆盖 cli 与 mcp(经假 `ScreenBackend` 驱动真实 `dispatch_sync`)；注入测试覆盖参数解析层（`key_type` 的 XSS/shell 串被逐字透传为字面按键、错 JSON 类型/未知按钮/滚动轴/截图格式/单边坐标均返回干净 isError）。无运行时钩子/插件/事件机制。
- 运行命令：`cargo test`（仓库根）
- 测试框架：Rust `#[cfg(test)]` + `cargo test`（集成测试位于 `tests/`）
- 模型：豆包（Doubao）生成

# Testing PANOPTES

PANOPTES is a screen-operation MCP server (screenshots + mouse/keyboard control).
It deliberately keeps a thin **backend seam** (`ScreenBackend` trait) so tests
can substitute a fake backend and never touch the real screen or inject real
input. All tests run under `cargo test`.

| Layer | Location | What it covers |
|-------|----------|----------------|
| Unit | `src/coords.rs`, `src/input.rs`, `src/tools.rs` (`#[cfg(test)]`) | Pure coordinate mapping / Retina scaling, region clamping, the key-chord parser, tool registry shape & gating. |
| CLI | `tests/cli.rs` | Binary argument validation, piped-stdin contract, control-gate refusal, help/version. |
| MCP protocol | `tests/mcp.rs` | Full MCP handshake, `tools/list`, `tools/call` round-trip, control gate, backend-failure mapping, JSON-RPC error codes — all against a fake backend. |
| Injection / robustness | `tests/injection.rs` | Argument-handling security — see below. |

## Run

```sh
cargo test
```

The only network touched is ephemeral loopback for the in-process MCP router.
Tests use a fake `ScreenBackend`; **no** real screenshot is taken and **no**
real mouse/keyboard event is injected.

## What the injection tests assert

PANOPTES has no web page (nothing to XSS into), no database (no SQL) and no
shell-out or request-derived file path, so its untrusted-input surface is the
JSON argument parser in `dispatch`. `tests/injection.rs` drives the real
`dispatch_sync` funnel with a recording fake backend and proves:

- hostile *text* (`<img src=x onerror=...>; rm -rf /; $(whoami)`) handed to
  `key_type` is delivered **byte-for-byte** to the backend as literal keystrokes —
  never shell-expanded, never escaped (there is nothing to render);
- wrong JSON types (a string for a numeric coordinate) → clean `isError`,
  never a panic or silent coercion;
- one-sided coordinates (`mouse_click` with only `x`), unknown button names,
  unknown scroll axes, unknown screenshot `return_format`, and unknown tools
  are all rejected as readable `isError` results.

## Hook / plugin tests

PANOPTES exposes a **static, compile-time** tool registry (`tools()`); there is
no user-registerable hook, plugin, event bus or callback mechanism. There are
therefore **0 hook tests**. The control gate (a fixed boolean flag) is the only
cross-cutting concern and is covered by `tests/mcp.rs::control_gate_locked_and_unlocked`.

## Expected result

`cargo test` should finish green. As of this document the suites are:
unit 17, `cli.rs` 6, `mcp.rs` 5, `injection.rs` 7 (passing:failing = 35:0).
