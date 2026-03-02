//! Image loading and decoding.
//! Downloads images from URLs or loads from files, and prepares them for PDF embedding

use std::fs;
use std::time::Duration;

/// Loaded image data ready for PDF embedding
#[derive(Debug, Clone)]
pub struct LoadedImage {
    /// Raw image data (decoded pixels in RGB format)
    pub data: Vec<u8>,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Bits per component (usually 8)
    pub bits_per_component: u8,
    /// Color space name for PDF
    pub color_space: String,
}

/// Error type for image loading
#[derive(Debug)]
pub enum ImageError {
    Download(String),
    Decode(String),
    FileRead(String),
    UnsupportedFormat(String),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageError::Download(e) => write!(f, "Download error: {}", e),
            ImageError::Decode(e) => write!(f, "Decode error: {}", e),
            ImageError::FileRead(e) => write!(f, "File read error: {}", e),
            ImageError::UnsupportedFormat(e) => write!(f, "Unsupported format: {}", e),
        }
    }
}

impl std::error::Error for ImageError {}

/// Load an image from a URL or file path
pub fn load_image(src: &str) -> Result<LoadedImage, ImageError> {
    let data = if src.starts_with("http://") || src.starts_with("https://") {
        download_image(src)?
    } else {
        load_image_file(src)?
    };

    decode_image(&data)
}

/// Download image from URL
fn download_image(url: &str) -> Result<Vec<u8>, ImageError> {
    log::info!("Downloading image: {}", url);

    match download_image_reqwest(url) {
        Ok(bytes) => Ok(bytes),
        Err(err) => {
            #[cfg(feature = "curl")]
            {
                log::warn!("Reqwest failed for image, falling back to curl: {}", err);
                return download_image_curl(url);
            }
            Err(err)
        }
    }
}

/// Load image from file
fn load_image_file(path: &str) -> Result<Vec<u8>, ImageError> {
    log::info!("Loading image file: {}", path);
    fs::read(path).map_err(|e| ImageError::FileRead(format!("{}: {}", path, e)))
}

fn download_image_reqwest(url: &str) -> Result<Vec<u8>, ImageError> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Mozilla/5.0")
        .build()
        .map_err(|e| ImageError::Download(e.to_string()))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| ImageError::Download(e.to_string()))?;

    response
        .bytes()
        .map(|b| b.to_vec())
        .map_err(|e| ImageError::Download(e.to_string()))
}

#[cfg(feature = "curl")]
fn download_image_curl(url: &str) -> Result<Vec<u8>, ImageError> {
    let output = std::process::Command::new("curl")
        .args(["-s", "-L", "-A", "Mozilla/5.0", url])
        .output()
        .map_err(|e| ImageError::Download(format!("Failed to run curl: {}", e)))?;

    if !output.status.success() {
        return Err(ImageError::Download(format!(
            "curl failed with status: {}",
            output.status
        )));
    }

    if output.stdout.is_empty() {
        return Err(ImageError::Download("Empty response".to_string()));
    }

    Ok(output.stdout)
}

/// Decode raw image bytes (PNG or JPEG) into a `LoadedImage`.
/// The format is detected from magic bytes, not from a file extension.
pub fn decode_image_bytes(data: &[u8]) -> Result<LoadedImage, ImageError> {
    decode_image(data)
}

/// Decode image data (PNG or JPEG)
fn decode_image(data: &[u8]) -> Result<LoadedImage, ImageError> {
    // Check file signature to determine format
    if data.len() < 8 {
        return Err(ImageError::Decode("Data too short".to_string()));
    }

    // PNG signature: 89 50 4E 47 0D 0A 1A 0A
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        decode_png(data)
    }
    // JPEG signature: FF D8 FF
    else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        decode_jpeg(data)
    }
    else {
        Err(ImageError::UnsupportedFormat(
            "Only PNG and JPEG are supported".to_string(),
        ))
    }
}

/// Decode PNG image using the png crate
fn decode_png(data: &[u8]) -> Result<LoadedImage, ImageError> {
    use png::Decoder;

    let decoder = Decoder::new(data);
    let mut reader = decoder.read_info()
        .map_err(|e| ImageError::Decode(format!("PNG decode error: {}", e)))?;

    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)
        .map_err(|e| ImageError::Decode(format!("PNG frame error: {}", e)))?;

    // Convert to RGB if necessary
    let (rgb_data, color_space) = match info.color_type {
        png::ColorType::Rgb => (buf[..info.buffer_size()].to_vec(), "DeviceRGB".to_string()),
        png::ColorType::Rgba => {
            // Remove alpha channel
            let mut rgb = Vec::with_capacity((info.buffer_size() / 4) * 3);
            for chunk in buf[..info.buffer_size()].chunks(4) {
                rgb.push(chunk[0]);
                rgb.push(chunk[1]);
                rgb.push(chunk[2]);
            }
            (rgb, "DeviceRGB".to_string())
        }
        png::ColorType::Grayscale => (buf[..info.buffer_size()].to_vec(), "DeviceGray".to_string()),
        png::ColorType::GrayscaleAlpha => {
            // Remove alpha channel
            let mut gray = Vec::with_capacity(info.buffer_size() / 2);
            for chunk in buf[..info.buffer_size()].chunks(2) {
                gray.push(chunk[0]);
            }
            (gray, "DeviceGray".to_string())
        }
        _ => return Err(ImageError::Decode("Unsupported PNG color type".to_string())),
    };

    Ok(LoadedImage {
        data: rgb_data,
        width: info.width,
        height: info.height,
        bits_per_component: info.bit_depth as u8,
        color_space,
    })
}

/// Decode JPEG image using the image crate
fn decode_jpeg(data: &[u8]) -> Result<LoadedImage, ImageError> {
    use image::ImageReader;
    use std::io::Cursor;

    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| ImageError::Decode(format!("Image format error: {}", e)))?;

    let img = reader.decode()
        .map_err(|e| ImageError::Decode(format!("JPEG decode error: {}", e)))?;

    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();

    Ok(LoadedImage {
        data: rgb.into_raw(),
        width,
        height,
        bits_per_component: 8,
        color_space: "DeviceRGB".to_string(),
    })
}
