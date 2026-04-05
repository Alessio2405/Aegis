//! Desktop automation module — keyboard input, key combos, and app/URL launching.
//!
//! Uses the `enigo` crate for cross-platform keyboard control and
//! `std::process::Command` for opening files / URLs / applications.
//!
//! Security note: these functions should only be called when `desktop_control_enabled`
//! is true in settings (enforced by the command layer in commands.rs).

use enigo::{Enigo, Key, KeyboardControllable};
use crate::error::{AppError, AppResult};

// ── Public API ─────────────────────────────────────────────────────────────

/// Type a string of text as if the user typed it on the keyboard.
pub fn type_text(text: &str) -> AppResult<()> {
    let mut e = Enigo::new();
    e.key_sequence(text);
    Ok(())
}

/// Press a key combination such as "ctrl+c", "ctrl+shift+t", "win+r", "alt+f4".
///
/// Format: modifier keys separated by `+`, final token is the main key.
/// Modifiers: ctrl/control, shift, alt, win/super/meta.
/// Special keys: enter/return, escape/esc, tab, space, backspace, delete,
///               home, end, pageup, pagedown, up, down, left, right,
///               f1..f12.
pub fn key_combo(combo: &str) -> AppResult<()> {
    let parts: Vec<&str> = combo
        .split('+')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    if parts.is_empty() {
        return Err(AppError::InvalidInput("Key combo cannot be empty".into()));
    }

    // Split into modifiers (all but last) and main key (last)
    let (modifier_strs, main_str) = parts.split_at(parts.len() - 1);

    let main_key = parse_key(main_str[0])
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown key: '{}'", main_str[0])))?;

    let mut modifiers: Vec<Key> = Vec::new();
    for m in modifier_strs {
        let k = parse_modifier(m)
            .ok_or_else(|| AppError::InvalidInput(format!("Unknown modifier key: '{m}'")))?;
        modifiers.push(k);
    }

    let mut e = Enigo::new();

    // Press all modifiers
    for &m in &modifiers {
        e.key_down(m);
    }
    // Click the main key
    e.key_click(main_key);
    // Release modifiers in reverse order
    for &m in modifiers.iter().rev() {
        e.key_up(m);
    }

    Ok(())
}

/// Open a file, folder, URL, or application name using the OS default handler.
///
/// - On Windows: `cmd /C start "" <target>`
/// - On macOS:   `open <target>`
/// - On Linux:   `xdg-open <target>`
pub fn open_target(target: &str) -> AppResult<()> {
    if target.trim().is_empty() {
        return Err(AppError::InvalidInput("Target cannot be empty".into()));
    }

    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/C", "start", "", target])
        .spawn()
        .map_err(AppError::Io)?;

    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg(target)
        .spawn()
        .map_err(AppError::Io)?;

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    std::process::Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map_err(AppError::Io)?;

    Ok(())
}

// ── Key parsing helpers ────────────────────────────────────────────────────

fn parse_modifier(s: &str) -> Option<Key> {
    match s.to_lowercase().as_str() {
        "ctrl" | "control" => Some(Key::Control),
        "shift"            => Some(Key::Shift),
        "alt"              => Some(Key::Alt),
        "win" | "super" | "meta" | "cmd" | "command" => Some(Key::Meta),
        _                  => None,
    }
}

fn parse_key(s: &str) -> Option<Key> {
    // First try modifier names (e.g. a solo "ctrl" press)
    if let Some(k) = parse_modifier(s) {
        return Some(k);
    }

    match s.to_lowercase().as_str() {
        "enter" | "return" => Some(Key::Return),
        "escape" | "esc"   => Some(Key::Escape),
        "tab"              => Some(Key::Tab),
        "space"            => Some(Key::Space),
        "backspace"        => Some(Key::Backspace),
        "delete" | "del"   => Some(Key::Delete),
        "home"             => Some(Key::Home),
        "end"              => Some(Key::End),
        "pageup"  | "pgup" => Some(Key::PageUp),
        "pagedown"| "pgdn" => Some(Key::PageDown),
        "up"               => Some(Key::UpArrow),
        "down"             => Some(Key::DownArrow),
        "left"             => Some(Key::LeftArrow),
        "right"            => Some(Key::RightArrow),
        "f1"               => Some(Key::F1),
        "f2"               => Some(Key::F2),
        "f3"               => Some(Key::F3),
        "f4"               => Some(Key::F4),
        "f5"               => Some(Key::F5),
        "f6"               => Some(Key::F6),
        "f7"               => Some(Key::F7),
        "f8"               => Some(Key::F8),
        "f9"               => Some(Key::F9),
        "f10"              => Some(Key::F10),
        "f11"              => Some(Key::F11),
        "f12"              => Some(Key::F12),
        other => {
            // Single character — use Key::Layout
            let mut chars = other.chars();
            let ch = chars.next()?;
            if chars.next().is_none() {
                Some(Key::Layout(ch))
            } else {
                None
            }
        }
    }
}
