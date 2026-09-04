//! Input injection via `enigo`: cursor, buttons, wheel, keyboard. Includes a
//! pure chord parser (`"cmd+shift+t"`) and a Retina-safe cursor-position fix.

use std::sync::MutexGuard;

use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use serde_json::{json, Value};

/// Buttons accepted by the tools (mirror of the JSON-facing names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Left,
    Right,
    Middle,
}

impl Btn {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "left" | "" => Ok(Btn::Left),
            "right" => Ok(Btn::Right),
            "middle" => Ok(Btn::Middle),
            other => Err(format!(
                "unknown button '{other}' (expected left, right or middle)"
            )),
        }
    }

    fn enigo(self) -> Button {
        match self {
            Btn::Left => Button::Left,
            Btn::Right => Button::Right,
            Btn::Middle => Button::Middle,
        }
    }
}

/// Parse a chord like `"cmd+shift+t"` into enigo keys. Modifiers come first in
/// the returned vector, in the order written; the regular key(s) follow.
pub fn parse_combo(s: &str) -> Result<Vec<Key>, String> {
    let mut mods = Vec::new();
    let mut regular = Vec::new();
    for part in s.split('+') {
        let name = part.trim().to_ascii_lowercase();
        if name.is_empty() {
            return Err(format!("empty key in chord '{s}'"));
        }
        let key = match name.as_str() {
            "cmd" | "command" | "meta" | "win" | "super" => Key::Meta,
            "ctrl" | "control" => Key::Control,
            "alt" | "option" | "opt" => Key::Alt,
            "shift" => Key::Shift,
            "enter" | "return" => Key::Return,
            "tab" => Key::Tab,
            "esc" | "escape" => Key::Escape,
            "space" => Key::Space,
            "backspace" => Key::Backspace,
            "delete" | "del" => Key::Delete,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "up" | "arrowup" => Key::UpArrow,
            "down" | "arrowdown" => Key::DownArrow,
            "left" | "arrowleft" => Key::LeftArrow,
            "right" | "arrowright" => Key::RightArrow,
            other => {
                if let Some(num) = other.strip_prefix('f') {
                    let idx: Option<u8> = match num {
                        "1" => Some(1),
                        "2" => Some(2),
                        "3" => Some(3),
                        "4" => Some(4),
                        "5" => Some(5),
                        "6" => Some(6),
                        "7" => Some(7),
                        "8" => Some(8),
                        "9" => Some(9),
                        "10" => Some(10),
                        "11" => Some(11),
                        "12" => Some(12),
                        _ => None,
                    };
                    match idx {
                        Some(1) => regular.push(Key::F1),
                        Some(2) => regular.push(Key::F2),
                        Some(3) => regular.push(Key::F3),
                        Some(4) => regular.push(Key::F4),
                        Some(5) => regular.push(Key::F5),
                        Some(6) => regular.push(Key::F6),
                        Some(7) => regular.push(Key::F7),
                        Some(8) => regular.push(Key::F8),
                        Some(9) => regular.push(Key::F9),
                        Some(10) => regular.push(Key::F10),
                        Some(11) => regular.push(Key::F11),
                        Some(12) => regular.push(Key::F12),
                        _ => return Err(format!("unknown key '{other}' in chord '{s}'")),
                    }
                    continue;
                }
                let mut chars = other.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) => Key::Unicode(c),
                    _ => return Err(format!("unknown key '{other}' in chord '{s}'")),
                }
            }
        };
        if matches!(key, Key::Meta | Key::Control | Key::Alt | Key::Shift) {
            mods.push(key);
        } else {
            regular.push(key);
        }
    }
    if regular.is_empty() {
        // A bare modifier ("cmd") is pressed and released as a click.
        match mods.pop() {
            Some(k) => regular.push(k),
            None => return Err(format!("empty chord '{s}'")),
        }
    }
    mods.extend(regular);
    Ok(mods)
}

/// Pure helper: on macOS enigo's `location()` mixes units on Retina — it
/// subtracts an AppKit point-y from the display height in *physical pixels*.
/// This recovers the top-left-origin logical y that `move_mouse(Abs)` expects.
/// `point_h` = primary monitor height in points, `scale` = capture ratio.
pub fn correct_macos_y(y_enigo: i32, point_h: u32, scale: f64) -> i32 {
    y_enigo - (point_h as f64 * (scale - 1.0)).round() as i32
}

/// Everything the input ops need: an exclusively held, initialized enigo
/// handle plus the primary monitor's logical height for the macOS y
/// correction. The handle lives inside `Option` so it can be created lazily
/// on first use (see `RealBackend::input`).
pub struct InputCtx<'a> {
    enigo: MutexGuard<'a, Option<Enigo>>,
}

