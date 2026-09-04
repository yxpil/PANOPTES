//! Command-line interface definition (English help per ecosystem conventions).
//! Every tool subcommand mirrors one MCP tool; piped stdin JSON (non-TTY)
//! is merged over the CLI args, stdin wins — the BIT exec-mode contract.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "panoptes",
    version,
    about = "Screen operation MCP server for AI agents — screenshots plus mouse and keyboard control (JSON-RPC 2.0 over Streamable HTTP)",
    after_help = "The MCP endpoint answers on http://<host>:<port>/ and /mcp (Streamable HTTP).\n\
BIT: add the server URL in the MCP servers page, or let discovery find it on 127.0.0.1:8757.\n\n\
Control tools (mouse_* / key_*) move the cursor, click, type and scroll on this\n\
machine. They require explicit authorization:\n\
pass --yes-i-can-control-this-machine or set PANOPTES_I_CAN_CONTROL=yes.\n\n\
macOS permissions: Screen Recording (screenshots) and Accessibility (input)\n\
must be granted to the app that launches panoptes — see System Settings >\n\
Privacy & Security."
)]
pub struct Cli {
    /// Acknowledge that input tools may control this machine.
    #[arg(long, global = true)]
    pub yes_i_can_control_this_machine: bool,
    /// Data directory for screenshots (default: ~/.panoptes).
    #[arg(long, global = true, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,
    /// Pretty-print JSON output (human mode).
    #[arg(long, global = true)]
    pub pretty: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run the MCP server (Streamable HTTP JSON-RPC on / and /mcp)
    Serve {
        /// Bind address (default 127.0.0.1 — loopback only)
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Bind port
        #[arg(long, default_value_t = 8757)]
        port: u16,
    },
    /// Print the exposed MCP tools as JSON
    Tools,
    /// Capture the screen (or a monitor / region) to a PNG file
    Screenshot {
        /// 0-based monitor index (default: primary monitor)
        #[arg(long)]
        monitor: Option<usize>,
        /// Crop rect in captured-image pixels: x,y,width,height
        #[arg(long, value_name = "X,Y,W,H", value_delimiter = ',',
              num_args = 4, value_names = ["X", "Y", "W", "H"])]
        region: Option<Vec<u32>>,
        /// Also print the PNG as base64 (field: png_base64)
        #[arg(long)]
        with_image: bool,
    },
    /// List monitors with geometry and scale
    ScreenInfo,
    /// Print the current cursor position
    MousePosition,
    /// Move the cursor
    MouseMove {
        #[arg(long)]
        x: i32,
        #[arg(long)]
        y: i32,
        /// Move relative to the current position
        #[arg(long)]
        relative: bool,
    },
    /// Click a mouse button (optionally at x,y first)
    MouseClick {
        /// left | right | middle
        #[arg(long, default_value = "left")]
        button: String,
        /// 1 | 2 | 3 (double / triple click)
        #[arg(long, default_value_t = 1)]
        clicks: u32,
        #[arg(long)]
        x: Option<i32>,
        #[arg(long)]
        y: Option<i32>,
    },
    /// Press, interpolate the cursor to the target, release
    MouseDrag {
        #[arg(long)]
        from_x: i32,
        #[arg(long)]
        from_y: i32,
        #[arg(long)]
        to_x: i32,
        #[arg(long)]
        to_y: i32,
        /// left | right | middle
        #[arg(long, default_value = "left")]
        button: String,
        #[arg(long, default_value_t = 500)]
        duration_ms: u64,
        #[arg(long, default_value_t = 10)]
        steps: u32,
    },
    /// Scroll the wheel
    MouseScroll {
        /// Wheel clicks; positive = down (vertical) / right (horizontal)
        #[arg(long, default_value_t = 3)]
        amount: i32,
        /// vertical | horizontal
        #[arg(long, default_value = "vertical")]
        axis: String,
    },
    /// Type literal text into the focused window
    KeyType {
        /// Text to type ("-" reads it from stdin JSON instead)
        #[arg(long)]
        text: String,
    },
    /// Press a key or chord (e.g. "cmd+c", "enter", "shift+tab")
    KeyPress {
        /// Key or '+'-separated chord
        #[arg(long)]
        keys: String,
    },
}
