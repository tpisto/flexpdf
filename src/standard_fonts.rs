//! Standard PDF fonts and AFM metrics parsing.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct StandardFontMetrics {
    pub name: &'static str,
    pub ascender: f32,
    pub descender: f32,
    pub cap_height: f32,
    pub widths: [u16; 256],
    pub kerning: HashMap<(u8, u8), i16>,
}

#[derive(Debug, Clone, Copy)]
pub struct StandardFontLineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub units_per_em: f32,
}

impl StandardFontLineMetrics {
    pub fn default_line_height_mult(self) -> f32 {
        (self.ascent - self.descent + self.line_gap) / self.units_per_em
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StandardFontVariant {
    pub family: &'static str,
    pub name: &'static str,
    pub weight: u16,
    pub is_italic: bool,
}

const STANDARD_VARIANTS: [StandardFontVariant; 12] = [
    StandardFontVariant { family: "Helvetica", name: "Helvetica", weight: 400, is_italic: false },
    StandardFontVariant { family: "Helvetica", name: "Helvetica-Bold", weight: 700, is_italic: false },
    StandardFontVariant { family: "Helvetica", name: "Helvetica-Oblique", weight: 400, is_italic: true },
    StandardFontVariant { family: "Helvetica", name: "Helvetica-BoldOblique", weight: 700, is_italic: true },
    StandardFontVariant { family: "Times-Roman", name: "Times-Roman", weight: 400, is_italic: false },
    StandardFontVariant { family: "Times-Roman", name: "Times-Bold", weight: 700, is_italic: false },
    StandardFontVariant { family: "Times-Roman", name: "Times-Italic", weight: 400, is_italic: true },
    StandardFontVariant { family: "Times-Roman", name: "Times-BoldItalic", weight: 700, is_italic: true },
    StandardFontVariant { family: "Courier", name: "Courier", weight: 400, is_italic: false },
    StandardFontVariant { family: "Courier", name: "Courier-Bold", weight: 700, is_italic: false },
    StandardFontVariant { family: "Courier", name: "Courier-Oblique", weight: 400, is_italic: true },
    StandardFontVariant { family: "Courier", name: "Courier-BoldOblique", weight: 700, is_italic: true },
];

const STANDARD_FAMILIES: [&str; 3] = ["Helvetica", "Times-Roman", "Courier"];

pub fn standard_variants() -> &'static [StandardFontVariant] {
    &STANDARD_VARIANTS
}

pub fn metrics_for(name: &str) -> Option<&'static StandardFontMetrics> {
    metrics_map().get(name)
}

pub fn metrics_or_default(name: &str) -> &'static StandardFontMetrics {
    if let Some(metrics) = metrics_for(name) {
        return metrics;
    }
    if let Some(metrics) = metrics_for("Helvetica") {
        return metrics;
    }
    default_metrics()
}

pub fn default_variant() -> &'static StandardFontVariant {
    STANDARD_VARIANTS
        .first()
        .unwrap_or(&StandardFontVariant {
            family: "Helvetica",
            name: "Helvetica",
            weight: 400,
            is_italic: false,
        })
}

pub fn line_metrics_for(name: &str) -> StandardFontLineMetrics {
    let family = standard_family_for_name(name);
    let descent = match family {
        "Times-Roman" => -220.0,
        "Courier" => -230.0,
        _ => -200.0,
    };

    StandardFontLineMetrics {
        ascent: 900.0,
        descent,
        line_gap: 0.0,
        units_per_em: 1000.0,
    }
}

fn default_metrics() -> &'static StandardFontMetrics {
    static FALLBACK: OnceLock<StandardFontMetrics> = OnceLock::new();
    FALLBACK.get_or_init(|| StandardFontMetrics {
        name: "Helvetica",
        ascender: 900.0,
        descender: -200.0,
        cap_height: 700.0,
        widths: [600; 256],
        kerning: HashMap::new(),
    })
}

pub fn win_ansi_code(ch: char) -> Option<u8> {
    let code = ch as u32;
    let mapped = match code {
        402 => 131,
        8211 => 150,
        8212 => 151,
        8216 => 145,
        8217 => 146,
        8218 => 130,
        8220 => 147,
        8221 => 148,
        8222 => 132,
        8224 => 134,
        8225 => 135,
        8226 => 149,
        8230 => 133,
        8364 => 128,
        8240 => 137,
        8249 => 139,
        8250 => 155,
        710 => 136,
        8482 => 153,
        338 => 140,
        339 => 156,
        732 => 152,
        352 => 138,
        353 => 154,
        376 => 159,
        381 => 142,
        382 => 158,
        _ => code,
    };

    if mapped <= 0xFF {
        Some(mapped as u8)
    } else {
        None
    }
}

pub fn win_ansi_char(code: u8) -> Option<char> {
    let mapped = match code {
        128 => 8364,
        130 => 8218,
        131 => 402,
        132 => 8222,
        133 => 8230,
        134 => 8224,
        135 => 8225,
        136 => 710,
        137 => 8240,
        138 => 352,
        139 => 8249,
        140 => 338,
        142 => 381,
        145 => 8216,
        146 => 8217,
        147 => 8220,
        148 => 8221,
        149 => 8226,
        150 => 8211,
        151 => 8212,
        152 => 732,
        153 => 8482,
        154 => 353,
        155 => 8250,
        156 => 339,
        158 => 382,
        159 => 376,
        _ => code as u32,
    };

    std::char::from_u32(mapped)
}

