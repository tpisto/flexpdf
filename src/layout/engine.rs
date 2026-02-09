use std::collections::HashMap;

use taffy::prelude::*;
use taffy::TaffyTree;

use crate::components::{Component, Page};
use crate::style::{apply_text_transform, FontStyle, TextAlign, TextOverflow};

use super::style::{auto_height_override, convert_style_to_taffy};
use super::types::{ComponentType, LayoutError, LayoutRect, NodeContext};

/// Layout engine wrapping Taffy.
pub struct LayoutEngine {
    taffy: TaffyTree<NodeContext>,
    viewport_width: f32,
    viewport_height: f32,
}

impl LayoutEngine {
    pub fn new() -> Self {
        let mut taffy = TaffyTree::new();
        // React-PDF uses float layout values; avoid pixel rounding in Taffy.
        taffy.disable_rounding();
        Self {
            taffy,
            viewport_width: 0.0,
            viewport_height: 0.0,
        }
    }

    /// Build layout tree from page and compute layout.
    pub fn compute_page_layout(
        &mut self,
        page: &Page,
        measure_text: &dyn Fn(
            &str,
            Option<&str>,
            f32,
            Option<u16>,
            Option<FontStyle>,
            Option<f32>,
            Option<TextAlign>,
            Option<f32>,
            Option<f32>,
            Option<usize>,
            Option<TextOverflow>,
            f32,
        ) -> (f32, f32),
    ) -> Result<(NodeId, HashMap<NodeId, LayoutRect>), LayoutError> {
        let (page_width, page_height) = page.dimensions();
        self.viewport_width = page_width;
        self.viewport_height = page_height;

        // Build the tree
        let root = self.build_page_node(page)?;

        // Compute layout with text measurement
        self.taffy
            .compute_layout_with_measure(
                root,
                Size {
                    width: AvailableSpace::Definite(page_width),
                    height: AvailableSpace::Definite(page_height),
                },
                |known_size, available_space, _node_id, context, _style| {
                    if let Some(ctx) = context {
                        if ctx.component_type == ComponentType::Text {
                            if let Some(ref text) = ctx.text_content {
                                let available_width = match available_space.width {
                                    AvailableSpace::Definite(w) => w,
                                    AvailableSpace::MaxContent => f32::MAX,
                                    AvailableSpace::MinContent => 0.0,
                                };

                                let transformed =
                                    apply_text_transform(text, ctx.text_transform);
                                let (w, h) = measure_text(
                                    &transformed,
                                    ctx.font_family.as_deref(),
                                    ctx.font_size,
                                    ctx.font_weight,
                                    ctx.font_style,
                                    ctx.line_height,
                                    ctx.text_align,
                                    ctx.letter_spacing,
                                    ctx.text_indent,
                                    ctx.max_lines,
                                    ctx.text_overflow,
                                    available_width,
                                );

                                return Size {
                                    width: known_size.width.unwrap_or(w),
                                    height: known_size.height.unwrap_or(h),
                                };
                            }
                        }
                    }

                    Size::ZERO
                },
            )
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        // Extract computed layouts
        let layouts = self.extract_layouts(root, 0.0, 0.0)?;

        Ok((root, layouts))
    }

    fn build_page_node(&mut self, page: &Page) -> Result<NodeId, LayoutError> {
        let (page_width, page_height) = page.dimensions();

        let mut style =
            convert_style_to_taffy(&page.style, self.viewport_width, self.viewport_height);
        style.size = Size {
            width: Dimension::Length(page_width),
            height: Dimension::Length(page_height),
        };

        let context = NodeContext {
            component_type: ComponentType::Page,
            text_content: None,
            font_family: None,
            font_size: 12.0,
            font_weight: None,
            font_style: None,
            line_height: None,
            text_align: None,
            letter_spacing: None,
            text_indent: None,
            text_transform: None,
            max_lines: None,
            text_overflow: None,
        };

        let children: Vec<NodeId> = page
            .children
            .iter()
            .filter_map(|c| self.build_component_node(c).ok())
            .collect();

        let node = self
            .taffy
            .new_with_children(style, &children)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        self.taffy
            .set_node_context(node, Some(context))
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        Ok(node)
    }

    fn build_component_node(&mut self, component: &Component) -> Result<NodeId, LayoutError> {
        match component {
            Component::View(view) => self.build_view_node(view),
            Component::Text(text) => self.build_text_node(text),
            Component::Image(image) => self.build_image_node(image),
            Component::Link(link) => self.build_link_node(link),
            Component::Note(note) => self.build_note_node(note),
        }
    }

    fn build_view_node(&mut self, view: &crate::components::View) -> Result<NodeId, LayoutError> {
        let mut style =
            convert_style_to_taffy(&view.style, self.viewport_width, self.viewport_height);

        if let Some(auto_height) =
            auto_height_override(view, self.viewport_width, self.viewport_height)
        {
            style.size.height = Dimension::Length(auto_height);
        }

        let context = NodeContext {
            component_type: ComponentType::View,
            text_content: None,
            font_family: view.style.font_family.clone(),
            font_size: view.style.font_size.unwrap_or(12.0),
            font_weight: view.style.font_weight,
            font_style: view.style.font_style,
            line_height: view.style.line_height,
            text_align: view.style.text_align,
            letter_spacing: view.style.letter_spacing,
            text_indent: view.style.text_indent,
            text_transform: view.style.text_transform,
            max_lines: view.style.max_lines,
            text_overflow: view.style.text_overflow,
        };

        let children: Vec<NodeId> = view
            .children
            .iter()
            .filter_map(|c| self.build_component_node(c).ok())
            .collect();

        let node = self
            .taffy
            .new_with_children(style, &children)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        self.taffy
            .set_node_context(node, Some(context))
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        Ok(node)
    }