impl<'a> InputCtx<'a> {
    /// Take ownership of a locked enigo slot that is guaranteed initialized.
    pub fn new(enigo: MutexGuard<'a, Option<Enigo>>) -> Self {
        Self { enigo }
    }

    fn enigo(&mut self) -> Result<&mut Enigo, String> {
        self.enigo
            .as_mut()
            .ok_or_else(|| "input backend not initialized".to_string())
    }

    /// Retina-safe cursor position in the global space `move_mouse(Abs)` uses.
    fn location(enigo: &mut Enigo) -> Result<(i32, i32), String> {
        let (x, y) = enigo
            .location()
            .map_err(|e| format!("cursor query failed: {e}"))?;
        if !cfg!(target_os = "macos") {
            return Ok((x, y));
        }
        // macOS: correct y with the primary monitor's geometry.
        let corrected = crate::capture::list_monitors()
            .ok()
            .and_then(|mons| {
                let primary = mons.iter().find(|m| m.geom.is_primary)?;
                let scale = 2.0_f64.max(primary.geom.scale_factor as f64);
                Some(correct_macos_y(y, primary.geom.height, scale))
            })
            .unwrap_or(y);
        Ok((x, corrected))
    }

    fn move_abs(enigo: &mut Enigo, x: i32, y: i32) -> Result<(), String> {
        enigo
            .move_mouse(x, y, Coordinate::Abs)
            .map_err(|e| format!("mouse move failed: {e}"))
    }

    /// Current cursor position payload.
    pub fn position(&mut self) -> Result<Value, String> {
        let (x, y) = Self::location(self.enigo()?)?;
        Ok(json!({
            "x": x,
            "y": y,
            "note": "global coordinates — exactly what mouse_move / mouse_click expect",
        }))
    }

    /// Move; `relative` is implemented on top of the corrected absolute space
    /// (enigo's own relative path consults the buggy macOS `location()`).
    pub fn move_to(&mut self, x: i32, y: i32, relative: bool) -> Result<Value, String> {
        let enigo = self.enigo()?;
        let (tx, ty) = if relative {
            let (cx, cy) = Self::location(enigo)?;
            (cx + x, cy + y)
        } else {
            (x, y)
        };
        Self::move_abs(enigo, tx, ty)?;
        let (fx, fy) = Self::location(enigo)?;
        Ok(json!({ "x": fx, "y": fy }))
    }

    /// Click `clicks` times, optionally pre-moving to (x, y).
    pub fn click(
        &mut self,
        button: Btn,
        clicks: u32,
        pre_move: Option<(i32, i32)>,
    ) -> Result<Value, String> {
        if clicks == 0 || clicks > 3 {
            return Err("clicks must be 1, 2 or 3".into());
        }
        let enigo = self.enigo()?;
        if let Some((x, y)) = pre_move {
            Self::move_abs(enigo, x, y)?;
        }
        let b = button.enigo();
        for _ in 0..clicks {
            enigo
                .button(b, Direction::Click)
                .map_err(ie("click failed"))?;
        }
        let (x, y) = Self::location(enigo)?;
        Ok(
            json!({ "button": format!("{button:?}").to_lowercase(), "clicks": clicks, "position": [x, y] }),
        )
    }