pub fn resolve_standard_variant(family: &str, weight: u16, is_italic: bool) -> Option<&'static StandardFontVariant> {
    // If the family already matches a variant name, use it directly.
    if let Some(variant) = STANDARD_VARIANTS.iter().find(|v| v.name == family) {
        return Some(variant);
    }

    let family = if family == "Times" { "Times-Roman" } else { family };
    if !STANDARD_FAMILIES.contains(&family) {
        return None;
    }

    let target_weight = if weight >= 600 { 700 } else { 400 };
    STANDARD_VARIANTS
        .iter()
        .find(|v| v.family == family && v.weight == target_weight && v.is_italic == is_italic)
        .or_else(|| STANDARD_VARIANTS.iter().find(|v| v.family == family && v.weight == target_weight))
        .or_else(|| STANDARD_VARIANTS.iter().find(|v| v.family == family))
}

fn standard_family_for_name(name: &str) -> &'static str {
    if name.starts_with("Times-") || name == "Times-Roman" || name == "Times" {
        "Times-Roman"
    } else if name.starts_with("Courier") {
        "Courier"
    } else {
        "Helvetica"
    }
}

fn metrics_map() -> &'static HashMap<&'static str, StandardFontMetrics> {
    static METRICS: OnceLock<HashMap<&'static str, StandardFontMetrics>> = OnceLock::new();
    METRICS.get_or_init(|| {
        let mut map = HashMap::new();
        // AFM data sourced from PDFKit (see assets/pdfkit/LICENSE).
        map.insert("Helvetica", parse_afm("Helvetica", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Helvetica.afm"))));
        map.insert("Helvetica-Bold", parse_afm("Helvetica-Bold", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Helvetica-Bold.afm"))));
        map.insert("Helvetica-Oblique", parse_afm("Helvetica-Oblique", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Helvetica-Oblique.afm"))));
        map.insert("Helvetica-BoldOblique", parse_afm("Helvetica-BoldOblique", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Helvetica-BoldOblique.afm"))));
        map.insert("Times-Roman", parse_afm("Times-Roman", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Times-Roman.afm"))));
        map.insert("Times-Bold", parse_afm("Times-Bold", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Times-Bold.afm"))));
        map.insert("Times-Italic", parse_afm("Times-Italic", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Times-Italic.afm"))));
        map.insert("Times-BoldItalic", parse_afm("Times-BoldItalic", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Times-BoldItalic.afm"))));
        map.insert("Courier", parse_afm("Courier", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Courier.afm"))));
        map.insert("Courier-Bold", parse_afm("Courier-Bold", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Courier-Bold.afm"))));
        map.insert("Courier-Oblique", parse_afm("Courier-Oblique", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Courier-Oblique.afm"))));
        map.insert("Courier-BoldOblique", parse_afm("Courier-BoldOblique", include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/pdfkit/afm/Courier-BoldOblique.afm"))));
        map
    })
}

fn parse_afm(name: &'static str, data: &str) -> StandardFontMetrics {
    let mut widths = [0u16; 256];
    let mut name_to_code: HashMap<&str, u8> = HashMap::new();
    let mut kerning: HashMap<(u8, u8), i16> = HashMap::new();

    let mut ascender = 700.0;
    let mut descender = -200.0;
    let mut cap_height = 700.0;

    for line in data.lines() {
        if let Some(value) = line.strip_prefix("Ascender ") {
            ascender = value.trim().parse::<f32>().unwrap_or(ascender);
        } else if let Some(value) = line.strip_prefix("Descender ") {
            descender = value.trim().parse::<f32>().unwrap_or(descender);
        } else if let Some(value) = line.strip_prefix("CapHeight ") {
            cap_height = value.trim().parse::<f32>().unwrap_or(cap_height);
        } else if line.starts_with("C ") {
            let mut code: Option<i32> = None;
            let mut width: Option<i32> = None;
            let mut glyph_name: Option<&str> = None;

            for part in line.split(';') {
                let part = part.trim();
                if let Some(value) = part.strip_prefix("C ") {
                    code = value.trim().parse::<i32>().ok();
                } else if let Some(value) = part.strip_prefix("WX ") {
                    width = value.trim().parse::<i32>().ok();
                } else if let Some(value) = part.strip_prefix("N ") {
                    glyph_name = Some(value.trim());
                }
            }

            if let (Some(code), Some(width)) = (code, width) {
                if (0..=255).contains(&code) {
                    widths[code as usize] = width.max(0) as u16;
                    if let Some(name) = glyph_name {
                        name_to_code.insert(name, code as u8);
                    }
                }
            }
        } else if let Some(rest) = line.strip_prefix("KPX ") {
            let mut parts = rest.split_whitespace();
            let left = parts.next();
            let right = parts.next();
            let value = parts.next();

            if let (Some(left), Some(right), Some(value)) = (left, right, value) {
                if let (Some(&left_code), Some(&right_code)) = (name_to_code.get(left), name_to_code.get(right)) {
                    if let Ok(kern) = value.parse::<i16>() {
                        kerning.insert((left_code, right_code), kern);
                    }
                }
            }
        }
    }

    StandardFontMetrics {
        name,
        ascender,
        descender,
        cap_height,
        widths,
        kerning,
    }
}
