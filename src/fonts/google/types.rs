/// Parsed font information from Google Fonts CSS.
#[derive(Debug, Clone)]
pub struct FontInfo {
    pub family: String,
    pub weight: u32,
    pub style: String, // "normal" or "italic"
    pub url: String,
}

#[derive(Debug)]
pub enum FontDownloadError {
    InvalidUrl(String),
    Network(String),
    Io(std::io::Error),
    ParseError(String),
}

impl std::fmt::Display for FontDownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontDownloadError::InvalidUrl(url) => write!(f, "Invalid Google Fonts URL: {}", url),
            FontDownloadError::Network(e) => write!(f, "Network error: {}", e),
            FontDownloadError::Io(e) => write!(f, "I/O error: {}", e),
            FontDownloadError::ParseError(e) => write!(f, "Parse error: {}", e),
        }
    }
}

impl std::error::Error for FontDownloadError {}
