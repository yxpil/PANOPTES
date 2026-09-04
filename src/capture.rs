//! Screen capture via `xcap`: monitor enumeration, PNG screenshots and the
//! JSON payload that teaches the model the pixel → mouse-coordinate mapping.

use std::path::Path;

use image::{imageops::crop_imm, ImageFormat, RgbaImage};
use serde_json::{json, Value};
use xcap::Monitor;

use crate::coords::{clamp_region, MonitorGeom};

/// Result of one screenshot: the always-non-empty text payload plus an
/// optional base64 PNG (for `return_format: "image"`).
pub struct ScreenshotOut {
    pub json: Value,
    pub png_b64: Option<String>,
}

/// One monitor: geometry (see [`MonitorGeom`]) + display name.
pub struct MonitorInfo {
    pub geom: MonitorGeom,
    pub name: String,
}

/// Enumerate monitors. `index` is the position in xcap's list, which is also
/// what the `screenshot`/`screen_info` tools accept as `monitor`.
pub fn list_monitors() -> Result<Vec<MonitorInfo>, String> {
    let monitors = Monitor::all().map_err(|e| format!("failed to list monitors: {e}"))?;
    let mut out = Vec::with_capacity(monitors.len());
    for (index, m) in monitors.into_iter().enumerate() {
        let name = m.name().unwrap_or_else(|_| format!("monitor {index}"));
        let geom = MonitorGeom {
            index,
            x: m.x().map_err(eg)?,
            y: m.y().map_err(eg)?,
            width: m.width().map_err(eg)?,
            height: m.height().map_err(eg)?,
            scale_factor: m.scale_factor().unwrap_or(1.0),
            is_primary: m.is_primary().unwrap_or(index == 0),
        };
        out.push(MonitorInfo { geom, name });
    }
    if out.is_empty() {
        return Err("no monitors detected".into());
    }
    Ok(out)
}

fn eg(e: xcap::XCapError) -> String {
    format!("monitor query failed: {e}")
}

/// Capture a screenshot and write it as PNG under `data_dir/shots/`.
///
/// `monitor` selects the display (None = primary); `region` is a crop rect in
/// captured-image pixels; `include_image` additionally base64-encodes the PNG
/// for MCP image content.
pub fn capture(
    monitor: Option<usize>,
    region: Option<(u32, u32, u32, u32)>,
    include_image: bool,
    data_dir: &Path,
) -> Result<ScreenshotOut, String> {
    let monitors = list_monitors()?;
    let info = match monitor {
        Some(idx) => monitors.get(idx).ok_or_else(|| {
            format!(
                "monitor {idx} does not exist ({} monitor(s) detected)",
                monitors.len()
            )
        })?,
        None => monitors
            .iter()
            .find(|m| m.geom.is_primary)
            .unwrap_or(&monitors[0]),
    };
    let geom = info.geom.clone();

    let mon = Monitor::all()
        .map_err(|e| format!("failed to re-open monitors: {e}"))?
        .into_iter()
        .nth(geom.index)
        .ok_or_else(|| format!("monitor {} disappeared", geom.index))?;
    let image: RgbaImage = mon
        .capture_image()
        .map_err(|e| capture_error(&e, data_dir))?;

    // Region crop happens in-process, in captured-image pixel space: xcap's
    // `capture_region` takes points on macOS but pixels on Windows, so it is
    // deliberately avoided here.
    let image_w = image.width();
    let (cropped, region_px) = match region {
        None => (image, None),
        Some((x, y, w, h)) => {
            let (x, y, w, h) =
                clamp_region(x, y, w, h, image.width(), image.height()).ok_or_else(|| {
                    "region is empty after clamping to the captured image".to_string()
                })?;
            let cropped = crop_imm(&image, x, y, w, h).to_image();
            (cropped, Some((x, y, w, h)))
        }
    };

    // Persist under <data_dir>/shots/.
    let dir = data_dir.join("shots");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;
    let path = dir.join(format!(
        "shot-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    cropped
        .save_with_format(&path, ImageFormat::Png)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    let scale = crate::coords::effective_scale(&geom, image_w);
    let json = json!({
        "path": path.display().to_string(),
        "image_px": { "width": cropped.width(), "height": cropped.height() },
        "logical": { "width": geom.width, "height": geom.height },
        "scale_factor": scale,
        "monitor": {
            "index": geom.index,
            "name": info.name,
            "origin": [geom.x, geom.y],
            "is_primary": geom.is_primary,
        },
        "region_px": region_px.map(|(x, y, w, h)| json!({"x": x, "y": y, "width": w, "height": h})),
        "pixel_to_logical": "mouse tools take global coordinates: logical = monitor.origin + (region.origin + pixel) / scale_factor",
    });

    let png_b64 = if include_image {
        let mut bytes = Vec::new();
        cropped
            .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
            .map_err(|e| format!("failed to encode PNG: {e}"))?;
        use base64::Engine as _;
        Some(base64::engine::general_purpose::STANDARD.encode(bytes))
    } else {
        None
    };

    Ok(ScreenshotOut { json, png_b64 })
}

/// Wrap capture failures with the macOS Screen Recording hint — permission
/// failures are silent (wallpaper-only) or plain errors, so the message must
/// teach the fix.
fn capture_error(e: &xcap::XCapError, _data_dir: &Path) -> String {
    format!(
        "screen capture failed: {e}. On macOS grant Screen Recording permission to the app that \
         runs panoptes (System Settings > Privacy & Security > Screen Recording); the binary \
         lives at {:?}. On Linux only X11 is supported.",
        std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    )
}

/// `screen_info` payload.
pub fn screen_info() -> Result<Value, String> {
    let monitors = list_monitors()?;
    Ok(json!({
        "monitors": monitors.iter().map(|m| json!({
            "index": m.geom.index,
            "name": m.name,
            "origin": [m.geom.x, m.geom.y],
            "width": m.geom.width,
            "height": m.geom.height,
            "scale_factor": m.geom.scale_factor,
            "is_primary": m.geom.is_primary,
        })).collect::<Vec<_>>(),
        "coordinate_note": "screenshot pixels are (monitor size * scale_factor); mouse tools take global logical coordinates",
    }))
}
