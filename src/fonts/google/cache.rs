use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Cache directory for downloaded fonts.
pub(super) fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("flexpdf")
        .join("fonts")
}

/// In-memory cache of loaded fonts (font family name -> font data).
static FONT_CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();

pub(super) fn get_font_cache() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    FONT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Sanitize a string for use as a filename.
pub(super) fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
