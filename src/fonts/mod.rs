//! Font system using Parley for accurate text layout
//! Provides text measurement, wrapping, and glyph positioning
//! Supports font registry with Google Fonts and local files

mod wrap;
pub mod embed;
pub mod google;

use wrap::{text_width_scaled, text_width_units_custom, truncate_lines_with_ellipsis, wrap_paragraph, wrap_paragraph_custom};

use crate::components::{FontDefinition, FontSource, HyphenationLang};
use crate::standard_fonts;
use crate::style::{FontStyle, TextAlign, TextOverflow};
use crate::fonts::google::resolve_to_google_url;
use hyphenation::{Language, Load, Standard};
use parley::layout::PositionedLayoutItem;
use parley::fontique::{FontInfoOverride, FontStyle as FontiqueStyle, FontWeight as FontiqueWeight};
use parley::style::{FontFeature, FontSettings, FontStack, LineHeight};
use parley::{FontContext, Layout, LayoutContext, StyleProperty};
use read_fonts::{FontRef, TableProvider};
use read_fonts::tables::kern::SubtableKind;
use read_fonts::types::{GlyphId, GlyphId16};
use skrifa::MetadataProvider;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::sync::{Arc, OnceLock};

/// Default font family name (used when no fonts defined)
pub const DEFAULT_FONT_FAMILY: &str = "Helvetica";

/// Loaded font data
#[derive(Clone)]
pub struct LoadedFont {
    pub family: String,
    pub data: Arc<Vec<u8>>,
    pub weight: u16,
    pub is_italic: bool,
    metrics: FontMetrics,
    kerning: Arc<HashMap<(u8, u8), f32>>,
    widths: Arc<[u16; 256]>,
    pub glyph_widths: Arc<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontKey {
    pub family: String,
    pub weight: u16,
    pub is_italic: bool,
}

impl FontKey {
    pub fn new(family: String, weight: u16, is_italic: bool) -> Self {
        Self {
            family,
            weight,
            is_italic,
        }
    }
}

/// Font system that manages fonts and provides text layout
pub struct FontSystem {
    font_cx: RefCell<FontContext>,
    layout_cx: RefCell<LayoutContext>,
    /// Registry of loaded fonts by family name
    fonts: RefCell<HashMap<String, LoadedFont>>,
    /// The first/default font family name
    default_family: RefCell<Option<String>>,
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedLineHeight {
    pub mult: f32,
    pub is_default: bool,
}

impl ResolvedLineHeight {
    fn for_parley(self) -> LineHeight {
        if self.is_default {
            LineHeight::MetricsRelative(1.0)
        } else {
            LineHeight::FontSizeRelative(self.mult)
        }
    }
}

fn default_font_features() -> FontSettings<'static, FontFeature> {
    // Match React-PDF/fontkit defaults by enabling common OpenType features.
    FontSettings::from("kern, liga, clig, calt")
}

const HYPHEN_PENALTY_JUSTIFY: f32 = 100.0;
const HYPHEN_PENALTY_DEFAULT: f32 = 600.0;
const ELLIPSIS_STR: &str = "\u{2026}";

fn hyphen_penalty_for_align(text_align: Option<TextAlign>) -> f32 {
    match text_align {
        Some(TextAlign::Justify) => HYPHEN_PENALTY_JUSTIFY,
        _ => HYPHEN_PENALTY_DEFAULT,
    }
}

fn hyphenator_for(lang: HyphenationLang) -> Option<&'static Standard> {
    static ENGLISH: OnceLock<Option<Standard>> = OnceLock::new();
    static FINNISH: OnceLock<Option<Standard>> = OnceLock::new();

    let init = |language| Standard::from_embedded(language).ok();
    match lang {
        HyphenationLang::English => ENGLISH.get_or_init(|| init(Language::EnglishUS)).as_ref(),
        HyphenationLang::Finnish => FINNISH.get_or_init(|| init(Language::Finnish)).as_ref(),
    }
}

enum ResolvedFontChoice {
    Custom(LoadedFont),
    Standard(&'static standard_fonts::StandardFontVariant),
}

#[derive(Debug, Clone, Copy)]
struct FontMetrics {
    ascent: f32,
    descent: f32,
    line_gap: f32,
    units_per_em: f32,
}

impl FontMetrics {
    fn default_line_height_mult(self) -> f32 {
        (self.ascent - self.descent + self.line_gap) / self.units_per_em
    }

    fn ascent_for_size(self, font_size: f32) -> f32 {
        self.ascent / self.units_per_em * font_size
    }

