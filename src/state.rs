//! Shared server state: control authorization + the screen backend.

use std::path::PathBuf;
use std::sync::Arc;

use crate::backend::ScreenBackend;
use crate::tools::{tools, ToolDef};

/// Immutable state shared by all MCP handlers and the CLI dispatch path.
pub struct AppState {
    /// Mirrors `--yes-i-can-control-this-machine` / `PANOPTES_I_CAN_CONTROL=yes`.
    pub control_authorized: bool,
    /// Screenshots land under `<data_dir>/shots/`.
    pub data_dir: PathBuf,
    /// The screen backend (real OS calls in production, fake in tests).
    pub backend: Arc<dyn ScreenBackend>,
}

impl AppState {
    /// All tools in stable registry order (gated tools stay listed; the gate
    /// is enforced at call time so callers learn how to request access).
    pub fn exposed_tools(&self) -> impl Iterator<Item = &ToolDef> {
        tools().iter()
    }

    /// Look up one tool by MCP name.
    pub fn find_tool(&self, name: &str) -> Option<&ToolDef> {
        self.exposed_tools().find(|t| t.name == name)
    }
}
