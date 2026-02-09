use taffy::prelude::*;

use crate::style::{
    AlignContent as StyleAlignContent,
    AlignItems as StyleAlignItems,
    AlignSelf as StyleAlignSelf,
    Dimension as StyleDimension,
    Display as StyleDisplay,
    FlexDirection as StyleFlexDirection,
    FlexWrap as StyleFlexWrap,
    JustifyContent as StyleJustifyContent,
    Overflow as StyleOverflow,
    Position as StylePosition,
    Style,
};

pub(super) fn convert_style_to_taffy(
    style: &Style,
    viewport_width: f32,
    viewport_height: f32,
) -> taffy::Style {
    let mut taffy_style = taffy::Style::default();

    taffy_style.box_sizing = BoxSizing::BorderBox;
    taffy_style.display = taffy::Display::Flex;

    // React-PDF (React Native) defaults to column layout for Views.
    taffy_style.flex_direction = FlexDirection::Column;

    // React-PDF defaults to flexShrink=1 (Yoga behavior).
    taffy_style.flex_shrink = 1.0;

    // Allow horizontal shrink like Yoga without breaking vertical padding sizing.
    taffy_style.min_size.width = Dimension::Length(0.0);
    taffy_style.min_size.height = Dimension::Auto;

    // Set default align_items to Stretch (CSS default for flexbox)
    taffy_style.align_items = Some(taffy::AlignItems::Stretch);

    // Flex direction
    if let Some(dir) = &style.flex_direction {
        taffy_style.flex_direction = match dir {
            StyleFlexDirection::Row => FlexDirection::Row,
            StyleFlexDirection::Column => FlexDirection::Column,
            StyleFlexDirection::RowReverse => FlexDirection::RowReverse,
            StyleFlexDirection::ColumnReverse => FlexDirection::ColumnReverse,
        };
    }

    // Flex
    if let Some(f) = style.flex {
        taffy_style.flex_grow = f;
        taffy_style.flex_shrink = 1.0;
        taffy_style.flex_basis = Dimension::Length(0.0);
        // Allow flex items to shrink below their content size horizontally
        taffy_style.min_size.width = Dimension::Length(0.0);
        taffy_style.min_size.height = Dimension::Auto;
    }
    if let Some(fg) = style.flex_grow {
        taffy_style.flex_grow = fg;
    }
    if let Some(fs) = style.flex_shrink {
        taffy_style.flex_shrink = fs;
    }
    if let Some(fb) = &style.flex_basis {
        taffy_style.flex_basis = convert_dimension(fb, viewport_width, viewport_height);
    }
    if let Some(align) = &style.align_self {
        taffy_style.align_self = match align {
            StyleAlignSelf::Auto => None, // Auto means inherit from parent
            StyleAlignSelf::FlexStart => Some(taffy::AlignSelf::FlexStart),
            StyleAlignSelf::FlexEnd => Some(taffy::AlignSelf::FlexEnd),
            StyleAlignSelf::Center => Some(taffy::AlignSelf::Center),
            StyleAlignSelf::Stretch => Some(taffy::AlignSelf::Stretch),
        };
    }
    if let Some(align_items) = &style.align_items {
        taffy_style.align_items = Some(match align_items {
            StyleAlignItems::FlexStart => taffy::AlignItems::FlexStart,
            StyleAlignItems::FlexEnd => taffy::AlignItems::FlexEnd,
            StyleAlignItems::Center => taffy::AlignItems::Center,
            StyleAlignItems::Stretch => taffy::AlignItems::Stretch,
            StyleAlignItems::Baseline => taffy::AlignItems::Baseline,
        });
    }
    if let Some(align_content) = &style.align_content {
        taffy_style.align_content = Some(match align_content {
            StyleAlignContent::FlexStart => taffy::AlignContent::FlexStart,
            StyleAlignContent::FlexEnd => taffy::AlignContent::FlexEnd,
            StyleAlignContent::Center => taffy::AlignContent::Center,
            StyleAlignContent::Stretch => taffy::AlignContent::Stretch,
            StyleAlignContent::SpaceBetween => taffy::AlignContent::SpaceBetween,
            StyleAlignContent::SpaceAround => taffy::AlignContent::SpaceAround,
            StyleAlignContent::SpaceEvenly => taffy::AlignContent::SpaceEvenly,
        });
    }
    if let Some(justify) = &style.justify_content {
        taffy_style.justify_content = Some(match justify {
            StyleJustifyContent::FlexStart => taffy::JustifyContent::FlexStart,
            StyleJustifyContent::FlexEnd => taffy::JustifyContent::FlexEnd,
            StyleJustifyContent::Center => taffy::JustifyContent::Center,
            StyleJustifyContent::SpaceBetween => taffy::JustifyContent::SpaceBetween,
            StyleJustifyContent::SpaceAround => taffy::JustifyContent::SpaceAround,
            StyleJustifyContent::SpaceEvenly => taffy::JustifyContent::SpaceEvenly,
        });
    }
    if let Some(wrap) = &style.flex_wrap {
        taffy_style.flex_wrap = match wrap {
            StyleFlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
            StyleFlexWrap::Wrap => taffy::FlexWrap::Wrap,
            StyleFlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
        };
    }

    // Dimensions
    if let Some(w) = &style.width {
        taffy_style.size.width = convert_dimension(w, viewport_width, viewport_height);
    }
    if let Some(h) = &style.height {
        taffy_style.size.height = convert_dimension(h, viewport_width, viewport_height);
    }
    if let Some(w) = &style.min_width {
        taffy_style.min_size.width = convert_dimension(w, viewport_width, viewport_height);
    }
    if let Some(h) = &style.min_height {
        taffy_style.min_size.height = convert_dimension(h, viewport_width, viewport_height);
    }
    if let Some(w) = &style.max_width {
        taffy_style.max_size.width = convert_dimension(w, viewport_width, viewport_height);
    }
    if let Some(h) = &style.max_height {
        taffy_style.max_size.height = convert_dimension(h, viewport_width, viewport_height);
    }

    // Positioning
    if let Some(position) = &style.position {
        taffy_style.position = match position {
            StylePosition::Relative => taffy::Position::Relative,
            StylePosition::Absolute => taffy::Position::Absolute,
        };
    }
    if let Some(display) = &style.display {
        taffy_style.display = match display {
            StyleDisplay::None => taffy::Display::None,
            StyleDisplay::Flex => taffy::Display::Flex,
        };
    }
    if let Some(overflow) = &style.overflow {
        let mapped = match overflow {
            StyleOverflow::Hidden => taffy::Overflow::Hidden,
            StyleOverflow::Visible => taffy::Overflow::Visible,
        };
        taffy_style.overflow = taffy::Point { x: mapped, y: mapped };
    }
    if style.top.is_some() || style.right.is_some() || style.bottom.is_some() || style.left.is_some() {
        taffy_style.inset = Rect {
            top: convert_inset(style.top.as_ref(), viewport_width, viewport_height),
            right: convert_inset(style.right.as_ref(), viewport_width, viewport_height),
            bottom: convert_inset(style.bottom.as_ref(), viewport_width, viewport_height),
            left: convert_inset(style.left.as_ref(), viewport_width, viewport_height),
        };
    }

    // Padding
    taffy_style.padding = Rect {
        top: LengthPercentage::Length(style.padding_top()),
        right: LengthPercentage::Length(style.padding_right()),
        bottom: LengthPercentage::Length(style.padding_bottom()),
        left: LengthPercentage::Length(style.padding_left()),
    };

    // Margin
    taffy_style.margin = Rect {
        top: LengthPercentageAuto::Length(style.margin_top()),
        right: LengthPercentageAuto::Length(style.margin_right()),
        bottom: LengthPercentageAuto::Length(style.margin_bottom()),
        left: LengthPercentageAuto::Length(style.margin_left()),
    };

    // Border (Taffy treats border as part of the box model)
    let bw_top = style.border_top_width();
    let bw_right = style.border_right_width();
    let bw_bottom = style.border_bottom_width();
    let bw_left = style.border_left_width();
    if bw_top > 0.0 || bw_right > 0.0 || bw_bottom > 0.0 || bw_left > 0.0 {
        taffy_style.border = Rect {
            top: LengthPercentage::Length(bw_top),
            right: LengthPercentage::Length(bw_right),
            bottom: LengthPercentage::Length(bw_bottom),
            left: LengthPercentage::Length(bw_left),
        };
    }

    // Gap
    if let Some(g) = style.gap {
        taffy_style.gap = Size {
            width: LengthPercentage::Length(g),
            height: LengthPercentage::Length(g),
        };
    }
    if let Some(rg) = style.row_gap {
        taffy_style.gap.height = LengthPercentage::Length(rg);
    }
    if let Some(cg) = style.column_gap {
        taffy_style.gap.width = LengthPercentage::Length(cg);
    }

    taffy_style
}

