use std::collections::HashMap;

use crate::fonts::{FontKey, FontSystem};
use crate::style::FontStyle;

/// Find the best matching font from the font map.
pub(super) fn find_best_font(
    font_map: &HashMap<FontKey, String>,
    font_system: &FontSystem,
    font_family: Option<&str>,
    weight: u16,
    font_style: Option<FontStyle>,
) -> String {
    let key = font_system.resolve_font_key(font_family, Some(weight), font_style);
    if let Some(name) = font_map.get(&key) {
        return name.clone();
    }

    let mut best_name = "F1".to_string();
    let mut best_diff = u16::MAX;

    for (map_key, name) in font_map.iter() {
        if map_key.is_italic == key.is_italic {
            let diff = (map_key.weight as i32 - key.weight as i32).unsigned_abs() as u16;
            if diff < best_diff {
                best_diff = diff;
                best_name = name.clone();
            }
        }
    }

    if best_diff == u16::MAX {
        for (map_key, name) in font_map.iter() {
            let diff = (map_key.weight as i32 - key.weight as i32).unsigned_abs() as u16;
            if diff < best_diff {
                best_diff = diff;
                best_name = name.clone();
            }
        }
    }

    best_name
}
