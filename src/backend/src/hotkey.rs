use crate::commands::AppState;
use crate::tray;
use std::str::FromStr;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub const SHOW_WINDOW_COMMAND: &str = "app.showWindow";

pub const UNAVAILABLE_EVENT: &str = "mynote:hotkey-unavailable";

pub fn plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri_plugin_global_shortcut::Builder::new().build()
}

/// Registering the chord is also what earns MyNote the right to take the
/// foreground on Windows
pub fn sync(app: &AppHandle) {
    let manager = app.global_shortcut();
    if let Err(e) = manager.unregister_all() {
        log::warn!("failed to drop the previous global shortcut: {e}");
    }
    let Some(chord) = configured_chord(app) else {
        return;
    };
    let Some(shortcut) = parse_chord(&chord) else {
        log::warn!("configured global shortcut is not registrable system-wide");
        return;
    };
    match manager.on_shortcut(shortcut, |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            log::trace!("global shortcut pressed");
            tray::reveal_window(app);
        }
    }) {
        Ok(()) => log::trace!("global shortcut registered"),
        Err(e) => {
            log::warn!("global shortcut registration failed: {e}");
            let _ = app.emit(
                UNAVAILABLE_EVENT,
                "another app already owns that system-wide shortcut",
            );
        }
    }
}

fn configured_chord(app: &AppHandle) -> Option<String> {
    let state = app.state::<AppState>();
    let settings = state.settings.lock().ok()?;
    settings.keybindings.get(SHOW_WINDOW_COMMAND)?.first().cloned()
}

// Chords without a modifier are refused here as well as in the UI
fn parse_chord(chord: &str) -> Option<Shortcut> {
    let mut modifiers = Modifiers::empty();
    let mut key = None;
    for part in chord.split('+').map(|p| if p.is_empty() { "+" } else { p }) {
        match part {
            "Mod" => modifiers |= primary_modifier(),
            "Alt" => modifiers |= Modifiers::ALT,
            "Shift" => modifiers |= Modifiers::SHIFT,
            _ => key = Some(part),
        }
    }
    if modifiers.is_empty() || modifiers == Modifiers::SHIFT {
        return None;
    }
    let code = Code::from_str(&code_name(key?)).ok()?;
    Some(Shortcut::new(Some(modifiers), code))
}

fn primary_modifier() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    }
}

/// The W3C `code` name of the key half — the inverse of the US-QWERTY table
/// keys/platform.ts resolves chords through.
fn code_name(key: &str) -> String {
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return key.to_string();
    };
    if c.is_ascii_lowercase() {
        return format!("Key{}", c.to_ascii_uppercase());
    }
    if c.is_ascii_digit() {
        return format!("Digit{c}");
    }
    match c {
        '-' => "Minus",
        '=' => "Equal",
        '[' => "BracketLeft",
        ']' => "BracketRight",
        '\\' => "Backslash",
        ';' => "Semicolon",
        '\'' => "Quote",
        '`' => "Backquote",
        ',' => "Comma",
        '.' => "Period",
        '/' => "Slash",
        ' ' => "Space",
        _ => key,
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_modifier_chord_into_its_code() {
        let shortcut = parse_chord("Mod+Alt+n").expect("registrable");
        assert_eq!(shortcut.key, Code::KeyN);
        assert_eq!(shortcut.mods, primary_modifier() | Modifiers::ALT);
    }

    #[test]
    fn parses_named_and_punctuation_keys() {
        assert_eq!(parse_chord("Mod+Alt+PageUp").unwrap().key, Code::PageUp);
        assert_eq!(parse_chord("Mod+Alt+F3").unwrap().key, Code::F3);
        assert_eq!(parse_chord("Mod+Alt+,").unwrap().key, Code::Comma);
        assert_eq!(parse_chord("Mod+Alt+1").unwrap().key, Code::Digit1);
    }

    #[test]
    fn refuses_chords_a_user_could_never_escape() {
        assert!(parse_chord("n").is_none());
        assert!(parse_chord("Shift+n").is_none());
        assert!(parse_chord("Mod+Alt+nonsense").is_none());
        assert!(parse_chord("Mod+Alt").is_none());
    }
}