pub(super) fn auto_height_override(
    view: &crate::components::View,
    viewport_width: f32,
    viewport_height: f32,
) -> Option<f32> {
    if !matches!(view.style.height, None | Some(StyleDimension::Auto)) {
        return None;
    }

    let flex_direction = view
        .style
        .flex_direction
        .unwrap_or(StyleFlexDirection::Column);
    if !matches!(flex_direction, StyleFlexDirection::Column | StyleFlexDirection::ColumnReverse) {
        return None;
    }

    let mut total = 0.0;
    for child in &view.children {
        let child_style = child.style();
        let height = explicit_height(child_style, viewport_width, viewport_height)?;
        total += height;
        total += child_style.margin_top() + child_style.margin_bottom();
    }

    let gap = view.style.row_gap.or(view.style.gap).unwrap_or(0.0);
    if view.children.len() > 1 {
        total += gap * (view.children.len() as f32 - 1.0);
    }

    total += view.style.padding_top() + view.style.padding_bottom();
    total += view.style.border_top_width() + view.style.border_bottom_width();

    Some(total)
}

fn convert_dimension(dim: &StyleDimension, viewport_width: f32, viewport_height: f32) -> Dimension {
    match dim {
        StyleDimension::Points(p) => Dimension::Length(*p),
        StyleDimension::Percent(p) => Dimension::Percent(*p / 100.0),
        StyleDimension::ViewportWidth(p) => Dimension::Length(viewport_width * (*p / 100.0)),
        StyleDimension::ViewportHeight(p) => Dimension::Length(viewport_height * (*p / 100.0)),
        StyleDimension::Auto => Dimension::Auto,
    }
}

fn explicit_height(style: &Style, viewport_width: f32, viewport_height: f32) -> Option<f32> {
    match style.height? {
        StyleDimension::Points(value) => Some(value),
        StyleDimension::ViewportWidth(value) => Some(viewport_width * (value / 100.0)),
        StyleDimension::ViewportHeight(value) => Some(viewport_height * (value / 100.0)),
        StyleDimension::Percent(_) | StyleDimension::Auto => None,
    }
}

fn convert_inset(
    dim: Option<&StyleDimension>,
    viewport_width: f32,
    viewport_height: f32,
) -> LengthPercentageAuto {
    match dim {
        Some(StyleDimension::Points(p)) => LengthPercentageAuto::Length(*p),
        Some(StyleDimension::Percent(p)) => LengthPercentageAuto::Percent(*p / 100.0),
        Some(StyleDimension::ViewportWidth(p)) => {
            LengthPercentageAuto::Length(viewport_width * (*p / 100.0))
        }
        Some(StyleDimension::ViewportHeight(p)) => {
            LengthPercentageAuto::Length(viewport_height * (*p / 100.0))
        }
        Some(StyleDimension::Auto) | None => LengthPercentageAuto::Auto,
    }
}
