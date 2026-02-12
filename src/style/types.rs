//! Core style types used by flexpdf.

use crate::components::ObjectFit;

#[derive(Debug, Clone, Default)]
pub struct Style {
    // Flexbox
    pub flex_direction: Option<FlexDirection>,
    pub flex: Option<f32>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<Dimension>,
    pub align_self: Option<AlignSelf>,
    pub align_items: Option<AlignItems>,
    pub align_content: Option<AlignContent>,
    pub justify_content: Option<JustifyContent>,
    pub flex_wrap: Option<FlexWrap>,

    // Dimensions
    pub width: Option<Dimension>,
    pub height: Option<Dimension>,
    pub min_width: Option<Dimension>,
    pub min_height: Option<Dimension>,
    pub max_width: Option<Dimension>,
    pub max_height: Option<Dimension>,

    // Positioning
    pub position: Option<Position>,
    pub top: Option<Dimension>,
    pub right: Option<Dimension>,
    pub bottom: Option<Dimension>,
    pub left: Option<Dimension>,
    pub display: Option<Display>,
    pub overflow: Option<Overflow>,
    pub z_index: Option<i32>,

    // Spacing
    pub padding: Option<f32>,
    pub padding_horizontal: Option<f32>,
    pub padding_vertical: Option<f32>,
    pub padding_top: Option<f32>,
    pub padding_right: Option<f32>,
    pub padding_bottom: Option<f32>,
    pub padding_left: Option<f32>,

    pub margin: Option<f32>,
    pub margin_horizontal: Option<f32>,
    pub margin_vertical: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub margin_left: Option<f32>,

    // Border
    pub border_width: Option<f32>,
    pub border_style: Option<BorderStyle>,
    pub border_color: Option<Color>,
    pub border_radius: Option<f32>,
    pub border_top_left_radius: Option<f32>,
    pub border_top_right_radius: Option<f32>,
    pub border_bottom_right_radius: Option<f32>,
    pub border_bottom_left_radius: Option<f32>,
    pub border_top_width: Option<f32>,
    pub border_right_width: Option<f32>,
    pub border_bottom_width: Option<f32>,
    pub border_left_width: Option<f32>,
    pub border_top_color: Option<Color>,
    pub border_right_color: Option<Color>,
    pub border_bottom_color: Option<Color>,
    pub border_left_color: Option<Color>,

    // Background
    pub background_color: Option<Color>,
    pub opacity: Option<f32>,

    // Gap (flexbox spacing)
    pub gap: Option<f32>,
    pub row_gap: Option<f32>,
    pub column_gap: Option<f32>,

    // Typography
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    pub line_height: Option<f32>,
    pub letter_spacing: Option<f32>,
    pub text_indent: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub max_lines: Option<usize>,
    pub text_overflow: Option<TextOverflow>,
    pub color: Option<Color>,
    pub text_align: Option<TextAlign>,
    pub text_decoration: Option<TextDecoration>,
    pub text_decoration_style: Option<TextDecorationStyle>,
    pub text_decoration_color: Option<Color>,

    // Image
    pub object_fit: Option<ObjectFit>,
    pub object_position: Option<ObjectPosition>,

