/// Platform-neutral hotkey events and the launcher shortcut spec.
///
/// UI and core must not import a concrete global-hotkey crate.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HotkeyEvent {
    ToggleLauncher,
}

/// Implemented by each OS. Constructed on the UI/main thread.
pub trait GlobalHotkeyService: Send {
    fn poll(&self) -> Option<HotkeyEvent>;
    /// Replace the registered launcher shortcut. Must run on the main thread.
    fn set_hotkey(&mut self, spec: &LauncherHotkey) -> Result<(), String>;
    /// Drop the current shortcut so Settings can record a replacement.
    fn clear_hotkey(&mut self) -> Result<(), String>;
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum HotkeySpecError {
    #[error("shortcut must include Option, Control, or Command")]
    MissingModifier,
    #[error("couldn't recognize \"{0}\" as a shortcut key")]
    UnsupportedKey(String),
    #[error("invalid shortcut")]
    InvalidFormat,
}

/// Global shortcut that toggles the launcher. Stored in `config.json` as
/// `alt+Space`, `super+shift+KeyK`, and similar.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LauncherHotkey {
    shift: bool,
    control: bool,
    alt: bool,
    super_key: bool,
    key: &'static str,
}

impl Default for LauncherHotkey {
    fn default() -> Self {
        Self {
            shift: false,
            control: false,
            alt: true,
            super_key: false,
            key: "Space",
        }
    }
}

impl LauncherHotkey {
    pub fn parse(input: &str) -> Result<Self, HotkeySpecError> {
        let mut shift = false;
        let mut control = false;
        let mut alt = false;
        let mut super_key = false;
        let mut key = None;
        let mut saw_token = false;
        for raw in input.split('+') {
            let token = raw.trim();
            if token.is_empty() {
                if saw_token || input.contains('+') {
                    return Err(HotkeySpecError::InvalidFormat);
                }
                continue;
            }
            saw_token = true;
            if key.is_some() {
                return Err(HotkeySpecError::InvalidFormat);
            }
            match token.to_ascii_uppercase().as_str() {
                "OPTION" | "ALT" => alt = true,
                "CONTROL" | "CTRL" => control = true,
                "COMMAND" | "CMD" | "SUPER" | "META" => super_key = true,
                "SHIFT" => shift = true,
                other => {
                    key = Some(
                        canonical_key(other)
                            .ok_or_else(|| HotkeySpecError::UnsupportedKey(token.to_string()))?,
                    );
                }
            }
        }
        let Some(key) = key else {
            return Err(HotkeySpecError::InvalidFormat);
        };
        if !alt && !control && !super_key {
            return Err(HotkeySpecError::MissingModifier);
        }
        Ok(Self {
            shift,
            control,
            alt,
            super_key,
            key,
        })
    }

    /// Canonical spec for `config.json` and the platform hotkey crate.
    pub fn to_spec(&self) -> String {
        let mut spec = String::new();
        if self.shift {
            spec.push_str("shift+");
        }
        if self.control {
            spec.push_str("control+");
        }
        if self.alt {
            spec.push_str("alt+");
        }
        if self.super_key {
            spec.push_str("super+");
        }
        spec.push_str(self.key);
        spec
    }

    /// macOS-style labels for `<kbd>` rendering: Control, Option, Shift, Command, key.
    pub fn display_tokens(&self) -> Vec<String> {
        let mut tokens = Vec::new();
        if self.control {
            tokens.push("Control".into());
        }
        if self.alt {
            tokens.push("Option".into());
        }
        if self.shift {
            tokens.push("Shift".into());
        }
        if self.super_key {
            tokens.push("Command".into());
        }
        tokens.push(key_label(self.key).into());
        tokens
    }

    pub fn view(&self) -> HotkeyView {
        HotkeyView {
            spec: self.to_spec(),
            tokens: self.display_tokens(),
        }
    }
}

impl Serialize for LauncherHotkey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_spec())
    }
}

impl<'de> Deserialize<'de> for LauncherHotkey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse(&raw).unwrap_or_default())
    }
}

/// UI-safe shortcut description. `spec` is what Settings writes back.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HotkeyView {
    pub spec: String,
    pub tokens: Vec<String>,
}

