use regex::Regex;

/// Check if a string looks like a Google Fonts URL.
pub fn is_google_fonts_url(s: &str) -> bool {
    s.contains("fonts.googleapis.com") || s.contains("fonts.gstatic.com")
}

/// Check if a string is a URL (starts with http:// or https://).
pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Convert a font family name to a Google Fonts URL with all weight and style variants.
/// e.g., "Roboto" -> "https://fonts.googleapis.com/css2?family=Roboto:ital,wght@0,100;0,200;...;1,100;1,200;..."
pub fn font_name_to_google_url(name: &str) -> String {
    let encoded_name = name.replace(' ', "+");
    // Request all weights in both normal (0) and italic (1) styles.
    // IMPORTANT: All normal weights first (0,xxx), then all italic weights (1,xxx).
    let weights = [100, 200, 300, 400, 500, 600, 700, 800, 900];
    let mut variants = Vec::new();
    // Normal weights first
    for &w in &weights {
        variants.push(format!("0,{}", w));
    }
    // Then italic weights
    for &w in &weights {
        variants.push(format!("1,{}", w));
    }
    format!(
        "https://fonts.googleapis.com/css2?family={}:ital,wght@{}",
        encoded_name,
        variants.join(";")
    )
}

/// Resolve a font family specification to a Google Fonts URL.
/// - If it's already a Google Fonts URL, return as-is.
/// - If it's a simple font name, convert to Google Fonts URL.
/// - If it's some other URL, return as-is (will likely fail).
pub fn resolve_to_google_url(font_spec: &str) -> String {
    if is_google_fonts_url(font_spec) {
        // Already a Google Fonts URL
        font_spec.to_string()
    } else if is_url(font_spec) {
        // Some other URL - return as-is
        font_spec.to_string()
    } else {
        // Simple font name - convert to Google Fonts URL
        font_name_to_google_url(font_spec)
    }
}

/// Extract font family name from a Google Fonts URL.
/// e.g., "https://fonts.googleapis.com/css2?family=Roboto" -> "Roboto".
pub fn extract_family_from_url(url: &str) -> Option<String> {
    // Pattern: family=FontName or family=Font+Name
    let re = Regex::new(r"family=([^&:]+)").ok()?;
    if let Some(caps) = re.captures(url) {
        let family = caps.get(1)?.as_str();
        // Replace + with space
        Some(family.replace('+', " "))
    } else {
        None
    }
}

/// Get the font family name to use in Parley.
/// For Google Fonts URLs, extracts and returns the family name.
/// For regular names, returns as-is.
pub fn resolve_font_family(family: &str) -> String {
    if is_google_fonts_url(family) {
        extract_family_from_url(family).unwrap_or_else(|| "Roboto".to_string())
    } else {
        family.to_string()
    }
}