    /// Press at `from`, interpolate to `to`, release.
    pub fn drag(
        &mut self,
        button: Btn,
        from: (i32, i32),
        to: (i32, i32),
        duration_ms: u64,
        steps: u32,
    ) -> Result<Value, String> {
        if duration_ms == 0 || duration_ms > 5000 {
            return Err("duration_ms must be 1-5000".into());
        }
        let steps = steps.clamp(1, 100);
        let enigo = self.enigo()?;
        let b = button.enigo();
        Self::move_abs(enigo, from.0, from.1)?;
        enigo
            .button(b, Direction::Press)
            .map_err(ie("press failed"))?;
        std::thread::sleep(std::time::Duration::from_millis(20));
        let per_step = (duration_ms / steps as u64).max(1);
        for step in 1..=steps {
            let t = step as f64 / steps as f64;
            let x = from.0 as f64 + (to.0 - from.0) as f64 * t;
            let y = from.1 as f64 + (to.1 - from.1) as f64 * t;
            Self::move_abs(enigo, x as i32, y as i32)?;
            std::thread::sleep(std::time::Duration::from_millis(per_step));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        enigo
            .button(b, Direction::Release)
            .map_err(ie("release failed"))?;
        let (x, y) = Self::location(enigo)?;
        Ok(json!({
            "from": [from.0, from.1],
            "to": [to.0, to.1],
            "duration_ms": duration_ms,
            "steps": steps,
            "end_position": [x, y],
        }))
    }

    /// Scroll `amount` wheel clicks; positive = down (vertical) / right
    /// (horizontal), matching a normal wheel (verified in enigo's macOS impl:
    /// positive length produces a negative CG wheel delta).
    pub fn scroll(&mut self, amount: i32, horizontal: bool) -> Result<Value, String> {
        let enigo = self.enigo()?;
        let axis = if horizontal {
            Axis::Horizontal
        } else {
            Axis::Vertical
        };
        enigo.scroll(amount, axis).map_err(ie("scroll failed"))?;
        Ok(json!({ "amount": amount, "axis": if horizontal { "horizontal" } else { "vertical" } }))
    }

    /// Type literal text.
    pub fn key_type(&mut self, text: &str) -> Result<Value, String> {
        if text.is_empty() {
            return Err("text is empty".into());
        }
        let enigo = self.enigo()?;
        enigo.text(text).map_err(ie("typing failed"))?;
        Ok(json!({ "typed_chars": text.chars().count() }))
    }

    /// Press a chord: modifiers are pressed, regular keys clicked, modifiers
    /// released in reverse order.
    pub fn key_press(&mut self, combo: &str) -> Result<Value, String> {
        let keys = parse_combo(combo)?;
        let enigo = self.enigo()?;
        let is_modifier = |k: &Key| matches!(k, Key::Meta | Key::Control | Key::Alt | Key::Shift);
        let mods: Vec<Key> = keys.iter().filter(|k| is_modifier(k)).copied().collect();
        let regular: Vec<Key> = keys.iter().filter(|k| !is_modifier(k)).copied().collect();
        for k in &mods {
            enigo
                .key(*k, Direction::Press)
                .map_err(ie("key press failed"))?;
        }
        for k in &regular {
            enigo
                .key(*k, Direction::Click)
                .map_err(ie("key click failed"))?;
        }
        for k in mods.iter().rev() {
            enigo
                .key(*k, Direction::Release)
                .map_err(ie("key release failed"))?;
        }
        Ok(json!({ "keys": combo }))
    }
}

fn ie(context: &'static str) -> impl Fn(enigo::InputError) -> String {
    move |e| format!("{context}: {e}")
}

/// Build the enigo handle. `open_prompt_to_get_permissions` (default on) pops
/// the macOS Accessibility prompt on first use instead of failing silently.
pub fn new_enigo() -> Result<Enigo, String> {
    Enigo::new(&Settings {
        open_prompt_to_get_permissions: true,
        ..Settings::default()
    })
    .map_err(|e| format!("failed to initialize input backend: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(names: &[&str]) -> Vec<Key> {
        parse_combo(&names.join("+")).expect("parse")
    }

    fn is_mod(k: &Key) -> bool {
        matches!(k, Key::Meta | Key::Control | Key::Alt | Key::Shift)
    }

    #[test]
    fn simple_key() {
        let k = keys(&["t"]);
        assert_eq!(k.len(), 1);
        assert!(!is_mod(&k[0]));
    }

    #[test]
    fn chord_orders_modifiers_first() {
        let k = keys(&["cmd", "shift", "t"]);
        assert_eq!(k.len(), 3);
        assert!(is_mod(&k[0]) && is_mod(&k[1]));
        assert!(!is_mod(&k[2]));
    }

    #[test]
    fn modifier_aliases() {
        for alias in ["cmd", "command", "meta", "win", "super"] {
            assert!(is_mod(&keys(&[alias])[0]), "alias {alias}");
        }
        for alias in ["ctrl", "control"] {
            assert!(is_mod(&keys(&[alias])[0]), "alias {alias}");
        }
        assert!(is_mod(&keys(&["option"])[0]));
    }

    #[test]
    fn named_keys() {
        let k = keys(&["enter"]);
        assert_eq!(k.len(), 1);
        let k = keys(&["f12"]);
        assert_eq!(k.len(), 1);
        let k = keys(&["arrowup"]);
        assert_eq!(k.len(), 1);
    }

    #[test]
    fn bare_modifier_becomes_a_click() {
        let k = keys(&["cmd"]);
        assert_eq!(k.len(), 1);
        assert!(is_mod(&k[0]));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_combo("").is_err());
        assert!(parse_combo("cmd++c").is_err());
        assert!(parse_combo("notakey").is_err());
        assert!(parse_combo("f13").is_err());
    }

    #[test]
    fn macos_y_correction() {
        // Retina: enigo y is inflated by point_height * (scale - 1).
        assert_eq!(correct_macos_y(1464, 982, 2.0), 482);
        // 1x displays are no-ops.
        assert_eq!(correct_macos_y(482, 982, 1.0), 482);
    }
}
