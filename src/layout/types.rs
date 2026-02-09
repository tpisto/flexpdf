use crate::style::{FontStyle, TextAlign, TextOverflow, TextTransform};

/// Layout result for a node.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayoutRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub content_x: f32,
    pub content_y: f32,
    pub content_width: f32,
    pub content_height: f32,
}

impl LayoutRect {
    pub fn content_rect(&self) -> LayoutRect {
        LayoutRect {
            x: self.content_x,
            y: self.content_y,
            width: self.content_width,
            height: self.content_height,
            content_x: self.content_x,
            content_y: self.content_y,
            content_width: self.content_width,
            content_height: self.content_height,
        }
    }
}

/// Context stored with each Taffy node.
#[derive(Debug, Clone)]
pub struct NodeContext {
    pub component_type: ComponentType,
    pub text_content: Option<String>,
    pub font_family: Option<String>,
    pub font_size: f32,
    pub font_weight: Option<u16>,
    pub font_style: Option<FontStyle>,
    pub line_height: Option<f32>,
    pub text_align: Option<TextAlign>,
    pub letter_spacing: Option<f32>,
    pub text_indent: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub max_lines: Option<usize>,
    pub text_overflow: Option<TextOverflow>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComponentType {
    Page,
    View,
    Text,
    Image,
    Link,
    Note,
}

#[derive(Debug)]
pub struct LayoutError(pub String);

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for LayoutError {}
