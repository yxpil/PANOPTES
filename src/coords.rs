//! Pure coordinate geometry: monitor layout and screenshot-pixel → enigo
//! coordinate mapping. No side effects, no platform calls — everything is
//! unit-testable.
//!
//! Platform facts this module is built around (verified against xcap/enigo
//! behaviour):
//! - macOS: xcap monitor `x/y/width/height` are logical points in the CG
//!   global space (origin: top-left of the main display); `capture_image()`
//!   returns physical pixels; enigo moves in the same logical point space.
//! - Windows / Linux X11: everything is physical pixels; the ratio is 1.0.
//! - Therefore the only trustworthy scale is the *self-calibrating* ratio
//!   `captured_image_width / monitor_width` — never `scale_factor()` (on
//!   Windows it is dpi/96 and would corrupt coordinates).

use serde::Serialize;

/// Geometry of one monitor, as xcap reports it (logical points on macOS,
/// physical pixels on Windows/X11) plus a best-effort `scale_factor`.
#[derive(Debug, Clone, Serialize)]
pub struct MonitorGeom {
    pub index: usize,
    /// Global origin (logical points on macOS, pixels on Windows/X11).
    pub x: i32,
    pub y: i32,
    /// Monitor width in the same unit as `x` (points on macOS).
    pub width: u32,
    pub height: u32,
    /// Best-effort DPI scale as reported by the platform (may be unreliable).
    pub scale_factor: f32,
    pub is_primary: bool,
}

/// Pixels (captured image) per enigo unit (logical point on macOS, pixel on
/// Windows/X11). Prefers the self-calibrating capture ratio; falls back to the
/// platform `scale_factor`, then to 1.0.
pub fn effective_scale(geom: &MonitorGeom, image_w: u32) -> f64 {
    if geom.width > 0 && image_w > 0 {
        let s = image_w as f64 / geom.width as f64;
        if s > 0.0 {
            return s;
        }
    }
    let s = geom.scale_factor as f64;
    if s > 0.0 {
        s
    } else {
        1.0
    }
}

/// Screenshot pixel (image space) → enigo absolute coordinate.
///
/// `pixel` is relative to the captured image (i.e. to `region_origin` when a
/// region crop was applied). `region_origin` is the region's top-left inside
/// the full monitor capture (pass `(0, 0)` for full-monitor shots). The result
/// is the global coordinate enigo expects: `origin + (region + pixel) / scale`.
/// Callers round with `as i32` at the call site.
pub fn pixel_to_logical_global(
    geom: &MonitorGeom,
    image_w: u32,
    region_origin: (u32, u32),
    pixel: (f64, f64),
) -> (f64, f64) {
    let s = effective_scale(geom, image_w);
    (
        geom.x as f64 + (region_origin.0 as f64 + pixel.0) / s,
        geom.y as f64 + (region_origin.1 as f64 + pixel.1) / s,
    )
}

/// Clamp + normalize a region rect against an image of `img_w x img_h`.
/// Returns `None` when the rect is empty after clamping (zero area).
pub fn clamp_region(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    img_w: u32,
    img_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    let x = x.min(img_w);
    let y = y.min(img_h);
    let width = width.min(img_w - x);
    let height = height.min(img_h - y);
    if width == 0 || height == 0 {
        None
    } else {
        Some((x, y, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn retina_primary() -> MonitorGeom {
        MonitorGeom {
            index: 0,
            x: 0,
            y: 0,
            width: 1512,
            height: 982,
            scale_factor: 2.0,
            is_primary: true,
        }
    }

    fn hd_secondary_left() -> MonitorGeom {
        MonitorGeom {
            index: 1,
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
            scale_factor: 1.0,
            is_primary: false,
        }
    }

    #[test]
    fn retina_capture_ratio_recovers_two() {
        // 3024 captured px across a 1512 pt monitor → scale 2.0.
        assert_eq!(effective_scale(&retina_primary(), 3024), 2.0);
    }

    #[test]
    fn one_to_one_when_pixels_match() {
        let geom = hd_secondary_left();
        assert_eq!(effective_scale(&geom, 1920), 1.0);
    }

    #[test]
    fn falls_back_to_scale_factor_then_one() {
        let mut geom = retina_primary();
        geom.width = 0; // capture geometry unavailable
        assert_eq!(effective_scale(&geom, 0), 2.0);
        geom.scale_factor = 0.0;
        assert_eq!(effective_scale(&geom, 0), 1.0);
    }

    #[test]
    fn pixel_mapping_on_retina_primary() {
        let geom = retina_primary();
        let (lx, ly) = pixel_to_logical_global(&geom, 3024, (0, 0), (1000.0, 500.0));
        assert_eq!((lx, ly), (500.0, 250.0));
    }

    #[test]
    fn negative_origin_monitor_maps_left_of_primary() {
        let geom = hd_secondary_left();
        let (lx, ly) = pixel_to_logical_global(&geom, 1920, (0, 0), (100.0, 50.0));
        assert_eq!((lx, ly), (-1820.0, 50.0));
    }

    #[test]
    fn region_origin_offsets_the_mapping() {
        // Region shot taken at (200, 100) inside a 3024x1964 Retina capture:
        // image pixel (0,0) is really (200,100) in monitor pixels.
        let geom = retina_primary();
        let (lx, ly) = pixel_to_logical_global(&geom, 3024, (200, 100), (100.0, 50.0));
        assert_eq!((lx, ly), (150.0, 75.0));
    }

    #[test]
    fn rounding_is_the_call_sites_job() {
        let geom = retina_primary();
        let (lx, ly) = pixel_to_logical_global(&geom, 3024, (0, 0), (3.0, 1.0));
        assert_eq!(lx as i32, 1);
        assert_eq!(ly as i32, 0);
    }

    #[test]
    fn region_clamping() {
        assert_eq!(clamp_region(0, 0, 100, 50, 200, 100), Some((0, 0, 100, 50)));
        // Oversized → clamped to the image bounds.
        assert_eq!(
            clamp_region(150, 90, 100, 100, 200, 100),
            Some((150, 90, 50, 10))
        );
        // Out of range → clamped to empty → None.
        assert_eq!(clamp_region(200, 100, 10, 10, 200, 100), None);
        assert_eq!(clamp_region(0, 0, 0, 10, 200, 100), None);
    }
}