    fn fallback() -> Self {
        Self {
            ascent: 800.0,
            descent: -200.0,
            line_gap: 200.0,
            units_per_em: 1000.0,
        }
    }
}

fn metrics_from_font_data(data: &[u8]) -> Option<FontMetrics> {
    let font = FontRef::from_index(data, 0).ok()?;
    let head = font.head().ok()?;
    let hhea = font.hhea().ok()?;
    let units_per_em = head.units_per_em() as f32;
    if units_per_em <= 0.0 {
        return None;
    }

    let ascent = hhea.ascender().to_i16() as f32;
    let descent = hhea.descender().to_i16() as f32;
    let line_gap = hhea.line_gap().to_i16() as f32;

    Some(FontMetrics {
        ascent,
        descent,
        line_gap,
        units_per_em,
    })
}

fn kerning_from_font_data(data: &[u8]) -> HashMap<(u8, u8), f32> {
    let mut map = HashMap::new();

    let font = match FontRef::from_index(data, 0) {
        Ok(font) => font,
        Err(_) => return map,
    };

    let kern = match font.kern() {
        Ok(kern) => kern,
        Err(_) => return map,
    };

    let units_per_em = match font.head() {
        Ok(head) => head.units_per_em() as f32,
        Err(_) => 1000.0,
    };
    let scale = if units_per_em > 0.0 { 1000.0 / units_per_em } else { 1.0 };

    let skrifa_font = match skrifa::FontRef::new(data) {
        Ok(font) => font,
        Err(_) => return map,
    };
    let charmap = skrifa_font.charmap();

    let mut glyph_to_codes: HashMap<GlyphId16, Vec<u8>> = HashMap::new();
    for code in 0u16..=255 {
        let code = code as u8;
        let ch = match standard_fonts::win_ansi_char(code) {
            Some(ch) => ch,
            None => continue,
        };
        let gid = charmap.map(ch).unwrap_or_default();
        let gid16 = GlyphId16::new(gid.to_u32() as u16);
        if gid16 == GlyphId16::NOTDEF {
            continue;
        }
        glyph_to_codes.entry(gid16).or_default().push(code);
    }

    for subtable in kern.subtables() {
        let subtable = match subtable {
            Ok(subtable) => subtable,
            Err(_) => continue,
        };
        if !subtable.is_horizontal() || subtable.is_cross_stream() {
            continue;
        }
        let kind = match subtable.kind() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        let SubtableKind::Format0(format0) = kind else {
            continue;
        };

        for pair in format0.pairs() {
            let left = pair.left();
            let right = pair.right();
            let value = pair.value() as f32;
            if value == 0.0 {
                continue;
            }
            let value = value * scale;
            let Some(left_codes) = glyph_to_codes.get(&left) else { continue; };
            let Some(right_codes) = glyph_to_codes.get(&right) else { continue; };
            for &l in left_codes {
                for &r in right_codes {
                    map.insert((l, r), value);
                }
            }
        }
    }

    map
}

fn widths_from_font_data(data: &[u8]) -> [u16; 256] {
    let mut widths = [0u16; 256];

    let font = match skrifa::FontRef::new(data) {
        Ok(font) => font,
        Err(_) => return widths,
    };

    let units_per_em = font.head().map(|h| h.units_per_em() as f32).unwrap_or(1000.0);
    let scale = if units_per_em > 0.0 { 1000.0 / units_per_em } else { 1.0 };

    let charmap = font.charmap();
    let glyph_metrics = font.glyph_metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );

    for code in 0u16..=255 {
        let code = code as u8;
        let ch = match standard_fonts::win_ansi_char(code) {
            Some(ch) => ch,
            None => continue,
        };
        let glyph_id = charmap.map(ch).unwrap_or_default();
        let advance = glyph_metrics.advance_width(glyph_id).unwrap_or(0.0);
        let pdf_width = (advance * scale).round().max(0.0) as u16;
        widths[code as usize] = pdf_width;
    }

    widths
}

fn glyph_widths_from_font_data(data: &[u8]) -> Vec<f32> {
    let font = match skrifa::FontRef::new(data) {
        Ok(font) => font,
        Err(_) => return Vec::new(),
    };

    let units_per_em = font.head().map(|h| h.units_per_em() as f32).unwrap_or(1000.0);
    let scale = if units_per_em > 0.0 { 1000.0 / units_per_em } else { 1.0 };
    let glyph_count = font.maxp().map(|m| m.num_glyphs() as usize).unwrap_or(0);

    let glyph_metrics = font.glyph_metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );

    let mut widths = Vec::with_capacity(glyph_count);
    for gid in 0..glyph_count {
        let glyph_id = GlyphId::new(gid as u32);
        let advance = glyph_metrics.advance_width(glyph_id).unwrap_or(0.0);
        let pdf_width = (advance * scale).max(0.0);
        widths.push(pdf_width);
    }

    widths
}

impl FontSystem {
    /// Create a new font system
    pub fn new() -> Self {
        Self {
            font_cx: RefCell::new(FontContext::new()),
            layout_cx: RefCell::new(LayoutContext::new()),
            fonts: RefCell::new(HashMap::new()),
            default_family: RefCell::new(Some(DEFAULT_FONT_FAMILY.to_string())),
        }
    }

    /// Register fonts from document definitions
    /// This should be called before rendering
    pub fn register_fonts(&self, definitions: &[FontDefinition]) {
        let mut defs: Vec<FontDefinition> = definitions.to_vec();
        if defs.is_empty() {
            defs.push(FontDefinition {
                family: "Roboto".to_string(),
                source: FontSource::Google("Roboto".to_string()),
            });
        }

        for def in &defs {
            if let Err(e) = self.load_font_definition(def) {
                log::warn!("Failed to load font '{}': {}", def.family, e);
            }
        }

        // If no fonts were loaded, fall back to the built-in standard fonts.
        if self.fonts.borrow().is_empty() {
            *self.default_family.borrow_mut() = Some(DEFAULT_FONT_FAMILY.to_string());
        }
    }

    /// Load a font from its definition
    fn load_font_definition(&self, def: &FontDefinition) -> Result<(), String> {
        match &def.source {
            FontSource::Google(name_or_url) => {
                let url = resolve_to_google_url(name_or_url);
                log::info!("Loading Google Font: {} -> {}", def.family, name_or_url);
                let variants = crate::fonts::google::download_all_font_variants(&url)
                    .map_err(|e| e.to_string())?;
                for (data, weight, is_italic) in variants {
                    let weight = weight.min(u16::MAX as u32) as u16;
                    self.register_font_variant(&def.family, data, weight, is_italic);
                }
            }
            FontSource::Local(path) => {
                log::info!("Loading local font: {} -> {}", def.family, path);
                let data = fs::read(path).map_err(|e| format!("Failed to read file '{}': {}", path, e))?;
                self.register_font_data(&def.family, data);
            }
        };
        Ok(())
    }

    /// Register raw font data with a family name (assumes weight 400, normal style)
    pub fn register_font_data(&self, family: &str, data: Vec<u8>) {
        self.register_font_variant(family, data, 400, false);
    }

    /// Get a specific font variant by family, weight, and style
    pub fn get_font_variant(&self, family: Option<&str>, weight: Option<u16>, is_italic: bool) -> Option<LoadedFont> {
        let fonts = self.fonts.borrow();
        let default_family = self.default_family();
        let family = family.unwrap_or(&default_family);
        let weight = weight.unwrap_or(400);
        let style_str = if is_italic { "italic" } else { "normal" };

        // Try exact match first
        let key = format!("{}-{}-{}", family, weight, style_str);
        if let Some(font) = fonts.get(&key) {
            return Some(font.clone());
        }

        // Try to find closest weight match for the same style
        let mut best_match: Option<&LoadedFont> = None;
        let mut best_diff = u16::MAX;

        for (_k, font) in fonts.iter() {
            if font.family == family && font.is_italic == is_italic {
                let diff = (font.weight as i32 - weight as i32).unsigned_abs() as u16;
                if diff < best_diff {
                    best_diff = diff;
                    best_match = Some(font);
                }
            }
        }

        // If no match for style, try any font from the family
        if best_match.is_none() {
            for font in fonts.values() {
                if font.family == family {
                    best_match = Some(font);
                    break;
                }
            }
        }

        best_match.cloned()
    }

