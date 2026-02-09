use std::collections::HashMap;
use std::fs;
use std::sync::MutexGuard;
use std::time::Duration;

use super::cache::{cache_dir, get_font_cache, sanitize_filename};
use super::css::{parse_all_font_variants, parse_google_fonts_css};
use super::types::FontDownloadError;
use super::url::{extract_family_from_url, resolve_to_google_url};

/// Download ALL font variants from Google Fonts (all weights + italic).
/// Returns a vector of (font_data, weight, is_italic) tuples.
pub fn download_all_font_variants(
    font_spec: &str,
) -> Result<Vec<(Vec<u8>, u32, bool)>, FontDownloadError> {
    let url = resolve_to_google_url(font_spec);
    let family = extract_family_from_url(&url)
        .ok_or_else(|| FontDownloadError::InvalidUrl(url.clone()))?;

    log::info!("Downloading all variants for font: {}", family);

    let client = blocking_client()?;
    let css = fetch_google_fonts_css_blocking(&client, &url)?;

    // Parse ALL font variants from CSS
    let font_infos = parse_all_font_variants(&css, &family)?;

    log::info!("Found {} font variants", font_infos.len());

    let mut results = Vec::new();
    for info in font_infos {
        // Check cache first
        let cache_key = format!("{}-{}-{}", family, info.weight, info.style);
        let cache_path = cache_dir().join(format!("{}.ttf", sanitize_filename(&cache_key)));

        let font_data = if cache_path.exists() {
            fs::read(&cache_path).map_err(FontDownloadError::Io)?
        } else {
            let data = download_font_file_blocking(&client, &info.url)?;
            // Cache to disk
            let _ = fs::create_dir_all(cache_dir());
            let _ = fs::write(&cache_path, &data);
            data
        };

        let is_italic = info.style == "italic";
        results.push((font_data, info.weight, is_italic));
    }

    Ok(results)
}

/// Download font from Google Fonts (single variant, for backwards compatibility).
/// Accepts either a font name (e.g., "Roboto") or a full Google Fonts URL.
/// Returns the font data bytes.
pub fn download_google_font(font_spec: &str) -> Result<Vec<u8>, FontDownloadError> {
    // Resolve to a Google Fonts URL if it's just a font name
    let url = resolve_to_google_url(font_spec);

    // Extract family name for caching
    let family = extract_family_from_url(&url)
        .ok_or_else(|| FontDownloadError::InvalidUrl(url.clone()))?;

    // Check in-memory cache first
    {
        let cache = lock_cache();
        if let Some(data) = cache.get(&family) {
            return Ok(data.clone());
        }
    }

    // Check disk cache
    let cache_path = cache_dir().join(format!("{}.ttf", sanitize_filename(&family)));
    if cache_path.exists() {
        let data = fs::read(&cache_path).map_err(FontDownloadError::Io)?;
        // Store in memory cache
        {
            let mut cache = lock_cache();
            cache.insert(family.clone(), data.clone());
        }
        return Ok(data);
    }

    // Download from Google Fonts using blocking reqwest
    log::info!("Downloading font: {}", family);

    // Use blocking client for simplicity
    let client = blocking_client()?;

    // Fetch the CSS with User-Agent that requests TTF
    let css = fetch_google_fonts_css_blocking(&client, &url)?;

    // Parse CSS to get font URL
    let font_info = parse_google_fonts_css(&css, &family)?;

    // Download the actual font file
    let font_data = download_font_file_blocking(&client, &font_info.url)?;

    // Cache to disk
    if let Err(e) = fs::create_dir_all(cache_dir()) {
        log::warn!("Could not create cache directory: {}", e);
    }
    if let Err(e) = fs::write(&cache_path, &font_data) {
        log::warn!("Could not cache font: {}", e);
    }

    // Store in memory cache
    {
        let mut cache = lock_cache();
        cache.insert(family, font_data.clone());
    }

    Ok(font_data)
}

/// Async version of download_google_font using tokio and reqwest.
pub async fn download_google_font_async(url: &str) -> Result<Vec<u8>, FontDownloadError> {
    // Extract family name for caching
    let family = extract_family_from_url(url)
        .ok_or_else(|| FontDownloadError::InvalidUrl(url.to_string()))?;

    // Check in-memory cache first
    {
        let cache = lock_cache();
        if let Some(data) = cache.get(&family) {
            return Ok(data.clone());
        }
    }

    // Check disk cache
    let cache_path = cache_dir().join(format!("{}.ttf", sanitize_filename(&family)));
    if cache_path.exists() {
        let data = fs::read(&cache_path).map_err(FontDownloadError::Io)?;
        // Store in memory cache
        {
            let mut cache = lock_cache();
            cache.insert(family.clone(), data.clone());
        }
        return Ok(data);
    }

    // Download from Google Fonts
    log::info!("Downloading font: {}", family);

    let client = async_client()?;

    // Fetch the CSS with User-Agent that requests TTF
    let css = fetch_google_fonts_css_async(&client, url).await?;

    // Parse CSS to get font URL
    let font_info = parse_google_fonts_css(&css, &family)?;

    // Download the actual font file
    let font_data = download_font_file_async(&client, &font_info.url).await?;

    // Cache to disk
    if let Err(e) = fs::create_dir_all(cache_dir()) {
        log::warn!("Could not create cache directory: {}", e);
    }
    if let Err(e) = fs::write(&cache_path, &font_data) {
        log::warn!("Could not cache font: {}", e);
    }

    // Store in memory cache
    {
        let mut cache = lock_cache();
        cache.insert(family, font_data.clone());
    }

    Ok(font_data)
}

