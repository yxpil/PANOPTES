//! PANOPTES — screen operation MCP server for AI agents: screenshots plus
//! mouse and keyboard control around the BIT ecosystem.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use serde_json::{json, Value};

use panoptes::backend::{ensure_data_dir, RealBackend};
use panoptes::cli::{Cli, Command};
use panoptes::dispatch;
use panoptes::state::AppState;
use panoptes::tools::tools;

fn log(msg: &str) {
    eprintln!("[panoptes] {msg}");
}

/// Resolve the data dir: `--data-dir` / `PANOPTES_DATA_DIR` / `~/.panoptes`.
fn data_dir(cli: &Cli) -> PathBuf {
    if let Some(dir) = &cli.data_dir {
        return dir.clone();
    }
    if let Ok(dir) = std::env::var("PANOPTES_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".panoptes")
}

/// Build the app state with the real backend.
fn build_state(cli: &Cli, control_authorized: bool) -> Result<AppState> {
    let dir = data_dir(cli);
    ensure_data_dir(&dir).map_err(anyhow::Error::msg)?;
    let backend = RealBackend::new(dir.clone()).map_err(anyhow::Error::msg)?;
    log("screen backend ready (xcap capture; input tools initialize on first use)");
    if !control_authorized {
        log("input tools LOCKED — start with --yes-i-can-control-this-machine to unlock");
    }
    Ok(AppState {
        control_authorized,
        data_dir: dir,
        backend: Arc::new(backend),
    })
}

async fn serve(cli: &Cli, host: String, port: u16, control_authorized: bool) -> Result<()> {
    let state = build_state(cli, control_authorized)?;
    let router = panoptes::mcp::mcp_router(state);
    let listener = tokio::net::TcpListener::bind((host.as_str(), port)).await?;
    log(&format!(
        "{}/{} tools; MCP endpoint listening on http://{host}:{port}/ (also /mcp)",
        tools().len(),
        panoptes::mcp::registered_tool_count(),
    ));
    axum::serve(listener, router).await?;
    Ok(())
}

/// Run one tool subcommand through the same dispatch funnel the MCP server
/// uses, after merging piped stdin JSON (stdin wins).
async fn run_tool(cli: &Cli, name: &str, mut args: Value, control_authorized: bool) -> Result<()> {
    if let Some(stdin) = read_piped_stdin() {
        match serde_json::from_str::<Value>(&stdin) {
            Ok(Value::Object(extra)) => {
                if let Value::Object(map) = &mut args {
                    for (k, v) in extra {
                        map.insert(k, v);
                    }
                }
            }
            Ok(_) => return Err(anyhow::Error::msg("piped stdin must be a JSON object")),
            Err(e) => {
                return Err(anyhow::Error::msg(format!(
                    "piped stdin is not valid JSON: {e}"
                )))
            }
        }
    }
    let state = build_state(cli, control_authorized)?;
    let outcome = dispatch::call(&state, name, args).await;
    if outcome.is_error {
        let text = outcome
            .content
            .first()
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("tool failed")
            .to_string();
        return Err(anyhow::Error::msg(text));
    }
    let mut payload = outcome.payload.unwrap_or(Value::Null);
    // Surface the screenshot base64 for CLI consumers that asked for it.
    if name == "screenshot" {
        let png = outcome
            .content
            .iter()
            .find(|c| c.get("type").and_then(|t| t.as_str()) == Some("image"))
            .and_then(|c| c.get("data"))
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());
        if let (Some(png), Value::Object(map)) = (png, &mut payload) {
            map.insert("png_base64".into(), Value::String(png));
        }
    }
    if cli.pretty {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{payload}");
    }
    Ok(())
}

/// Read stdin when it is piped (BIT exec-mode contract); None on a TTY.
fn read_piped_stdin() -> Option<String> {
    use std::io::{IsTerminal, Read};
    if std::io::stdin().is_terminal() {
        return None;
    }
    let mut buf = String::new();
    match std::io::stdin().read_to_string(&mut buf) {
        Ok(0) => None,
        Ok(_) => Some(buf),
        Err(_) => None,
    }
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("[panoptes] error: {err:#}");
        std::process::exit(2);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let control_authorized = cli.yes_i_can_control_this_machine
        || std::env::var("PANOPTES_I_CAN_CONTROL").as_deref() == Ok("yes");

    let print_tools = |cli: &Cli| -> Result<()> {
        let inventory = json!({
            "control_authorized": control_authorized,
            "data_dir": data_dir(cli).display().to_string(),
            "tools": tools().iter().map(|t| json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema,
                "control_gated": t.control_gated,
            })).collect::<Vec<_>>(),
        });
        if cli.pretty {
            println!("{}", serde_json::to_string_pretty(&inventory)?);
        } else {
            println!("{inventory}");
        }
        Ok(())
    };

    match &cli.command {
        Some(Command::Serve { host, port }) => {
            serve(&cli, host.clone(), *port, control_authorized).await
        }
        Some(Command::Tools) => print_tools(&cli),
        Some(Command::Screenshot {
            monitor,
            region,
            with_image,
        }) => {
            let args = json!({
                "monitor": monitor,
                "region": region.as_ref().and_then(|r| match r.len() {
                    4 => Some(json!({"x": r[0], "y": r[1], "width": r[2], "height": r[3]})),
                    _ => None,
                }),
                "return_format": if *with_image { "image" } else { "text" },
            });
            run_tool(&cli, "screenshot", args, control_authorized).await
        }
        Some(Command::ScreenInfo) => {
            run_tool(&cli, "screen_info", json!({}), control_authorized).await
        }
        Some(Command::MousePosition) => {
            run_tool(&cli, "mouse_position", json!({}), control_authorized).await
        }
        Some(Command::MouseMove { x, y, relative }) => {
            let args = json!({ "x": x, "y": y, "relative": relative });
            run_tool(&cli, "mouse_move", args, control_authorized).await
        }
        Some(Command::MouseClick {
            button,
            clicks,
            x,
            y,
        }) => {
            let args = json!({ "button": button, "clicks": clicks, "x": x, "y": y });
            run_tool(&cli, "mouse_click", args, control_authorized).await
        }
        Some(Command::MouseDrag {
            from_x,
            from_y,
            to_x,
            to_y,
            button,
            duration_ms,
            steps,
        }) => {
            let args = json!({
                "from_x": from_x, "from_y": from_y, "to_x": to_x, "to_y": to_y,
                "button": button, "duration_ms": duration_ms, "steps": steps,
            });
            run_tool(&cli, "mouse_drag", args, control_authorized).await
        }
        Some(Command::MouseScroll { amount, axis }) => {
            let args = json!({ "amount": amount, "axis": axis });
            run_tool(&cli, "mouse_scroll", args, control_authorized).await
        }
        Some(Command::KeyType { text }) => {
            let args = json!({ "text": text });
            run_tool(&cli, "key_type", args, control_authorized).await
        }
        Some(Command::KeyPress { keys }) => {
            let args = json!({ "keys": keys });
            run_tool(&cli, "key_press", args, control_authorized).await
        }
        None => {
            Cli::command().print_help()?;
            Ok(())
        }
    }
}