    fn build_link_node(&mut self, link: &crate::components::Link) -> Result<NodeId, LayoutError> {
        let style =
            convert_style_to_taffy(&link.style, self.viewport_width, self.viewport_height);

        let context = NodeContext {
            component_type: ComponentType::Link,
            text_content: None,
            font_family: link.style.font_family.clone(),
            font_size: link.style.font_size.unwrap_or(12.0),
            font_weight: link.style.font_weight,
            font_style: link.style.font_style,
            line_height: link.style.line_height,
            text_align: link.style.text_align,
            letter_spacing: link.style.letter_spacing,
            text_indent: link.style.text_indent,
            text_transform: link.style.text_transform,
            max_lines: link.style.max_lines,
            text_overflow: link.style.text_overflow,
        };

        let children: Vec<NodeId> = link
            .children
            .iter()
            .filter_map(|c| self.build_component_node(c).ok())
            .collect();

        let node = self
            .taffy
            .new_with_children(style, &children)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        self.taffy
            .set_node_context(node, Some(context))
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        Ok(node)
    }

    fn build_note_node(&mut self, note: &crate::components::Note) -> Result<NodeId, LayoutError> {
        let style =
            convert_style_to_taffy(&note.style, self.viewport_width, self.viewport_height);

        let context = NodeContext {
            component_type: ComponentType::Note,
            text_content: Some(note.content.clone()),
            font_family: note.style.font_family.clone(),
            font_size: note.style.font_size.unwrap_or(12.0),
            font_weight: note.style.font_weight,
            font_style: note.style.font_style,
            line_height: note.style.line_height,
            text_align: note.style.text_align,
            letter_spacing: note.style.letter_spacing,
            text_indent: note.style.text_indent,
            text_transform: note.style.text_transform,
            max_lines: note.style.max_lines,
            text_overflow: note.style.text_overflow,
        };

        let node = self
            .taffy
            .new_leaf(style)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        self.taffy
            .set_node_context(node, Some(context))
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        Ok(node)
    }

    fn build_text_node(&mut self, text: &crate::components::Text) -> Result<NodeId, LayoutError> {
        let mut style =
            convert_style_to_taffy(&text.style, self.viewport_width, self.viewport_height);

        // Text nodes: if no explicit width, use auto (content-sized)
        // Height is always content-based
        if text.style.width.is_none() {
            style.size.width = Dimension::Auto;
        }
        style.size.height = Dimension::Auto;

        let font_size = text.style.font_size.unwrap_or(12.0);
        let font_weight = text.style.font_weight;
        let font_style = text.style.font_style;
        let line_height = text.style.line_height;

        // Use full_text() to get concatenated content for measurement
        // (handles both simple content and spans)
        let context = NodeContext {
            component_type: ComponentType::Text,
            text_content: Some(text.full_text()),
            font_family: text.style.font_family.clone(),
            font_size,
            font_weight,
            font_style,
            line_height,
            text_align: text.style.text_align,
            letter_spacing: text.style.letter_spacing,
            text_indent: text.style.text_indent,
            text_transform: text.style.text_transform,
            max_lines: text.style.max_lines,
            text_overflow: text.style.text_overflow,
        };

        let node = self
            .taffy
            .new_leaf(style)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        self.taffy
            .set_node_context(node, Some(context))
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        Ok(node)
    }

    fn build_image_node(&mut self, image: &crate::components::Image) -> Result<NodeId, LayoutError> {
        let style =
            convert_style_to_taffy(&image.style, self.viewport_width, self.viewport_height);

        let context = NodeContext {
            component_type: ComponentType::Image,
            text_content: None,
            font_family: None,
            font_size: 12.0,
            font_weight: None,
            font_style: None,
            line_height: None,
            text_align: None,
            letter_spacing: None,
            text_indent: None,
            text_transform: None,
            max_lines: None,
            text_overflow: None,
        };

        let node = self
            .taffy
            .new_leaf(style)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        self.taffy
            .set_node_context(node, Some(context))
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;

        Ok(node)
    }

    fn extract_layouts(
        &self,
        node: NodeId,
        parent_x: f32,
        parent_y: f32,
    ) -> Result<HashMap<NodeId, LayoutRect>, LayoutError> {
        let mut layouts = HashMap::new();

        let layout = self
            .taffy
            .layout(node)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;
        let x = parent_x + layout.location.x;
        let y = parent_y + layout.location.y;

        layouts.insert(
            node,
            LayoutRect {
                x,
                y,
                width: layout.size.width,
                height: layout.size.height,
                content_x: parent_x + layout.content_box_x(),
                content_y: parent_y + layout.content_box_y(),
                content_width: layout.content_box_width(),
                content_height: layout.content_box_height(),
            },
        );

        let children = self
            .taffy
            .children(node)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))?;
        for child in children {
            layouts.extend(self.extract_layouts(child, x, y)?);
        }

        Ok(layouts)
    }

    pub fn get_context(&self, node: NodeId) -> Option<&NodeContext> {
        self.taffy.get_node_context(node)
    }

    pub fn children(&self, node: NodeId) -> Result<Vec<NodeId>, LayoutError> {
        self.taffy
            .children(node)
            .map_err(|e| LayoutError(format!("Taffy error: {:?}", e)))
    }

    pub fn raw_layout(&self, node: NodeId) -> Option<taffy::Layout> {
        self.taffy.layout(node).ok().copied()
    }

    pub fn raw_style(&self, node: NodeId) -> Option<taffy::Style> {
        self.taffy.style(node).ok().cloned()
    }
}