fn canonical_key(upper: &str) -> Option<&'static str> {
    Some(match upper {
        "BACKQUOTE" | "`" => "Backquote",
        "BACKSLASH" | "\\" => "Backslash",
        "BRACKETLEFT" | "[" => "BracketLeft",
        "BRACKETRIGHT" | "]" => "BracketRight",
        "COMMA" | "," => "Comma",
        "DIGIT0" | "0" => "Digit0",
        "DIGIT1" | "1" => "Digit1",
        "DIGIT2" | "2" => "Digit2",
        "DIGIT3" | "3" => "Digit3",
        "DIGIT4" | "4" => "Digit4",
        "DIGIT5" | "5" => "Digit5",
        "DIGIT6" | "6" => "Digit6",
        "DIGIT7" | "7" => "Digit7",
        "DIGIT8" | "8" => "Digit8",
        "DIGIT9" | "9" => "Digit9",
        "EQUAL" | "=" => "Equal",
        "KEYA" | "A" => "KeyA",
        "KEYB" | "B" => "KeyB",
        "KEYC" | "C" => "KeyC",
        "KEYD" | "D" => "KeyD",
        "KEYE" | "E" => "KeyE",
        "KEYF" | "F" => "KeyF",
        "KEYG" | "G" => "KeyG",
        "KEYH" | "H" => "KeyH",
        "KEYI" | "I" => "KeyI",
        "KEYJ" | "J" => "KeyJ",
        "KEYK" | "K" => "KeyK",
        "KEYL" | "L" => "KeyL",
        "KEYM" | "M" => "KeyM",
        "KEYN" | "N" => "KeyN",
        "KEYO" | "O" => "KeyO",
        "KEYP" | "P" => "KeyP",
        "KEYQ" | "Q" => "KeyQ",
        "KEYR" | "R" => "KeyR",
        "KEYS" | "S" => "KeyS",
        "KEYT" | "T" => "KeyT",
        "KEYU" | "U" => "KeyU",
        "KEYV" | "V" => "KeyV",
        "KEYW" | "W" => "KeyW",
        "KEYX" | "X" => "KeyX",
        "KEYY" | "Y" => "KeyY",
        "KEYZ" | "Z" => "KeyZ",
        "MINUS" | "-" => "Minus",
        "PERIOD" | "." => "Period",
        "QUOTE" | "'" => "Quote",
        "SEMICOLON" | ";" => "Semicolon",
        "SLASH" | "/" => "Slash",
        "BACKSPACE" => "Backspace",
        "ENTER" | "RETURN" => "Enter",
        "SPACE" => "Space",
        "TAB" => "Tab",
        "DELETE" => "Delete",
        "END" => "End",
        "HOME" => "Home",
        "PAGEDOWN" => "PageDown",
        "PAGEUP" => "PageUp",
        "ARROWDOWN" | "DOWN" => "ArrowDown",
        "ARROWLEFT" | "LEFT" => "ArrowLeft",
        "ARROWRIGHT" | "RIGHT" => "ArrowRight",
        "ARROWUP" | "UP" => "ArrowUp",
        "ESCAPE" | "ESC" => "Escape",
        "F1" => "F1",
        "F2" => "F2",
        "F3" => "F3",
        "F4" => "F4",
        "F5" => "F5",
        "F6" => "F6",
        "F7" => "F7",
        "F8" => "F8",
        "F9" => "F9",
        "F10" => "F10",
        "F11" => "F11",
        "F12" => "F12",
        "F13" => "F13",
        "F14" => "F14",
        "F15" => "F15",
        "F16" => "F16",
        "F17" => "F17",
        "F18" => "F18",
        "F19" => "F19",
        "F20" => "F20",
        _ => return None,
    })
}

fn key_label(key: &str) -> &str {
    match key {
        "Space" => "Space",
        "Enter" => "Return",
        "Escape" => "Esc",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "PageUp" => "Page Up",
        "PageDown" => "Page Down",
        "Backspace" => "Delete",
        "Delete" => "Fwd Del",
        "Minus" => "-",
        "Equal" => "=",
        "Comma" => ",",
        "Period" => ".",
        "Slash" => "/",
        "Semicolon" => ";",
        "Quote" => "'",
        "Backquote" => "`",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Backslash" => "\\",
        "Digit0" => "0",
        "Digit1" => "1",
        "Digit2" => "2",
        "Digit3" => "3",
        "Digit4" => "4",
        "Digit5" => "5",
        "Digit6" => "6",
        "Digit7" => "7",
        "Digit8" => "8",
        "Digit9" => "9",
        "KeyA" => "A",
        "KeyB" => "B",
        "KeyC" => "C",
        "KeyD" => "D",
        "KeyE" => "E",
        "KeyF" => "F",
        "KeyG" => "G",
        "KeyH" => "H",
        "KeyI" => "I",
        "KeyJ" => "J",
        "KeyK" => "K",
        "KeyL" => "L",
        "KeyM" => "M",
        "KeyN" => "N",
        "KeyO" => "O",
        "KeyP" => "P",
        "KeyQ" => "Q",
        "KeyR" => "R",
        "KeyS" => "S",
        "KeyT" => "T",
        "KeyU" => "U",
        "KeyV" => "V",
        "KeyW" => "W",
        "KeyX" => "X",
        "KeyY" => "Y",
        "KeyZ" => "Z",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_option_space() {
        let hotkey = LauncherHotkey::default();
        assert_eq!(hotkey.to_spec(), "alt+Space");
        assert_eq!(hotkey.display_tokens(), ["Option", "Space"]);
    }

    #[test]
    fn parses_aliases_to_the_same_spec() {
        let a = LauncherHotkey::parse("option+space").unwrap();
        let b = LauncherHotkey::parse("alt+Space").unwrap();
        let c = LauncherHotkey::parse("ALT + SPACE").unwrap();
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(a.to_spec(), "alt+Space");
    }

    #[test]
    fn parses_command_shift_letter() {
        let hotkey = LauncherHotkey::parse("cmd+shift+k").unwrap();
        assert_eq!(hotkey.to_spec(), "shift+super+KeyK");
        assert_eq!(hotkey.display_tokens(), ["Shift", "Command", "K"]);
    }

    #[test]
    fn rejects_shortcut_without_a_real_modifier() {
        assert_eq!(
            LauncherHotkey::parse("space"),
            Err(HotkeySpecError::MissingModifier)
        );
        assert_eq!(
            LauncherHotkey::parse("shift+space"),
            Err(HotkeySpecError::MissingModifier)
        );
        assert!(LauncherHotkey::parse("").is_err());
        assert!(LauncherHotkey::parse("alt+").is_err());
        assert!(LauncherHotkey::parse("alt+space+k").is_err());
    }

    #[test]
    fn serde_roundtrip_and_invalid_fallback() {
        let hotkey = LauncherHotkey::parse("control+alt+KeyK").unwrap();
        let json = serde_json::to_string(&hotkey).unwrap();
        assert_eq!(json, "\"control+alt+KeyK\"");
        let loaded: LauncherHotkey = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, hotkey);
        let fallback: LauncherHotkey = serde_json::from_str("\"not-a-key\"").unwrap();
        assert_eq!(fallback, LauncherHotkey::default());
    }
}