    fn resolve_font_choice(
        &self,
        family: Option<&str>,
        weight: Option<u16>,
        font_style: Option<FontStyle>,
    ) -> ResolvedFontChoice {
        let weight = weight.unwrap_or(400);
        let is_italic = matches!(font_style, Some(FontStyle::Italic) | Some(FontStyle::Oblique));
        let default_family = self.default_family();
        let requested_family = family.unwrap_or(default_family.as_str());

        if let Some(custom) = self.get_font_variant(Some(requested_family), Some(weight), is_italic) {
            return ResolvedFontChoice::Custom(custom);
        }

        if let Some(variant) = standard_fonts::resolve_standard_variant(requested_family, weight, is_italic) {
            return ResolvedFontChoice::Standard(variant);
        }

        if requested_family != default_family.as_str() {
            if let Some(custom) = self.get_font_variant(Some(default_family.as_str()), Some(weight), is_italic) {
                return ResolvedFontChoice::Custom(custom);
            }

            if let Some(variant) = standard_fonts::resolve_standard_variant(default_family.as_str(), weight, is_italic) {
                return ResolvedFontChoice::Standard(variant);
            }
        }

        let fallback = standard_fonts::resolve_standard_variant(DEFAULT_FONT_FAMILY, weight, is_italic)
            .or_else(|| standard_fonts::resolve_standard_variant(DEFAULT_FONT_FAMILY, 400, false))
            .unwrap_or_else(|| standard_fonts::default_variant());
        ResolvedFontChoice::Standard(fallback)
    }

    pub fn resolve_font_key(
        &self,
        family: Option<&str>,
        weight: Option<u16>,
        font_style: Option<FontStyle>,
    ) -> FontKey {
        match self.resolve_font_choice(family, weight, font_style) {
            ResolvedFontChoice::Custom(font) => FontKey::new(font.family, font.weight, font.is_italic),
            ResolvedFontChoice::Standard(variant) => FontKey::new(variant.family.to_string(), variant.weight, variant.is_italic),
        }
    }

    fn resolve_font_metrics(
        &self,
        family: Option<&str>,
        weight: Option<u16>,
        font_style: Option<FontStyle>,
    ) -> FontMetrics {
        match self.resolve_font_choice(family, weight, font_style) {
            ResolvedFontChoice::Custom(font) => font.metrics,
            ResolvedFontChoice::Standard(variant) => {
                let metrics = standard_fonts::line_metrics_for(variant.name);
                FontMetrics {
                    ascent: metrics.ascent,
                    descent: metrics.descent,
                    line_gap: metrics.line_gap,
                    units_per_em: metrics.units_per_em,
                }
            }
        }
    }

    pub fn resolve_kerning(
        &self,
        family: Option<&str>,
        weight: Option<u16>,
        font_style: Option<FontStyle>,
    ) -> Option<Arc<HashMap<(u8, u8), f32>>> {
        match self.resolve_font_choice(family, weight, font_style) {
            ResolvedFontChoice::Custom(font) => {
                if font.kerning.is_empty() {
                    None
                } else {
                    Some(font.kerning)
                }
            }
            ResolvedFontChoice::Standard(_) => None,
        }
    }

    /// Load the default font (standard Helvetica)
    pub fn load_default_font(&self) -> String {
        DEFAULT_FONT_FAMILY.to_string()
    }

    /// Register a specific font variant (weight + style)
    fn register_font_variant(&self, family: &str, data: Vec<u8>, weight: u16, is_italic: bool) {
        let metrics = metrics_from_font_data(&data).unwrap_or_else(FontMetrics::fallback);
        let kerning = Arc::new(kerning_from_font_data(&data));
        let widths = Arc::new(widths_from_font_data(&data));
        let glyph_widths = Arc::new(glyph_widths_from_font_data(&data));
        let data = Arc::new(data);

        // Register with Parley's font context
        {
            let mut font_cx = self.font_cx.borrow_mut();
            let info_override = FontInfoOverride {
                family_name: Some(family),
                weight: Some(FontiqueWeight::new(weight as f32)),
                style: Some(if is_italic {
                    FontiqueStyle::Italic
                } else {
                    FontiqueStyle::Normal
                }),
                ..Default::default()
            };
            font_cx
                .collection
                .register_fonts(data.to_vec().into(), Some(info_override));
        }

        // Store each variant with a unique key: "Family-weight-style"
        let style_str = if is_italic { "italic" } else { "normal" };
        let key = format!("{}-{}-{}", family, weight, style_str);

        {
            let mut fonts = self.fonts.borrow_mut();
            if fonts.is_empty() {
                *self.default_family.borrow_mut() = Some(family.to_string());
            }
            fonts.insert(
                key,
                LoadedFont {
                    family: family.to_string(),
                    data,
                    weight,
                    is_italic,
                    metrics,
                    kerning,
                    widths,
                    glyph_widths,
                },
            );
        }
    }

    /// Get the default font family name
    pub fn default_family(&self) -> String {
        self.default_family
            .borrow()
            .clone()
            .unwrap_or_else(|| DEFAULT_FONT_FAMILY.to_string())
    }

    /// Get font data by family name
    pub fn get_font(&self, family: &str) -> Option<LoadedFont> {
        self.fonts.borrow().get(family).cloned()
    }

    /// Get font data, falling back to default if not found
    pub fn get_font_or_default(&self, family: Option<&str>) -> LoadedFont {
        let default = self.default_family();
        let family = family.unwrap_or(&default);

        if let Some(font) = self.fonts.borrow().get(family).cloned() {
            return font;
        }

        // Fall back to default
        self.fonts
            .borrow()
            .get(&default)
            .cloned()
            .unwrap_or_else(|| LoadedFont {
                family: default,
                data: Arc::new(Vec::new()),
                weight: 400,
                is_italic: false,
                metrics: FontMetrics::fallback(),
                kerning: Arc::new(HashMap::new()),
                widths: Arc::new([0u16; 256]),
                glyph_widths: Arc::new(Vec::new()),
            })
    }

