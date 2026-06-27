//! Font Awesome 6 Free icon data shared by the tray renderer and settings UI.
//!
//! Icons live across three fonts (Regular, Solid, Brands). The same icon name
//! can exist in more than one font, so a configured icon is identified by a
//! `"<style>:<name>"` spec (e.g. `"solid:house"`). Glyphs are addressed by their
//! Private Use Area codepoints parsed from the `.codepoints` files generated
//! alongside the fonts; neither `ab_glyph` nor `egui` perform ligature shaping.

use std::collections::HashMap;
use std::sync::OnceLock;

pub const FA_REGULAR_TTF: &[u8] = include_bytes!("../assets/fonts/fa-regular-400.ttf");
pub const FA_SOLID_TTF: &[u8] = include_bytes!("../assets/fonts/fa-solid-900.ttf");
pub const FA_BRANDS_TTF: &[u8] = include_bytes!("../assets/fonts/fa-brands-400.ttf");

const REGULAR_CODEPOINTS: &str = include_str!("../assets/fonts/fa-regular.codepoints");
const SOLID_CODEPOINTS: &str = include_str!("../assets/fonts/fa-solid.codepoints");
const BRANDS_CODEPOINTS: &str = include_str!("../assets/fonts/fa-brands.codepoints");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IconStyle {
    Regular,
    Solid,
    Brands,
}

impl IconStyle {
    pub const ALL: [IconStyle; 3] = [IconStyle::Regular, IconStyle::Solid, IconStyle::Brands];

    /// Stable index used to address the per-style font arrays.
    pub fn index(self) -> usize {
        match self {
            IconStyle::Regular => 0,
            IconStyle::Solid => 1,
            IconStyle::Brands => 2,
        }
    }

    /// Short tag persisted in config specs.
    pub fn tag(self) -> &'static str {
        match self {
            IconStyle::Regular => "regular",
            IconStyle::Solid => "solid",
            IconStyle::Brands => "brands",
        }
    }

    /// Human-readable label for the UI.
    pub fn label(self) -> &'static str {
        match self {
            IconStyle::Regular => "Regular",
            IconStyle::Solid => "Solid",
            IconStyle::Brands => "Brands",
        }
    }

    /// egui font family name registered for this style.
    pub fn family(self) -> &'static str {
        match self {
            IconStyle::Regular => "fa-regular",
            IconStyle::Solid => "fa-solid",
            IconStyle::Brands => "fa-brands",
        }
    }

    pub fn font_ttf(self) -> &'static [u8] {
        match self {
            IconStyle::Regular => FA_REGULAR_TTF,
            IconStyle::Solid => FA_SOLID_TTF,
            IconStyle::Brands => FA_BRANDS_TTF,
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "regular" => Some(IconStyle::Regular),
            "solid" => Some(IconStyle::Solid),
            "brands" => Some(IconStyle::Brands),
            _ => None,
        }
    }

    fn codepoints(self) -> &'static str {
        match self {
            IconStyle::Regular => REGULAR_CODEPOINTS,
            IconStyle::Solid => SOLID_CODEPOINTS,
            IconStyle::Brands => BRANDS_CODEPOINTS,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Icon {
    pub name: &'static str,
    pub ch: char,
    pub style: IconStyle,
}

impl Icon {
    /// Config spec identifying this icon (`"<style>:<name>"`).
    pub fn spec(&self) -> String {
        spec(self.style, self.name)
    }
}

/// A resolved icon ready to render: glyph char plus the font to render it with.
#[derive(Debug, Clone, Copy)]
pub struct IconRef {
    pub ch: char,
    pub style: IconStyle,
}

static ICONS: OnceLock<Vec<Icon>> = OnceLock::new();
static BY_SPEC: OnceLock<HashMap<String, char>> = OnceLock::new();

/// Full icon list, sorted by name then style.
pub fn icons() -> &'static [Icon] {
    ICONS
        .get_or_init(|| {
            let mut list = Vec::new();
            for style in IconStyle::ALL {
                for (name, ch) in style.codepoints().lines().filter_map(parse_line) {
                    list.push(Icon { name, ch, style });
                }
            }
            list.sort_by(|a, b| {
                a.name
                    .cmp(b.name)
                    .then(a.style.index().cmp(&b.style.index()))
            });
            list
        })
        .as_slice()
}

/// Builds the persisted config spec for a style/name pair.
pub fn spec(style: IconStyle, name: &str) -> String {
    format!("{}:{name}", style.tag())
}

/// Resolves a config spec (`"<style>:<name>"`) into a renderable icon.
pub fn resolve(spec: &str) -> Option<IconRef> {
    let (tag, _) = spec.split_once(':')?;
    let style = IconStyle::from_tag(tag)?;
    let map = BY_SPEC.get_or_init(|| icons().iter().map(|icon| (icon.spec(), icon.ch)).collect());
    map.get(spec).map(|&ch| IconRef { ch, style })
}

fn parse_line(line: &'static str) -> Option<(&'static str, char)> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let (name, hex) = line.split_once(' ')?;
    let code = u32::from_str_radix(hex.trim(), 16).ok()?;
    let ch = char::from_u32(code)?;
    Some((name, ch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_icon() {
        let icon = icons()
            .iter()
            .find(|i| i.style == IconStyle::Solid)
            .unwrap();
        let resolved = resolve(&icon.spec()).expect("spec resolves");
        assert_eq!(resolved.ch, icon.ch);
        assert_eq!(resolved.style, IconStyle::Solid);
    }

    #[test]
    fn icon_list_has_all_styles() {
        let list = icons();
        assert!(list.iter().any(|i| i.style == IconStyle::Regular));
        assert!(list.iter().any(|i| i.style == IconStyle::Solid));
        assert!(list.iter().any(|i| i.style == IconStyle::Brands));
        assert!(list.len() > 1000);
    }

    #[test]
    fn unknown_spec_resolves_to_none() {
        assert!(resolve("solid:definitely_not_an_icon_xyz").is_none());
        assert!(resolve("no_style_separator").is_none());
        assert!(resolve("bogus:house").is_none());
    }
}
