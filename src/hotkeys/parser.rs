use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        win: false,
    };

    pub fn from_parts(ctrl: bool, alt: bool, shift: bool, win: bool) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            win,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    pub modifiers: Modifiers,
    #[serde(with = "vk_serde")]
    pub key: VIRTUAL_KEY,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub display: String,
}

mod vk_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;

    pub fn serialize<S>(vk: &VIRTUAL_KEY, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u16(vk.0)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VIRTUAL_KEY, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code = u16::deserialize(deserializer)?;
        Ok(VIRTUAL_KEY(code))
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum HotkeyParseError {
    #[error("empty hotkey")]
    Empty,
    #[error("unknown key token: {0}")]
    UnknownToken(String),
    #[error("missing key after modifiers")]
    MissingKey,
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display)
    }
}

impl Hotkey {
    pub fn parse(input: &str) -> Result<Self, HotkeyParseError> {
        let tokens: Vec<&str> = input
            .split('+')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if tokens.is_empty() {
            return Err(HotkeyParseError::Empty);
        }

        let mut modifiers = Modifiers::NONE;
        let mut key_token: Option<&str> = None;

        for token in tokens {
            match normalize_token(token) {
                Some(ModifierToken::Ctrl) => modifiers.ctrl = true,
                Some(ModifierToken::Alt) => modifiers.alt = true,
                Some(ModifierToken::Shift) => modifiers.shift = true,
                Some(ModifierToken::Win) => modifiers.win = true,
                None => {
                    if key_token.is_some() {
                        return Err(HotkeyParseError::UnknownToken(token.to_string()));
                    }
                    key_token = Some(token);
                }
            }
        }

        let key_token = key_token.ok_or(HotkeyParseError::MissingKey)?;
        let key = token_to_vk(key_token)
            .ok_or_else(|| HotkeyParseError::UnknownToken(key_token.to_string()))?;
        let display = format_hotkey(&modifiers, key_token);

        Ok(Self {
            modifiers,
            key,
            display,
        })
    }

    pub fn normalize(input: &str) -> Result<String, HotkeyParseError> {
        Ok(Self::parse(input)?.display)
    }
}

enum ModifierToken {
    Ctrl,
    Alt,
    Shift,
    Win,
}

fn normalize_token(token: &str) -> Option<ModifierToken> {
    match token.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => Some(ModifierToken::Ctrl),
        "alt" => Some(ModifierToken::Alt),
        "shift" => Some(ModifierToken::Shift),
        "win" | "super" | "meta" | "windows" => Some(ModifierToken::Win),
        _ => None,
    }
}

fn token_to_vk(token: &str) -> Option<VIRTUAL_KEY> {
    let upper = token.to_ascii_uppercase();
    if upper.len() == 1 {
        let ch = upper.chars().next()?;
        if ch.is_ascii_uppercase() {
            return Some(VIRTUAL_KEY(ch as u16));
        }
        if ch.is_ascii_digit() {
            return Some(VIRTUAL_KEY(ch as u16));
        }
    }

    match upper.as_str() {
        "F1" => Some(VIRTUAL_KEY(0x70)),
        "F2" => Some(VIRTUAL_KEY(0x71)),
        "F3" => Some(VIRTUAL_KEY(0x72)),
        "F4" => Some(VIRTUAL_KEY(0x73)),
        "F5" => Some(VIRTUAL_KEY(0x74)),
        "F6" => Some(VIRTUAL_KEY(0x75)),
        "F7" => Some(VIRTUAL_KEY(0x76)),
        "F8" => Some(VIRTUAL_KEY(0x77)),
        "F9" => Some(VIRTUAL_KEY(0x78)),
        "F10" => Some(VIRTUAL_KEY(0x79)),
        "F11" => Some(VIRTUAL_KEY(0x7A)),
        "F12" => Some(VIRTUAL_KEY(0x7B)),
        "TAB" => Some(VIRTUAL_KEY(0x09)),
        "SPACE" => Some(VIRTUAL_KEY(0x20)),
        "ENTER" | "RETURN" => Some(VIRTUAL_KEY(0x0D)),
        "ESCAPE" | "ESC" => Some(VIRTUAL_KEY(0x1B)),
        "LEFT" => Some(VIRTUAL_KEY(0x25)),
        "RIGHT" => Some(VIRTUAL_KEY(0x27)),
        "UP" => Some(VIRTUAL_KEY(0x26)),
        "DOWN" => Some(VIRTUAL_KEY(0x28)),
        _ => None,
    }
}

fn format_hotkey(modifiers: &Modifiers, key_token: &str) -> String {
    let mut parts = Vec::new();
    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.win {
        parts.push("Win");
    }
    parts.push(key_token);
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_win_number() {
        let hk = Hotkey::parse("Win+2").unwrap();
        assert!(hk.modifiers.win);
        assert_eq!(hk.key, VIRTUAL_KEY('2' as u16));
        assert_eq!(hk.display, "Win+2");
    }

    #[test]
    fn parse_win_shift_number() {
        let hk = Hotkey::parse("Win+Shift+6").unwrap();
        assert!(hk.modifiers.win);
        assert!(hk.modifiers.shift);
        assert_eq!(hk.key, VIRTUAL_KEY('6' as u16));
    }

    #[test]
    fn normalize_super_alias() {
        let hk = Hotkey::parse("Super+3").unwrap();
        assert!(hk.modifiers.win);
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(Hotkey::parse(""), Err(HotkeyParseError::Empty));
    }
}