    /// Get all loaded fonts
    pub fn all_fonts(&self) -> Vec<LoadedFont> {
        self.fonts.borrow().values().cloned().collect()
    }

    /// Get line metrics for text at a given size
    pub fn line_metrics(&self, font_size: f32) -> LineMetrics {
        let metrics = self.resolve_font_metrics(None, Some(400), None);
        let ascent = metrics.ascent_for_size(font_size);
        let descent = metrics.descent / metrics.units_per_em * font_size;
        let line_height = metrics.default_line_height_mult() * font_size;
        LineMetrics {
            ascent,
            descent,
            line_height,
        }
    }

    pub fn resolve_line_height(
        &self,
        line_height: Option<f32>,
        font_family: Option<&str>,
        font_weight: Option<u16>,
        font_style: Option<FontStyle>,
    ) -> ResolvedLineHeight {
        let metrics = self.resolve_font_metrics(font_family, font_weight, font_style);
        let default_mult = metrics.default_line_height_mult();
        let mut mult = line_height.unwrap_or(default_mult);
        let mut is_default = line_height.is_none();
        if !mult.is_finite() || mult <= 0.0 {
            mult = default_mult;
            is_default = true;
        }
        ResolvedLineHeight { mult, is_default }
    }

    pub fn resolve_line_height_mult(
        &self,
        line_height: Option<f32>,
        font_family: Option<&str>,
        font_weight: Option<u16>,
        font_style: Option<FontStyle>,
    ) -> f32 {
        self.resolve_line_height(line_height, font_family, font_weight, font_style)
            .mult
    }

    fn build_advance_map(
        &self,
        paragraph: &str,
        font: &LoadedFont,
        font_size: f32,
        line_height: ResolvedLineHeight,
        letter_spacing: Option<f32>,
    ) -> Vec<f32> {
        if paragraph.is_empty() {
            return vec![0.0];
        }

        let mut font_cx = self.font_cx.borrow_mut();
        let mut layout_cx = self.layout_cx.borrow_mut();

        let scale = 1.0;
        let mut builder = layout_cx.ranged_builder(&mut font_cx, paragraph, scale, false);
        builder.push_default(StyleProperty::FontStack(FontStack::Single(
            parley::style::FontFamily::Named(font.family.clone().into()),
        )));
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::LineHeight(line_height.for_parley()));
        builder.push_default(StyleProperty::FontWeight(parley::style::FontWeight::new(font.weight as f32)));
        builder.push_default(StyleProperty::FontStyle(if font.is_italic {
            parley::style::FontStyle::Italic
        } else {
            parley::style::FontStyle::Normal
        }));
        builder.push_default(StyleProperty::FontFeatures(default_font_features()));
        if let Some(spacing) = letter_spacing {
            builder.push_default(StyleProperty::LetterSpacing(spacing));
        }

        let mut layout: Layout<[u8; 4]> = builder.build(paragraph);
        layout.break_all_lines(None);

