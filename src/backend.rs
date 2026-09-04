//! The seam between the MCP/CLI surface and the OS: a `ScreenBackend` trait
//! with the real implementation (`xcap` + `enigo`). Tests substitute their own
//! backend, so CI never touches the real screen or injects input.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use enigo::Enigo;
use serde_json::Value;

use crate::input::Btn;

/// One screenshot result: the text payload plus an optional base64 PNG.
pub struct Shot {
    pub json: Value,
    pub png_b64: Option<String>,
}

pub trait ScreenBackend: Send + Sync {
    fn screen_info(&self) -> Result<Value, String>;
    fn screenshot(
        &self,
        monitor: Option<usize>,
        region: Option<(u32, u32, u32, u32)>,
        include_image: bool,
    ) -> Result<Shot, String>;
    fn mouse_position(&self) -> Result<Value, String>;
    fn mouse_move(&self, x: i32, y: i32, relative: bool) -> Result<Value, String>;
    fn mouse_click(
        &self,
        button: Btn,
        clicks: u32,
        pre_move: Option<(i32, i32)>,
    ) -> Result<Value, String>;
    fn mouse_drag(
        &self,
        button: Btn,
        from: (i32, i32),
        to: (i32, i32),
        duration_ms: u64,
        steps: u32,
    ) -> Result<Value, String>;
    fn mouse_scroll(&self, amount: i32, horizontal: bool) -> Result<Value, String>;
    fn key_type(&self, text: &str) -> Result<Value, String>;
    fn key_press(&self, combo: &str) -> Result<Value, String>;
}

/// Real implementation: `enigo` behind a mutex (it is not `Sync`), created
/// lazily on the first input call so a screenshot-only server works without
/// the macOS Accessibility permission. xcap handles captures; PNGs land under
/// `<data_dir>/shots/`.
pub struct RealBackend {
    enigo: Mutex<Option<Enigo>>,
    data_dir: PathBuf,
}

impl RealBackend {
    pub fn new(data_dir: PathBuf) -> Result<Self, String> {
        Ok(Self {
            enigo: Mutex::new(None),
            data_dir,
        })
    }

    /// Lock and (on first use) initialize the enigo handle.
    fn input(&self) -> Result<crate::input::InputCtx<'_>, String> {
        let mut guard = self
            .enigo
            .lock()
            .map_err(|_| "input backend lock poisoned".to_string())?;
        if guard.is_none() {
            *guard = Some(crate::input::new_enigo()?);
        }
        Ok(crate::input::InputCtx::new(guard))
    }
}

impl ScreenBackend for RealBackend {
    fn screen_info(&self) -> Result<Value, String> {
        crate::capture::screen_info()
    }

    fn screenshot(
        &self,
        monitor: Option<usize>,
        region: Option<(u32, u32, u32, u32)>,
        include_image: bool,
    ) -> Result<Shot, String> {
        crate::capture::capture(monitor, region, include_image, &self.data_dir).map(|out| Shot {
            json: out.json,
            png_b64: out.png_b64,
        })
    }

    fn mouse_position(&self) -> Result<Value, String> {
        self.input()?.position()
    }

    fn mouse_move(&self, x: i32, y: i32, relative: bool) -> Result<Value, String> {
        self.input()?.move_to(x, y, relative)
    }

    fn mouse_click(
        &self,
        button: Btn,
        clicks: u32,
        pre_move: Option<(i32, i32)>,
    ) -> Result<Value, String> {
        self.input()?.click(button, clicks, pre_move)
    }

    fn mouse_drag(
        &self,
        button: Btn,
        from: (i32, i32),
        to: (i32, i32),
        duration_ms: u64,
        steps: u32,
    ) -> Result<Value, String> {
        self.input()?.drag(button, from, to, duration_ms, steps)
    }

    fn mouse_scroll(&self, amount: i32, horizontal: bool) -> Result<Value, String> {
        self.input()?.scroll(amount, horizontal)
    }

    fn key_type(&self, text: &str) -> Result<Value, String> {
        self.input()?.key_type(text)
    }

    fn key_press(&self, combo: &str) -> Result<Value, String> {
        self.input()?.key_press(combo)
    }
}

/// Ensure the data dir exists (used by both serve and CLI paths).
pub fn ensure_data_dir(data_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("failed to create data dir {}: {e}", data_dir.display()))
}