    // Transforms
    pub transform: Vec<TransformOp>,
    pub transform_origin: Option<TransformOrigin>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextDecoration {
    #[default]
    None,
    Underline,
    LineThrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextDecorationStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextOverflow {
    #[default]
    Clip,
    Ellipsis,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum FlexDirection {
    Row,
    #[default]
    Column,
    RowReverse,
    ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignSelf {
    Auto,
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignItems {
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    Baseline,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AlignContent {
    FlexStart,
    FlexEnd,
    Center,
    Stretch,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Position {
    Relative,
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Display {
    #[default]
    Flex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    Points(f32),
    Percent(f32),
    ViewportWidth(f32),
    ViewportHeight(f32),
    Auto,
}

impl Default for Dimension {
    fn default() -> Self {
        Dimension::Auto
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BorderStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformOp {
    Rotate(f32),
    Scale(f32, f32),
    Translate(f32, f32),
    Skew(f32, f32),
    Matrix([f32; 6]),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformOrigin {
    pub x: f32,
    pub y: f32,
}

impl Default for TransformOrigin {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectPosition {
    pub x: f32,
    pub y: f32,
}

impl Default for ObjectPosition {
    fn default() -> Self {
        Self { x: 0.5, y: 0.5 }
    }
}

impl Color {
    pub fn new(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b }
    }

    pub fn black() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        // Try named CSS colors first.
        if let Some(c) = Self::from_named(hex) {
            return Some(c);
        }
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
            Some(Self::new(r, g, b))
        } else if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? as f32 * 17.0 / 255.0;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? as f32 * 17.0 / 255.0;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? as f32 * 17.0 / 255.0;
            Some(Self::new(r, g, b))
        } else {
            None
        }
    }

    fn from_named(name: &str) -> Option<Self> {
        let (r, g, b) = match name.to_ascii_lowercase().as_str() {
            "black" => (0, 0, 0),
            "white" => (255, 255, 255),
            "red" => (255, 0, 0),
            "green" => (0, 128, 0),
            "blue" => (0, 0, 255),
            "yellow" => (255, 255, 0),
            "cyan" | "aqua" => (0, 255, 255),
            "magenta" | "fuchsia" => (255, 0, 255),
            "gray" | "grey" => (128, 128, 128),
            "silver" => (192, 192, 192),
            "maroon" => (128, 0, 0),
            "olive" => (128, 128, 0),
            "lime" => (0, 255, 0),
            "teal" => (0, 128, 128),
            "navy" => (0, 0, 128),
            "purple" => (128, 0, 128),
            "orange" => (255, 165, 0),
            "pink" => (255, 192, 203),
            "brown" => (165, 42, 42),
            "coral" => (255, 127, 80),
            "crimson" => (220, 20, 60),
            "darkblue" => (0, 0, 139),
            "darkgreen" => (0, 100, 0),
            "darkgray" | "darkgrey" => (169, 169, 169),
            "darkred" => (139, 0, 0),
            "gold" => (255, 215, 0),
            "indigo" => (75, 0, 130),
            "ivory" => (255, 255, 240),
            "khaki" => (240, 230, 140),
            "lavender" => (230, 230, 250),
            "lightblue" => (173, 216, 230),
            "lightgray" | "lightgrey" => (211, 211, 211),
            "lightgreen" => (144, 238, 144),
            "lightyellow" => (255, 255, 224),
            "orangered" => (255, 69, 0),
            "orchid" => (218, 112, 214),
            "salmon" => (250, 128, 114),
            "skyblue" => (135, 206, 235),
            "slategray" | "slategrey" => (112, 128, 144),
            "steelblue" => (70, 130, 180),
            "tan" => (210, 180, 140),
            "tomato" => (255, 99, 71),
            "turquoise" => (64, 224, 208),
            "violet" => (238, 130, 238),
            "wheat" => (245, 222, 179),
            "transparent" => return Some(Self::new(0.0, 0.0, 0.0)),
            _ => return None,
        };
        Some(Self::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0))
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::black()
    }
}

/// Parse a CSS-like style string into a Style struct
impl Style {
    pub fn padding_top(&self) -> f32 {
        self.padding_top
            .or(self.padding_vertical)
            .or(self.padding)
            .unwrap_or(0.0)
    }
    pub fn padding_right(&self) -> f32 {
        self.padding_right
            .or(self.padding_horizontal)
            .or(self.padding)
            .unwrap_or(0.0)
    }
    pub fn padding_bottom(&self) -> f32 {
        self.padding_bottom
            .or(self.padding_vertical)
            .or(self.padding)
            .unwrap_or(0.0)
    }
    pub fn padding_left(&self) -> f32 {
        self.padding_left
            .or(self.padding_horizontal)
            .or(self.padding)
            .unwrap_or(0.0)
    }

    pub fn margin_top(&self) -> f32 {
        self.margin_top
            .or(self.margin_vertical)
            .or(self.margin)
            .unwrap_or(0.0)
    }
    pub fn margin_right(&self) -> f32 {
        self.margin_right
            .or(self.margin_horizontal)
            .or(self.margin)
            .unwrap_or(0.0)
    }
    pub fn margin_bottom(&self) -> f32 {
        self.margin_bottom
            .or(self.margin_vertical)
            .or(self.margin)
            .unwrap_or(0.0)
    }
    pub fn margin_left(&self) -> f32 {
        self.margin_left
            .or(self.margin_horizontal)
            .or(self.margin)
            .unwrap_or(0.0)
    }

    pub fn border_top_left_radius(&self) -> f32 {
        self.border_top_left_radius
            .or(self.border_radius)
            .unwrap_or(0.0)
    }
    pub fn border_top_right_radius(&self) -> f32 {
        self.border_top_right_radius
            .or(self.border_radius)
            .unwrap_or(0.0)
    }
    pub fn border_bottom_right_radius(&self) -> f32 {
        self.border_bottom_right_radius
            .or(self.border_radius)
            .unwrap_or(0.0)
    }
    pub fn border_bottom_left_radius(&self) -> f32 {
        self.border_bottom_left_radius
            .or(self.border_radius)
            .unwrap_or(0.0)
    }

    pub fn border_top_width(&self) -> f32 {
        self.border_top_width.or(self.border_width).unwrap_or(0.0)
    }
    pub fn border_right_width(&self) -> f32 {
        self.border_right_width.or(self.border_width).unwrap_or(0.0)
    }
    pub fn border_bottom_width(&self) -> f32 {
        self.border_bottom_width.or(self.border_width).unwrap_or(0.0)
    }
    pub fn border_left_width(&self) -> f32 {
        self.border_left_width.or(self.border_width).unwrap_or(0.0)
    }

    pub fn border_top_color(&self) -> Color {
        self.border_top_color
            .or(self.border_color)
            .unwrap_or(Color::black())
    }
    pub fn border_right_color(&self) -> Color {
        self.border_right_color
            .or(self.border_color)
            .unwrap_or(Color::black())
    }
    pub fn border_bottom_color(&self) -> Color {
        self.border_bottom_color
            .or(self.border_color)
            .unwrap_or(Color::black())
    }
    pub fn border_left_color(&self) -> Color {
        self.border_left_color
            .or(self.border_color)
            .unwrap_or(Color::black())
    }
}

pub fn apply_text_transform(text: &str, transform: Option<TextTransform>) -> String {
    match transform.unwrap_or(TextTransform::None) {
        TextTransform::None => text.to_string(),
        TextTransform::Uppercase => text.to_uppercase(),
        TextTransform::Lowercase => text.to_lowercase(),
        TextTransform::Capitalize => {
            let mut result = String::with_capacity(text.len());
            let mut new_word = true;
            for ch in text.chars() {
                if ch.is_alphanumeric() {
                    if new_word {
                        for upper in ch.to_uppercase() {
                            result.push(upper);
                        }
                        new_word = false;
                    } else {
                        result.push(ch);
                    }
                } else {
                    new_word = true;
                    result.push(ch);
                }
            }
            result
        }
    }
}