        let mut clusters: Vec<(std::ops::Range<usize>, f32)> = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                    let run = glyph_run.run();
                    for cluster in run.visual_clusters() {
                        if cluster.is_hard_line_break() {
                            continue;
                        }
                        let range = cluster.text_range();
                        if range.start == range.end {
                            continue;
                        }
                        clusters.push((range, cluster.advance()));
                    }
                }
            }
        }

        clusters.sort_by_key(|(range, _)| range.start);

        let mut advances = vec![0.0; paragraph.len() + 1];
        let mut cursor = 0usize;
        let mut cumulative = 0.0;
        for (range, advance) in clusters {
            let end = range.end.min(advances.len() - 1);
            if cursor < end {
                for idx in cursor..end {
                    advances[idx] = cumulative;
                }
            }
            cumulative += advance;
            advances[end] = cumulative;
            cursor = end;
        }
        for idx in cursor..advances.len() {
            advances[idx] = cumulative;
        }

        advances
    }

    pub fn resolve_ascent(
        &self,
        font_family: Option<&str>,
        font_size: f32,
        font_weight: Option<u16>,
        font_style: Option<FontStyle>,
    ) -> f32 {
        let metrics = self.resolve_font_metrics(font_family, font_weight, font_style);
        metrics.ascent_for_size(font_size)
    }

    fn measure_custom_unwrapped(
        &self,
        text: &str,
        font: &LoadedFont,
        font_size: f32,
        line_height: ResolvedLineHeight,
        letter_spacing: Option<f32>,
    ) -> (f32, f32) {
        let mut font_cx = self.font_cx.borrow_mut();
        let mut layout_cx = self.layout_cx.borrow_mut();

        let scale = 1.0;
        let mut builder = layout_cx.ranged_builder(&mut font_cx, text, scale, false);
        builder.push_default(StyleProperty::FontStack(FontStack::Single(
            parley::style::FontFamily::Named(font.family.clone().into()),
        )));
        builder.push_default(StyleProperty::FontSize(font_size));
        builder.push_default(StyleProperty::LineHeight(line_height.for_parley()));
        builder.push_default(StyleProperty::FontWeight(parley::style::FontWeight::new(font.weight as f32)));
        builder.push_default(StyleProperty::FontStyle(if font.is_italic {
            parley::style::FontStyle::Italic
        } else {
            parley::style::FontStyle::Normal
        }));
        builder.push_default(StyleProperty::FontFeatures(default_font_features()));
        if let Some(spacing) = letter_spacing {
            builder.push_default(StyleProperty::LetterSpacing(spacing));
        }

        let mut layout: Layout<[u8; 4]> = builder.build(text);
        layout.break_all_lines(None);
        (layout.width(), layout.height())
    }

    /// Measure text dimensions with wrapping
    pub fn measure_text(
        &self,
        text: &str,
        font_family: Option<&str>,
        font_size: f32,
        font_weight: Option<u16>,
        font_style: Option<FontStyle>,
        line_height: ResolvedLineHeight,
        hyphenation: Option<HyphenationLang>,
        text_align: Option<TextAlign>,
        letter_spacing: Option<f32>,
        text_indent: Option<f32>,
        max_lines: Option<usize>,
        text_overflow: Option<TextOverflow>,
        max_width: f32,
    ) -> (f32, f32) {
        let line_height_mult = line_height.mult;
        if text.is_empty() {
            return (0.0, font_size * line_height_mult);
        }
        let letter_spacing = letter_spacing.unwrap_or(0.0);
        let text_indent = text_indent.unwrap_or(0.0);
        let wrap_width = if max_width > 0.0 { Some(max_width) } else { None };

        let (width, height) = match self.resolve_font_choice(font_family, font_weight, font_style) {
            ResolvedFontChoice::Standard(variant) => {
                let metrics = standard_fonts::metrics_or_default(variant.name);

                let mut wrapped_lines: Vec<String> = Vec::new();
                for paragraph in text.split('\n') {
                    wrapped_lines.extend(wrap_paragraph(
                        paragraph,
                        metrics,
                        font_size,
                        wrap_width,
                        hyphenation,
                        letter_spacing,
                    ));
                }

                wrapped_lines = truncate_lines_with_ellipsis(
                    wrapped_lines,
                    max_lines,
                    text_overflow,
                    wrap_width,
                    &|line| text_width_scaled(line, metrics, font_size, letter_spacing),
                );

                let mut max_line = 0.0;
                for (idx, line) in wrapped_lines.iter().enumerate() {
                    let mut line_width = text_width_scaled(line, metrics, font_size, letter_spacing);
                    if idx == 0 && text_indent > 0.0 {
                        line_width += text_indent;
                    }
                    if line_width > max_line {
                        max_line = line_width;
                    }
                }

                let line_count = wrapped_lines.len();
                let height = if line_count == 0 {
                    font_size * line_height_mult
                } else {
                    line_count as f32 * font_size * line_height_mult
                };
                (max_line, height)
            }
            ResolvedFontChoice::Custom(font) => {
                let measure_token = |token: &str| -> f32 {
                    if token.chars().all(|ch| ch.is_whitespace()) {
                        let units = text_width_units_custom(
                            token,
                            font.widths.as_ref(),
                            letter_spacing,
                            font_size,
                        );
                        return units * font_size / 1000.0;
                    }
                    let (width, _) = self.measure_custom_unwrapped(
                        token,
                        &font,
                        font_size,
                        line_height,
                        Some(letter_spacing),
                    );
                    width
                };

                let paragraphs: Vec<&str> = text.split('\n').collect();
                let mut wrapped_lines: Vec<String> = Vec::new();
                let hyphen_penalty = hyphen_penalty_for_align(text_align);
                for paragraph in paragraphs {
                    if let Some(wrap) = wrap_width {
                        let advance_map = self.build_advance_map(
                            paragraph,
                            &font,
                            font_size,
                            line_height,
                            Some(letter_spacing),
                        );
                        wrapped_lines.extend(wrap_paragraph_custom(
                            paragraph,
                            wrap,
                            &measure_token,
                            hyphenation,
                            Some(&advance_map),
                            hyphen_penalty,
                        ));
                    } else {
                        wrapped_lines.push(paragraph.to_string());
                    }
                }

                wrapped_lines = truncate_lines_with_ellipsis(
                    wrapped_lines,
                    max_lines,
                    text_overflow,
                    wrap_width,
                    &|line| {
                        self.measure_custom_unwrapped(
                            line,
                            &font,
                            font_size,
                            line_height,
                            Some(letter_spacing),
                        )
                        .0
                    },
                );

                let mut max_line = 0.0;
                for (idx, line) in wrapped_lines.iter().enumerate() {
                    let mut line_width = self
                        .measure_custom_unwrapped(line, &font, font_size, line_height, Some(letter_spacing))
                        .0;
                    if idx == 0 && text_indent > 0.0 {
                        line_width += text_indent;
                    }
                    if line_width > max_line {
                        max_line = line_width;
                    }
                }

                let line_count = wrapped_lines.len();
                let height = if line_count == 0 {
                    font_size * line_height_mult
                } else {
                    line_count as f32 * font_size * line_height_mult
                };
                (max_line, height)
            }
        };

        if std::env::var_os("FLEX_PDF_DEBUG_MEASURE").is_some() {
            let snippet: String = text.chars().take(60).collect();
            log::debug!(
                "measure_text: \"{}\" size={} line_height={} max_width={} => w={:.2} h={:.2}",
                snippet,
                font_size,
                line_height_mult,
                max_width,
                width,
                height
            );
        }

        (width, height)
    }

    /// Layout text and return positioned glyphs for rendering
    pub fn layout_text(
        &self,
        text: &str,
        font_family: Option<&str>,
        font_size: f32,
        font_weight: Option<u16>,
        font_style: Option<FontStyle>,
        line_height: ResolvedLineHeight,
        hyphenation: Option<HyphenationLang>,
        max_width: f32,
        text_align: Option<TextAlign>,
        letter_spacing: Option<f32>,
        text_indent: Option<f32>,
        max_lines: Option<usize>,
        text_overflow: Option<TextOverflow>,
    ) -> TextLayout {
        let line_height_mult = line_height.mult;
        if text.is_empty() {
            return TextLayout {
                lines: vec![],
                width: 0.0,
                height: font_size * line_height_mult,
                font_size,
                line_height: font_size * line_height_mult,
                font_family: self.default_family(),
            };
        }

        match self.resolve_font_choice(font_family, font_weight, font_style) {
            ResolvedFontChoice::Standard(variant) => {
                let metrics = standard_fonts::metrics_or_default(variant.name);
                layout_standard_text(
                    text,
                    metrics,
                    variant.family,
                    font_size,
                    line_height_mult,
                    hyphenation,
                    max_width,
                    text_align,
                    letter_spacing,
                    text_indent,
                    max_lines,
                    text_overflow,
                )
            }
            ResolvedFontChoice::Custom(font) => {
                let letter_spacing = letter_spacing.unwrap_or(0.0);
                let text_indent = text_indent.unwrap_or(0.0);
                let wrap_width = if max_width > 0.0 { Some(max_width) } else { None };

                let measure_token = |token: &str| -> f32 {
                    if token.chars().all(|ch| ch.is_whitespace()) {
                        let units =
                            text_width_units_custom(token, font.widths.as_ref(), letter_spacing, font_size);
                        return units * font_size / 1000.0;
                    }
                    let (width, _) = self.measure_custom_unwrapped(
                        token,
                        &font,
                        font_size,
                        line_height,
                        Some(letter_spacing),
                    );
                    width
                };

                let paragraphs: Vec<&str> = text.split('\n').collect();
                let mut wrapped_lines: Vec<String> = Vec::new();
                let hyphen_penalty = hyphen_penalty_for_align(text_align);
                for paragraph in paragraphs {
                    if let Some(wrap) = wrap_width {
                        let advance_map = self.build_advance_map(
                            paragraph,
                            &font,
                            font_size,
                            line_height,
                            Some(letter_spacing),
                        );
                        wrapped_lines.extend(wrap_paragraph_custom(
                            paragraph,
                            wrap,
                            &measure_token,
                            hyphenation,
                            Some(&advance_map),
                            hyphen_penalty,
                        ));
                    } else {
                        wrapped_lines.push(paragraph.to_string());
                    }
                }

                let joined_text = wrapped_lines.join("\n");

                let layout: Layout<[u8; 4]> = {
                    let mut font_cx = self.font_cx.borrow_mut();
                    let mut layout_cx = self.layout_cx.borrow_mut();

                    let scale = 1.0;
                    let mut builder = layout_cx.ranged_builder(&mut font_cx, &joined_text, scale, false);
                    builder.push_default(StyleProperty::FontStack(FontStack::Single(
                        parley::style::FontFamily::Named(font.family.clone().into()),
                    )));
                    builder.push_default(StyleProperty::FontSize(font_size));
                    builder.push_default(StyleProperty::LineHeight(line_height.for_parley()));
                    builder.push_default(StyleProperty::FontWeight(parley::style::FontWeight::new(font.weight as f32)));
                    builder.push_default(StyleProperty::FontStyle(if font.is_italic {
                        parley::style::FontStyle::Italic
                    } else {
                        parley::style::FontStyle::Normal
                    }));
                    builder.push_default(StyleProperty::FontFeatures(default_font_features()));
                    if letter_spacing != 0.0 {
                        builder.push_default(StyleProperty::LetterSpacing(letter_spacing));
                    }

                    let mut layout: Layout<[u8; 4]> = builder.build(&joined_text);
                    layout.break_all_lines(None);
                    layout
                };

                let mut lines = Vec::new();
                for line in layout.lines() {
                    let metrics = line.metrics();
                    let line_y = metrics.baseline - (metrics.leading * 0.5);
                    let mut line_text = String::new();
                    let mut glyphs: Vec<GlyphSegment> = Vec::new();

                    for item in line.items() {
                        if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                            let run = glyph_run.run();
                            for cluster in run.visual_clusters() {
                                if cluster.is_hard_line_break() {
                                    continue;
                                }
                                let text_range = cluster.text_range();
                                let Some(cluster_text) = joined_text.get(text_range) else {
                                    continue;
                                };
                                if cluster_text.is_empty() {
                                    continue;
                                }
                                line_text.push_str(cluster_text);
                                let is_whitespace = cluster.is_space_or_nbsp()
                                    || cluster_text.chars().all(|ch| ch.is_whitespace());
                                for glyph in cluster.glyphs() {
                                    let gid = glyph.id.min(u32::from(u16::MAX)) as u16;
                                    let nominal_units = font
                                        .glyph_widths
                                        .get(gid as usize)
                                        .copied()
                                        .unwrap_or(0.0);
                                    let actual_units = glyph.advance * 1000.0 / font_size;
                                    let adjust = nominal_units - actual_units;
                                    let is_mark = glyph.advance.abs() <= f32::EPSILON;
                                    glyphs.push(GlyphSegment {
                                        glyph_id: gid,
                                        adjust,
                                        is_whitespace,
                                        is_mark,
                                    });
                                }
                            }
                        }
                    }

                    while let Some(last) = glyphs.last() {
                        if last.is_whitespace {
                            glyphs.pop();
                        } else {
                            break;
                        }
                    }

                    let trimmed = line_text.trim_end().to_string();
                    if !trimmed.is_empty() {
                        let line_width = metrics.advance - metrics.trailing_whitespace;
                        lines.push(TextLine {
                            text: trimmed,
                            x: 0.0,
                            y: line_y,
                            width: line_width,
                            segments: None,
                            glyphs: Some(glyphs),
                        });

                    }
                }

                let truncated = truncate_lines_with_ellipsis(
                    lines.iter().map(|line| line.text.clone()).collect(),
                    max_lines,
                    text_overflow,
                    wrap_width,
                    &|line| {
                        self.measure_custom_unwrapped(
                            line,
                            &font,
                            font_size,
                            line_height,
                            Some(letter_spacing),
                        )
                        .0
                    },
                );

                if truncated.len() < lines.len() {
                    lines.truncate(truncated.len());
                }

                for (idx, text) in truncated.into_iter().enumerate() {
                    if let Some(line) = lines.get_mut(idx) {
                        if line.text != text {
                            line.text = text;
                            line.glyphs = self.layout_inline_glyphs(
                                &line.text,
                                Some(&font.family),
                                font_size,
                                Some(font.weight),
                                Some(if font.is_italic {
                                    FontStyle::Italic
                                } else {
                                    FontStyle::Normal
                                }),
                                line_height,
                                Some(letter_spacing),
                            );
                            line.width = self
                                .measure_custom_unwrapped(
                                    &line.text,
                                    &font,
                                    font_size,
                                    line_height,
                                    Some(letter_spacing),
                                )
                                .0;
                        }
                    }
                }

                let mut max_line_width = 0.0;
                for (idx, line) in lines.iter_mut().enumerate() {
                    let mut line_width = line.width;
                    if idx == 0 && text_indent > 0.0 {
                        line_width += text_indent;
                    }
                    let align_offset = match text_align {
                        Some(TextAlign::Center) => wrap_width.map(|w| (w - line_width) / 2.0).unwrap_or(0.0),
                        Some(TextAlign::Right) => wrap_width.map(|w| w - line_width).unwrap_or(0.0),
                        Some(TextAlign::Justify) => 0.0,
                        _ => 0.0,
                    };
                    let mut x = align_offset.max(0.0);
                    if idx == 0 && text_indent > 0.0 {
                        x += text_indent;
                    }
                    line.x = x;
                    line.width = line_width;
                    if line_width > max_line_width {
                        max_line_width = line_width;
                    }
                }

                let line_count = lines.len();
                TextLayout {
                    lines,
                    width: max_line_width,
                    height: if line_count == 0 {
                        font_size * line_height_mult
                    } else {
                        line_count as f32 * font_size * line_height_mult
                    },
                    font_size,
                    line_height: font_size * line_height_mult,
                    font_family: font.family,
                }
            }
        }
    }

    pub fn layout_inline_segments(
        &self,
        text: &str,
        font_family: Option<&str>,
        font_size: f32,
        font_weight: Option<u16>,
        font_style: Option<FontStyle>,
        line_height: ResolvedLineHeight,
        letter_spacing: Option<f32>,
    ) -> Option<Vec<TextSegment>> {
        if text.is_empty() {
            return Some(Vec::new());
        }

        match self.resolve_font_choice(font_family, font_weight, font_style) {
            ResolvedFontChoice::Standard(_) => None,
            ResolvedFontChoice::Custom(font) => {
                let mut font_cx = self.font_cx.borrow_mut();
                let mut layout_cx = self.layout_cx.borrow_mut();

                let scale = 1.0;
                let mut builder = layout_cx.ranged_builder(&mut font_cx, text, scale, false);
                builder.push_default(StyleProperty::FontStack(FontStack::Single(
                    parley::style::FontFamily::Named(font.family.clone().into()),
                )));
                builder.push_default(StyleProperty::FontSize(font_size));
                builder.push_default(StyleProperty::LineHeight(line_height.for_parley()));
                builder.push_default(StyleProperty::FontWeight(parley::style::FontWeight::new(font.weight as f32)));
                builder.push_default(StyleProperty::FontStyle(if font.is_italic {
                    parley::style::FontStyle::Italic
                } else {
                    parley::style::FontStyle::Normal
                }));
                builder.push_default(StyleProperty::FontFeatures(default_font_features()));
                if let Some(spacing) = letter_spacing {
                    builder.push_default(StyleProperty::LetterSpacing(spacing));
                }

                let mut layout: Layout<[u8; 4]> = builder.build(text);
                layout.break_all_lines(None);

                let mut segments = Vec::new();
                for line in layout.lines() {
                    for item in line.items() {
                        if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                            let run = glyph_run.run();
                            for cluster in run.visual_clusters() {
                                if cluster.is_hard_line_break() {
                                    continue;
                                }
                                let text_range = cluster.text_range();
                                let Some(cluster_text) = text.get(text_range) else {
                                    continue;
                                };
                                if cluster_text.is_empty() {
                                    continue;
                                }
                                let cluster_width_units = text_width_units_custom(
                                    cluster_text,
                                    font.widths.as_ref(),
                                    letter_spacing.unwrap_or(0.0),
                                    font_size,
                                );
                                let cluster_advance_units = cluster.advance() * 1000.0 / font_size;
                                let adjust = cluster_width_units - cluster_advance_units;
                                segments.push(TextSegment {
                                    text: cluster_text.to_string(),
                                    adjust,
                                });
                            }
                        }
                    }
                }

                Some(segments)
            }
        }
    }

    pub fn layout_inline_glyphs(
        &self,
        text: &str,
        font_family: Option<&str>,
        font_size: f32,
        font_weight: Option<u16>,
        font_style: Option<FontStyle>,
        line_height: ResolvedLineHeight,
        letter_spacing: Option<f32>,
    ) -> Option<Vec<GlyphSegment>> {
        if text.is_empty() {
            return Some(Vec::new());
        }

        match self.resolve_font_choice(font_family, font_weight, font_style) {
            ResolvedFontChoice::Standard(_) => None,
            ResolvedFontChoice::Custom(font) => {
                let mut font_cx = self.font_cx.borrow_mut();
                let mut layout_cx = self.layout_cx.borrow_mut();

                let scale = 1.0;
                let mut builder = layout_cx.ranged_builder(&mut font_cx, text, scale, false);
                builder.push_default(StyleProperty::FontStack(FontStack::Single(
                    parley::style::FontFamily::Named(font.family.clone().into()),
                )));
                builder.push_default(StyleProperty::FontSize(font_size));
                builder.push_default(StyleProperty::LineHeight(line_height.for_parley()));
                builder.push_default(StyleProperty::FontWeight(parley::style::FontWeight::new(font.weight as f32)));
                builder.push_default(StyleProperty::FontStyle(if font.is_italic {
                    parley::style::FontStyle::Italic
                } else {
                    parley::style::FontStyle::Normal
                }));
                builder.push_default(StyleProperty::FontFeatures(default_font_features()));
                if let Some(spacing) = letter_spacing {
                    builder.push_default(StyleProperty::LetterSpacing(spacing));
                }

                let mut layout: Layout<[u8; 4]> = builder.build(text);
                layout.break_all_lines(None);

                let mut glyphs = Vec::new();
                for line in layout.lines() {
                    for item in line.items() {
                        if let PositionedLayoutItem::GlyphRun(glyph_run) = item {
                            let run = glyph_run.run();
                            for cluster in run.visual_clusters() {
                                if cluster.is_hard_line_break() {
                                    continue;
                                }
                                let text_range = cluster.text_range();
                                let Some(cluster_text) = text.get(text_range) else {
                                    continue;
                                };
                                if cluster_text.is_empty() {
                                    continue;
                                }
                                let is_whitespace = cluster.is_space_or_nbsp()
                                    || cluster_text.chars().all(|ch| ch.is_whitespace());
                                for glyph in cluster.glyphs() {
                                    let gid = glyph.id.min(u32::from(u16::MAX)) as u16;
                                    let nominal_units = font
                                        .glyph_widths
                                        .get(gid as usize)
                                        .copied()
                                        .unwrap_or(0.0);
                                    let actual_units = glyph.advance * 1000.0 / font_size;
                                    let adjust = nominal_units - actual_units;
                                    let is_mark = glyph.advance.abs() <= f32::EPSILON;
                                    glyphs.push(GlyphSegment {
                                        glyph_id: gid,
                                        adjust,
                                        is_whitespace,
                                        is_mark,
                                    });
                                }
                            }
                        }
                    }
                }

                Some(glyphs)
            }
        }
    }
}


