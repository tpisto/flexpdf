//! Google Fonts downloader and cache.
//! Downloads fonts from Google Fonts URLs and caches them locally.

mod cache;
mod css;
mod download;
mod types;
mod url;

pub use css::parse_all_font_variants;
pub use download::{download_all_font_variants, download_google_font, download_google_font_async};
pub use types::{FontDownloadError, FontInfo};
pub use url::{
    extract_family_from_url,
    font_name_to_google_url,
    is_google_fonts_url,
    is_url,
    resolve_font_family,
    resolve_to_google_url,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_family_from_url() {
        assert_eq!(
            extract_family_from_url("https://fonts.googleapis.com/css2?family=Roboto"),
            Some("Roboto".to_string())
        );
        assert_eq!(
            extract_family_from_url("https://fonts.googleapis.com/css2?family=Open+Sans"),
            Some("Open Sans".to_string())
        );
        assert_eq!(
            extract_family_from_url("https://fonts.googleapis.com/css2?family=Roboto:wght@400;700"),
            Some("Roboto".to_string())
        );
    }

    #[test]
    fn test_is_google_fonts_url() {
        assert!(is_google_fonts_url(
            "https://fonts.googleapis.com/css2?family=Roboto"
        ));
        assert!(!is_google_fonts_url("Roboto"));
        assert!(!is_google_fonts_url("Arial"));
    }
}