/// Fetch the Google Fonts CSS (blocking).
fn fetch_google_fonts_css_blocking(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<String, FontDownloadError> {
    match fetch_google_fonts_css_blocking_reqwest(client, url) {
        Ok(text) => Ok(text),
        Err(err) => {
            #[cfg(feature = "curl")]
            {
                log::warn!("Reqwest failed for Google Fonts CSS, falling back to curl: {}", err);
                return fetch_google_fonts_css_curl(url);
            }
            Err(err)
        }
    }
}

/// Fetch the Google Fonts CSS (async).
async fn fetch_google_fonts_css_async(
    client: &reqwest::Client,
    url: &str,
) -> Result<String, FontDownloadError> {
    // Use Wget User-Agent to get TTF format
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| FontDownloadError::Network(e.to_string()))?;

    response
        .text()
        .await
        .map_err(|e| FontDownloadError::Network(e.to_string()))
}

/// Download the actual font file (blocking).
fn download_font_file_blocking(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<Vec<u8>, FontDownloadError> {
    match download_font_file_blocking_reqwest(client, url) {
        Ok(bytes) => Ok(bytes),
        Err(err) => {
            #[cfg(feature = "curl")]
            {
                log::warn!("Reqwest failed for font file, falling back to curl: {}", err);
                return download_font_file_curl(url);
            }
            Err(err)
        }
    }
}

/// Download the actual font file (async).
async fn download_font_file_async(
    client: &reqwest::Client,
    url: &str,
) -> Result<Vec<u8>, FontDownloadError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| FontDownloadError::Network(e.to_string()))?;

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| FontDownloadError::Network(e.to_string()))
}

fn lock_cache() -> MutexGuard<'static, HashMap<String, Vec<u8>>> {
    match get_font_cache().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn blocking_client() -> Result<reqwest::blocking::Client, FontDownloadError> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Wget")
        .build()
        .map_err(|e| FontDownloadError::Network(e.to_string()))
}

fn async_client() -> Result<reqwest::Client, FontDownloadError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Wget")
        .build()
        .map_err(|e| FontDownloadError::Network(e.to_string()))
}

fn fetch_google_fonts_css_blocking_reqwest(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<String, FontDownloadError> {
    let response = client
        .get(url)
        .send()
        .map_err(|e| FontDownloadError::Network(e.to_string()))?;

    let text = response
        .text()
        .map_err(|e| FontDownloadError::Network(e.to_string()))?;

    if text.starts_with("<!DOCTYPE") || text.starts_with("<html") {
        return Err(FontDownloadError::ParseError("Got HTML instead of CSS".to_string()));
    }

    Ok(text)
}

fn download_font_file_blocking_reqwest(
    client: &reqwest::blocking::Client,
    url: &str,
) -> Result<Vec<u8>, FontDownloadError> {
    let response = client
        .get(url)
        .send()
        .map_err(|e| FontDownloadError::Network(e.to_string()))?;

    response
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|e| FontDownloadError::Network(e.to_string()))
}

#[cfg(feature = "curl")]
fn fetch_google_fonts_css_curl(url: &str) -> Result<String, FontDownloadError> {
    let output = std::process::Command::new("curl")
        .args(["-s", "-A", "Wget", url])
        .output()
        .map_err(|e| FontDownloadError::Network(format!("Failed to run curl: {}", e)))?;

    if !output.status.success() {
        return Err(FontDownloadError::Network(format!(
            "curl failed with status: {}",
            output.status
        )));
    }

    let text = String::from_utf8(output.stdout)
        .map_err(|e| FontDownloadError::Network(format!("Invalid UTF-8: {}", e)))?;

    if text.starts_with("<!DOCTYPE") || text.starts_with("<html") {
        return Err(FontDownloadError::ParseError("Got HTML instead of CSS".to_string()));
    }

    Ok(text)
}

#[cfg(feature = "curl")]
fn download_font_file_curl(url: &str) -> Result<Vec<u8>, FontDownloadError> {
    let output = std::process::Command::new("curl")
        .args(["-s", "-L", "-A", "Wget", url])
        .output()
        .map_err(|e| FontDownloadError::Network(format!("Failed to run curl: {}", e)))?;

    if !output.status.success() {
        return Err(FontDownloadError::Network(format!(
            "curl failed with status: {}",
            output.status
        )));
    }

    if output.stdout.is_empty() {
        return Err(FontDownloadError::Network("Empty response".to_string()));
    }

    Ok(output.stdout)
}