fn layout_standard_text(
    text: &str,
    metrics: &standard_fonts::StandardFontMetrics,
    family_name: &str,
    font_size: f32,
    line_height_mult: f32,
    hyphenation: Option<HyphenationLang>,
    max_width: f32,
    text_align: Option<TextAlign>,
    letter_spacing: Option<f32>,
    text_indent: Option<f32>,
    max_lines: Option<usize>,
    text_overflow: Option<TextOverflow>,
) -> TextLayout {
    let line_height = font_size * line_height_mult;
    let wrap_width = if max_width > 0.0 { Some(max_width) } else { None };
    let letter_spacing = letter_spacing.unwrap_or(0.0);
    let text_indent = text_indent.unwrap_or(0.0);

    let line_metrics = standard_fonts::line_metrics_for(metrics.name);
    let ascent = line_metrics.ascent / line_metrics.units_per_em * font_size;
    let descent = line_metrics.descent / line_metrics.units_per_em * font_size;
    let em_height = ascent - descent;
    let leading = (line_height - em_height).max(0.0);
    let baseline_offset = ascent + (leading / 2.0);

    let paragraphs: Vec<&str> = text.split('\n').collect();
    let mut raw_lines: Vec<String> = Vec::new();
    for paragraph in paragraphs {
        raw_lines.extend(wrap_paragraph(
            paragraph,
            metrics,
            font_size,
            wrap_width,
            hyphenation,
            letter_spacing,
        ));
    }

    raw_lines = truncate_lines_with_ellipsis(
        raw_lines,
        max_lines,
        text_overflow,
        wrap_width,
        &|line| text_width_scaled(line, metrics, font_size, letter_spacing),
    );

    let mut lines = Vec::new();
    let mut max_line_width = 0.0;
    for (line_index, line) in raw_lines.into_iter().enumerate() {
        let trimmed = line.trim_end().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let mut line_width = text_width_scaled(&trimmed, metrics, font_size, letter_spacing);
        if line_index == 0 && text_indent > 0.0 {
            line_width += text_indent;
        }
        if line_width > max_line_width {
            max_line_width = line_width;
        }

        let align_offset = match text_align {
            Some(TextAlign::Center) => wrap_width.map(|w| (w - line_width) / 2.0).unwrap_or(0.0),
            Some(TextAlign::Right) => wrap_width.map(|w| w - line_width).unwrap_or(0.0),
            Some(TextAlign::Justify) => 0.0,
            _ => 0.0,
        };

        let mut x = align_offset.max(0.0);
        if line_index == 0 && text_indent > 0.0 {
            x += text_indent;
        }
        let line_y = baseline_offset + (line_index as f32 * line_height);
        lines.push(TextLine {
            text: trimmed,
            x,
            y: line_y,
            width: line_width,
            segments: None,
            glyphs: None,
        });
    }

    let height = if lines.is_empty() {
        line_height
    } else {
        lines.len() as f32 * line_height
    };
    TextLayout {
        lines,
        width: max_line_width,
        height,
        font_size,
        line_height,
        font_family: family_name.to_string(),
    }
}

