use regex::Regex;

use super::types::{FontDownloadError, FontInfo};

/// Parse Google Fonts CSS to extract ALL font URLs (for all weights).
pub fn parse_all_font_variants(
    css: &str,
    family: &str,
) -> Result<Vec<FontInfo>, FontDownloadError> {
    let font_face_re = Regex::new(r"@font-face\s*\{([^}]+)\}").unwrap();
    let url_re = Regex::new(r"src:\s*url\(([^)]+)\)").unwrap();
    // Match font-family with or without quotes
    let family_re = Regex::new(r#"font-family:\s*['"]([^'"]+)['"]"#).unwrap();
    let weight_re = Regex::new(r"font-weight:\s*(\d+)").unwrap();
    let style_re = Regex::new(r"font-style:\s*(\w+)").unwrap();

    let mut fonts = Vec::new();
    let mut font_face_count = 0;

    for cap in font_face_re.captures_iter(css) {
        font_face_count += 1;
        let block = &cap[1];

        // Check if this is for our font family
        if let Some(family_cap) = family_re.captures(block) {
            let found_family = family_cap[1].trim();
            if found_family.to_lowercase() != family.to_lowercase() {
                continue;
            }
        } else {
            continue;
        }

        // Extract URL
        if let Some(url_cap) = url_re.captures(block) {
            let url = url_cap[1].trim().trim_matches('"').trim_matches('\'');

            // Extract weight (default to 400)
            let weight = weight_re
                .captures(block)
                .and_then(|c| c[1].parse().ok())
                .unwrap_or(400);

            // Extract style (default to normal)
            let style = style_re
                .captures(block)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| "normal".to_string());

            fonts.push(FontInfo {
                family: family.to_string(),
                weight,
                style,
                url: url.to_string(),
            });
        }
    }

    if fonts.is_empty() {
        return Err(FontDownloadError::ParseError(format!(
            "Could not find any font URLs for family '{}' in CSS (found {} font-face blocks)",
            family, font_face_count
        )));
    }

    Ok(fonts)
}

/// Parse Google Fonts CSS to extract font URL (single font, prefers weight 400).
pub(super) fn parse_google_fonts_css(
    css: &str,
    family: &str,
) -> Result<FontInfo, FontDownloadError> {
    // Pattern to match @font-face blocks
    let font_face_re = Regex::new(r"@font-face\s*\{([^}]+)\}").unwrap();
    let url_re = Regex::new(r"src:\s*url\(([^)]+)\)").unwrap();
    let family_re = Regex::new(r#"font-family:\s*['"]?([^'";\n]+)['"]?"#).unwrap();
    let weight_re = Regex::new(r"font-weight:\s*(\d+)").unwrap();
    let style_re = Regex::new(r"font-style:\s*(\w+)").unwrap();

    for cap in font_face_re.captures_iter(css) {
        let block = &cap[1];

        // Check if this is for our font family
        if let Some(family_cap) = family_re.captures(block) {
            let found_family = family_cap[1].trim();
            if found_family.to_lowercase() != family.to_lowercase() {
                continue;
            }
        } else {
            continue;
        }

        // Extract URL
        if let Some(url_cap) = url_re.captures(block) {
            let url = url_cap[1].trim().trim_matches('"').trim_matches('\'');

            // Extract weight (default to 400)
            let weight = weight_re
                .captures(block)
                .and_then(|c| c[1].parse().ok())
                .unwrap_or(400);

            // Extract style (default to normal)
            let style = style_re
                .captures(block)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| "normal".to_string());

            // Prefer .ttf files, but accept others
            // For weight 400 and normal style (most common)
            if weight == 400 && style == "normal" {
                return Ok(FontInfo {
                    family: family.to_string(),
                    weight,
                    style,
                    url: url.to_string(),
                });
            }
        }
    }

    // If no 400 normal found, take the first one
    for cap in font_face_re.captures_iter(css) {
        let block = &cap[1];

        if let Some(family_cap) = family_re.captures(block) {
            let found_family = family_cap[1].trim();
            if found_family.to_lowercase() != family.to_lowercase() {
                continue;
            }
        } else {
            continue;
        }

        if let Some(url_cap) = url_re.captures(block) {
            let url = url_cap[1].trim().trim_matches('"').trim_matches('\'');
            let weight = weight_re
                .captures(block)
                .and_then(|c| c[1].parse().ok())
                .unwrap_or(400);
            let style = style_re
                .captures(block)
                .map(|c| c[1].to_string())
                .unwrap_or_else(|| "normal".to_string());

            return Ok(FontInfo {
                family: family.to_string(),
                weight,
                style,
                url: url.to_string(),
            });
        }
    }

    Err(FontDownloadError::ParseError(format!(
        "Could not find font URL for family '{}' in CSS",
        family
    )))
}