impl Default for FontSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Line metrics for a font at a specific size
#[derive(Debug, Clone, Copy)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
}

/// Result of laying out text
#[derive(Debug, Clone)]
pub struct TextLayout {
    pub lines: Vec<TextLine>,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
    pub line_height: f32,
    pub font_family: String,
}

impl TextLayout {
    pub fn lines(&self) -> &[TextLine] {
        &self.lines
    }
}

/// A line of text with its position
#[derive(Debug, Clone)]
pub struct TextLine {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub segments: Option<Vec<TextSegment>>,
    pub glyphs: Option<Vec<GlyphSegment>>,
}

#[derive(Debug, Clone)]
pub struct TextSegment {
    pub text: String,
    pub adjust: f32,
}

#[derive(Debug, Clone)]
pub struct GlyphSegment {
    pub glyph_id: u16,
    pub adjust: f32,
    pub is_whitespace: bool,
    pub is_mark: bool,
}

/// Measure wrapped text using the font system
pub fn measure_wrapped_text(
    text: &str,
    font_family: Option<&str>,
    font_size: f32,
    font_weight: Option<u16>,
    font_style: Option<FontStyle>,
    line_height: ResolvedLineHeight,
    hyphenation: Option<HyphenationLang>,
    text_align: Option<TextAlign>,
    letter_spacing: Option<f32>,
    text_indent: Option<f32>,
    max_lines: Option<usize>,
    text_overflow: Option<TextOverflow>,
    max_width: f32,
    font_system: &FontSystem,
) -> (f32, f32) {
    font_system.measure_text(
        text,
        font_family,
        font_size,
        font_weight,
        font_style,
        line_height,
        hyphenation,
        text_align,
        letter_spacing,
        text_indent,
        max_lines,
        text_overflow,
        max_width,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyphenation::Hyphenator;

    #[test]
    fn hyphenation_dictionaries_load() {
        assert!(hyphenator_for(HyphenationLang::English).is_some());
        assert!(hyphenator_for(HyphenationLang::Finnish).is_some());
    }

    #[test]
    fn wrap_paragraph_hyphenates_when_enabled() {
        let metrics = standard_fonts::metrics_for("Helvetica").expect("standard font metrics");
        let word = "hyphenation";
        let hyphenator = hyphenator_for(HyphenationLang::English).expect("hyphenator");
        let breaks = &hyphenator.hyphenate(word).breaks;
        assert!(!breaks.is_empty());
        let first_break = breaks[0];
        let prefix = &word[..first_break];
        let mut prefix_with_hyphen = String::from(prefix);
        prefix_with_hyphen.push('-');
        let wrap_width = text_width_scaled(&prefix_with_hyphen, metrics, 12.0, 0.0) + 0.1;

        let lines = wrap_paragraph(
            word,
            metrics,
            12.0,
            Some(wrap_width),
            Some(HyphenationLang::English),
            0.0,
        );
        assert!(lines.len() > 1);
        assert!(lines[0].ends_with('-'));
    }
}
