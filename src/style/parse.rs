//! Parsing helpers for style strings.

use super::types::*;
use crate::components::ObjectFit;

pub fn parse_style(style_str: &str) -> Style {
    let mut style = Style::default();

    for declaration in style_str.split(';') {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }

        if let Some((property, value)) = declaration.split_once(':') {
            let property = property.trim();
            let value = value.trim();
            parse_property(&mut style, property, value);
        }
    }

    style
}

fn parse_property(style: &mut Style, property: &str, value: &str) {
    match property {
        // Flexbox
        "flexDirection" | "flex-direction" => {
            style.flex_direction = Some(match value {
                "row" => FlexDirection::Row,
                "column" => FlexDirection::Column,
                "row-reverse" => FlexDirection::RowReverse,
                "column-reverse" => FlexDirection::ColumnReverse,
                _ => FlexDirection::Column,
            });
        }
        "flex" => {
            if let Ok(v) = value.parse::<f32>() {
                style.flex = Some(v);
            }
        }
        "flexGrow" | "flex-grow" => {
            if let Ok(v) = value.parse::<f32>() {
                style.flex_grow = Some(v);
            }
        }
        "flexShrink" | "flex-shrink" => {
            if let Ok(v) = value.parse::<f32>() {
                style.flex_shrink = Some(v);
            }
        }
        "flexBasis" | "flex-basis" => {
            style.flex_basis = Some(parse_dimension(value));
        }
        "alignSelf" | "align-self" => {
            style.align_self = Some(match value {
                "auto" => AlignSelf::Auto,
                "flex-start" | "flexStart" | "start" => AlignSelf::FlexStart,
                "flex-end" | "flexEnd" | "end" => AlignSelf::FlexEnd,
                "center" => AlignSelf::Center,
                "stretch" => AlignSelf::Stretch,
                _ => AlignSelf::Auto,
            });
        }
        "alignItems" | "align-items" => {
            style.align_items = Some(match value {
                "flex-start" | "flexStart" | "start" => AlignItems::FlexStart,
                "flex-end" | "flexEnd" | "end" => AlignItems::FlexEnd,
                "center" => AlignItems::Center,
                "baseline" => AlignItems::Baseline,
                "stretch" => AlignItems::Stretch,
                _ => AlignItems::Stretch,
            });
        }
        "alignContent" | "align-content" => {
            style.align_content = Some(match value {
                "flex-start" | "flexStart" | "start" => AlignContent::FlexStart,
                "flex-end" | "flexEnd" | "end" => AlignContent::FlexEnd,
                "center" => AlignContent::Center,
                "space-between" => AlignContent::SpaceBetween,
                "space-around" => AlignContent::SpaceAround,
                "space-evenly" => AlignContent::SpaceEvenly,
                "stretch" => AlignContent::Stretch,
                _ => AlignContent::Stretch,
            });
        }
        "justifyContent" | "justify-content" => {
            style.justify_content = Some(match value {
                "flex-start" | "flexStart" | "start" => JustifyContent::FlexStart,
                "flex-end" | "flexEnd" | "end" => JustifyContent::FlexEnd,
                "center" => JustifyContent::Center,
                "space-between" => JustifyContent::SpaceBetween,
                "space-around" => JustifyContent::SpaceAround,
                "space-evenly" => JustifyContent::SpaceEvenly,
                _ => JustifyContent::FlexStart,
            });
        }
        "flexWrap" | "flex-wrap" => {
            style.flex_wrap = Some(match value {
                "wrap" => FlexWrap::Wrap,
                "wrap-reverse" => FlexWrap::WrapReverse,
                "nowrap" => FlexWrap::NoWrap,
                _ => FlexWrap::NoWrap,
            });
        }
        "flexFlow" | "flex-flow" => {
            // React-PDF ignores flexFlow; keep as a no-op for parity.
        }

        // Dimensions
        "width" => {
            style.width = Some(parse_dimension(value));
        }
        "height" => {
            style.height = Some(parse_dimension(value));
        }
        "minWidth" | "min-width" => {
            style.min_width = Some(parse_dimension(value));
        }
        "minHeight" | "min-height" => {
            style.min_height = Some(parse_dimension(value));
        }
        "maxWidth" | "max-width" => {
            style.max_width = Some(parse_dimension(value));
        }
        "maxHeight" | "max-height" => {
            style.max_height = Some(parse_dimension(value));
        }
        "top" => {
            style.top = Some(parse_dimension(value));
        }
        "right" => {
            style.right = Some(parse_dimension(value));
        }
        "bottom" => {
            style.bottom = Some(parse_dimension(value));
        }
        "left" => {
            style.left = Some(parse_dimension(value));
        }

        // Positioning
        "position" => {
            style.position = Some(match value {
                "absolute" => Position::Absolute,
                _ => Position::Relative,
            });
        }
        "display" => {
            style.display = Some(match value {
                "none" => Display::None,
                _ => Display::Flex,
            });
        }
        "overflow" => {
            style.overflow = Some(match value {
                "hidden" => Overflow::Hidden,
                _ => Overflow::Visible,
            });
        }
        "zIndex" | "z-index" => {
            if let Ok(v) = value.parse::<i32>() {
                style.z_index = Some(v);
            }
        }

        // Padding
        "padding" => {
            if let Some((top, right, bottom, left)) = parse_box_shorthand(value) {
                style.padding_top = Some(top);
                style.padding_right = Some(right);
                style.padding_bottom = Some(bottom);
                style.padding_left = Some(left);
            } else if let Some(v) = parse_length(value) {
                style.padding = Some(v);
            }
        }
        "paddingHorizontal" | "padding-horizontal" => {
            if let Some(v) = parse_length(value) {
                style.padding_horizontal = Some(v);
            }
        }
        "paddingVertical" | "padding-vertical" => {
            if let Some(v) = parse_length(value) {
                style.padding_vertical = Some(v);
            }
        }
        "paddingTop" | "padding-top" => {
            if let Some(v) = parse_length(value) {
                style.padding_top = Some(v);
            }
        }
        "paddingRight" | "padding-right" => {
            if let Some(v) = parse_length(value) {
                style.padding_right = Some(v);
            }
        }
        "paddingBottom" | "padding-bottom" => {
            if let Some(v) = parse_length(value) {
                style.padding_bottom = Some(v);
            }
        }
        "paddingLeft" | "padding-left" => {
            if let Some(v) = parse_length(value) {
                style.padding_left = Some(v);
            }
        }

        // Margin
        "margin" => {
            if let Some((top, right, bottom, left)) = parse_box_shorthand(value) {
                style.margin_top = Some(top);
                style.margin_right = Some(right);
                style.margin_bottom = Some(bottom);
                style.margin_left = Some(left);
            } else if let Some(v) = parse_length(value) {
                style.margin = Some(v);
            }
        }
        "marginHorizontal" | "margin-horizontal" => {
            if let Some(v) = parse_length(value) {
                style.margin_horizontal = Some(v);
            }
        }
        "marginVertical" | "margin-vertical" => {
            if let Some(v) = parse_length(value) {
                style.margin_vertical = Some(v);
            }
        }
        "marginTop" | "margin-top" => {
            if let Some(v) = parse_length(value) {
                style.margin_top = Some(v);
            }
        }
        "marginRight" | "margin-right" => {
            if let Some(v) = parse_length(value) {
                style.margin_right = Some(v);
            }
        }
        "marginBottom" | "margin-bottom" => {
            if let Some(v) = parse_length(value) {
                style.margin_bottom = Some(v);
            }
        }
        "marginLeft" | "margin-left" => {
            if let Some(v) = parse_length(value) {
                style.margin_left = Some(v);
            }
        }

        // Border
        "border" => {
            let mut width = None;
            let mut color = None;
            let mut style_value = None;
            for part in value.split_whitespace() {
                if width.is_none() {
                    if let Some(v) = parse_length(part) {
                        width = Some(v);
                        continue;
                    }
                }
                if style_value.is_none() {
                    style_value = match part {
                        "solid" => Some(BorderStyle::Solid),
                        "dashed" => Some(BorderStyle::Dashed),
                        "dotted" => Some(BorderStyle::Dotted),
                        _ => None,
                    };
                    if style_value.is_some() {
                        continue;
                    }
                }
                if color.is_none() {
                    color = Color::from_hex(part);
                }
            }
            if let Some(v) = width {
                style.border_width = Some(v);
            }
            if let Some(c) = color {
                style.border_color = Some(c);
            }
            if let Some(s) = style_value {
                style.border_style = Some(s);
            }
        }
        "borderWidth" | "border-width" => {
            if let Some(v) = parse_length(value) {
                style.border_width = Some(v);
            }
        }
        "borderStyle" | "border-style" => {
            style.border_style = Some(match value {
                "dashed" => BorderStyle::Dashed,
                "dotted" => BorderStyle::Dotted,
                _ => BorderStyle::Solid,
            });
        }
        "borderColor" | "border-color" => {
            if let Some(c) = Color::from_hex(value) {
                style.border_color = Some(c);
            }
        }
        "borderRadius" | "border-radius" => {
            if let Some(v) = parse_length(value) {
                style.border_radius = Some(v);
            }
        }
        "borderTopLeftRadius" | "border-top-left-radius" => {
            if let Some(v) = parse_length(value) {
                style.border_top_left_radius = Some(v);
            }
        }
        "borderTopRightRadius" | "border-top-right-radius" => {
            if let Some(v) = parse_length(value) {
                style.border_top_right_radius = Some(v);
            }
        }
        "borderBottomRightRadius" | "border-bottom-right-radius" => {
            if let Some(v) = parse_length(value) {
                style.border_bottom_right_radius = Some(v);
            }
        }
        "borderBottomLeftRadius" | "border-bottom-left-radius" => {
            if let Some(v) = parse_length(value) {
                style.border_bottom_left_radius = Some(v);
            }
        }
        "borderTopWidth" | "border-top-width" => {
            if let Some(v) = parse_length(value) {
                style.border_top_width = Some(v);
            }
        }
        "borderRightWidth" | "border-right-width" => {
            if let Some(v) = parse_length(value) {
                style.border_right_width = Some(v);
            }
        }
        "borderBottomWidth" | "border-bottom-width" => {
            if let Some(v) = parse_length(value) {
                style.border_bottom_width = Some(v);
            }
        }
        "borderLeftWidth" | "border-left-width" => {
            if let Some(v) = parse_length(value) {
                style.border_left_width = Some(v);
            }
        }
        "borderTopColor" | "border-top-color" => {
            if let Some(c) = Color::from_hex(value) {
                style.border_top_color = Some(c);
            }
        }
        "borderRightColor" | "border-right-color" => {
            if let Some(c) = Color::from_hex(value) {
                style.border_right_color = Some(c);
            }
        }
        "borderBottomColor" | "border-bottom-color" => {
            if let Some(c) = Color::from_hex(value) {
                style.border_bottom_color = Some(c);
            }
        }
        "borderLeftColor" | "border-left-color" => {
            if let Some(c) = Color::from_hex(value) {
                style.border_left_color = Some(c);
            }
        }

        // Background
        "backgroundColor" | "background-color" | "background" => {
            if let Some(c) = Color::from_hex(value) {
                style.background_color = Some(c);
            }
        }
        "opacity" => {
            if let Ok(v) = value.parse::<f32>() {
                let clamped = v.clamp(0.0, 1.0);
                style.opacity = Some(clamped);
            }
        }

        // Gap
        "gap" => {
            if let Some(v) = parse_length(value) {
                style.gap = Some(v);
            }
        }
        "rowGap" | "row-gap" => {
            if let Some(v) = parse_length(value) {
                style.row_gap = Some(v);
            }
        }
        "columnGap" | "column-gap" => {
            if let Some(v) = parse_length(value) {
                style.column_gap = Some(v);
            }
        }

        // Typography
        "fontFamily" | "font-family" => {
            // Accept quoted or unquoted values, and URLs
            let family = value
                .trim_matches('"')
                .trim_matches('\'')
                .trim();
            if !family.is_empty() {
                style.font_family = Some(family.to_string());
            }
        }
        "fontSize" | "font-size" => {
            if let Some(v) = parse_length(value) {
                style.font_size = Some(v);
            }
        }
        "fontWeight" | "font-weight" => {
            let weight = match value.to_lowercase().as_str() {
                "thin" | "100" => 100,
                "extralight" | "extra-light" | "200" => 200,
                "light" | "300" => 300,
                "normal" | "regular" | "400" => 400,
                "medium" | "500" => 500,
                "semibold" | "semi-bold" | "600" => 600,
                "bold" | "700" => 700,
                "extrabold" | "extra-bold" | "800" => 800,
                "black" | "900" => 900,
                _ => value.parse::<u16>().unwrap_or(400),
            };
            style.font_weight = Some(weight);
        }
        "fontStyle" | "font-style" => {
            style.font_style = Some(match value.to_lowercase().as_str() {
                "italic" => FontStyle::Italic,
                "oblique" => FontStyle::Oblique,
                _ => FontStyle::Normal,
            });
        }
        "lineHeight" | "line-height" => {
            if let Ok(v) = value.parse::<f32>() {
                style.line_height = Some(v);
            }
        }
        "letterSpacing" | "letter-spacing" => {
            if let Some(v) = parse_length(value) {
                style.letter_spacing = Some(v);
            }
        }
        "textIndent" | "text-indent" => {
            if let Some(v) = parse_length(value) {
                style.text_indent = Some(v);
            }
        }
        "textTransform" | "text-transform" => {
            style.text_transform = Some(match value.to_lowercase().as_str() {
                "uppercase" => TextTransform::Uppercase,
                "lowercase" => TextTransform::Lowercase,
                "capitalize" => TextTransform::Capitalize,
                _ => TextTransform::None,
            });
        }
        "maxLines" => {
            if let Ok(v) = value.parse::<usize>() {
                style.max_lines = Some(v);
            }
        }
        "textOverflow" | "text-overflow" => {
            style.text_overflow = Some(match value {
                "ellipsis" => TextOverflow::Ellipsis,
                _ => TextOverflow::Clip,
            });
        }
        "color" => {
            if let Some(c) = Color::from_hex(value) {
                style.color = Some(c);
            }
        }
        "textAlign" | "text-align" => {
            style.text_align = Some(match value.to_lowercase().as_str() {
                "left" | "start" => TextAlign::Left,
                "center" => TextAlign::Center,
                "right" | "end" => TextAlign::Right,
                "justify" => TextAlign::Justify,
                _ => TextAlign::Left,
            });
        }
        "textDecoration" | "text-decoration" => {
            style.text_decoration = Some(match value.to_lowercase().as_str() {
                "underline" => TextDecoration::Underline,
                "line-through" | "lineThrough" => TextDecoration::LineThrough,
                _ => TextDecoration::None,
            });
        }
        "textDecorationStyle" | "text-decoration-style" => {
            style.text_decoration_style = Some(match value {
                "dashed" => TextDecorationStyle::Dashed,
                "dotted" => TextDecorationStyle::Dotted,
                _ => TextDecorationStyle::Solid,
            });
        }
        "textDecorationColor" | "text-decoration-color" => {
            if let Some(c) = Color::from_hex(value) {
                style.text_decoration_color = Some(c);
            }
        }

        // Image
        "objectFit" | "object-fit" => {
            style.object_fit = Some(ObjectFit::from_str(value));
        }
        "objectPosition" | "object-position" => {
            style.object_position = parse_object_position(value);
        }

        // Transform
        "transform" => {
            style.transform = parse_transform(value);
        }
        "transformOrigin" | "transform-origin" => {
            style.transform_origin = parse_transform_origin(value);
        }

        _ => {}
    }
}

fn parse_dimension(value: &str) -> Dimension {
    let value = value.trim();
    if value == "auto" {
        Dimension::Auto
    } else if value.ends_with('%') {
        if let Ok(v) = value.trim_end_matches('%').parse::<f32>() {
            Dimension::Percent(v)
        } else {
            Dimension::Auto
        }
    } else if value.ends_with("vw") {
        if let Ok(v) = value.trim_end_matches("vw").parse::<f32>() {
            Dimension::ViewportWidth(v)
        } else {
            Dimension::Auto
        }
    } else if value.ends_with("vh") {
        if let Ok(v) = value.trim_end_matches("vh").parse::<f32>() {
            Dimension::ViewportHeight(v)
        } else {
            Dimension::Auto
        }
    } else {
        if let Some(v) = parse_length(value) {
            Dimension::Points(v)
        } else {
            Dimension::Auto
        }
    }
}

fn parse_length(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some(v) = value.strip_suffix("in") {
        return v.trim().parse::<f32>().ok().map(|n| n * 72.0);
    }
    if let Some(v) = value.strip_suffix("cm") {
        return v.trim().parse::<f32>().ok().map(|n| n * 72.0 / 2.54);
    }
    if let Some(v) = value.strip_suffix("mm") {
        return v.trim().parse::<f32>().ok().map(|n| n * 72.0 / 25.4);
    }
    let value = value
        .trim_end_matches("pt")
        .trim_end_matches("px")
        .trim();
    value.parse::<f32>().ok()
}

fn parse_box_shorthand(value: &str) -> Option<(f32, f32, f32, f32)> {
    let parts: Vec<f32> = value
        .split_whitespace()
        .filter_map(parse_length)
        .collect();

    match parts.as_slice() {
        [v] => Some((*v, *v, *v, *v)),
        [v, h] => Some((*v, *h, *v, *h)),
        [t, h, b] => Some((*t, *h, *b, *h)),
        [t, r, b, l] => Some((*t, *r, *b, *l)),
        _ => None,
    }
}

fn parse_transform(value: &str) -> Vec<TransformOp> {
    let mut ops = Vec::new();
    for part in value.split(')').map(str::trim) {
        if part.is_empty() {
            continue;
        }
        let Some((name, args)) = part.split_once('(') else {
            continue;
        };
        let args: Vec<&str> = args
            .split(|c| c == ',' || c == ' ')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        match name.trim() {
            "rotate" => {
                if let Some(angle) = args.get(0).and_then(|v| parse_angle(v)) {
                    ops.push(TransformOp::Rotate(angle));
                }
            }
            "scale" => {
                let sx = args.get(0).and_then(|v| v.parse::<f32>().ok()).unwrap_or(1.0);
                let sy = args.get(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(sx);
                ops.push(TransformOp::Scale(sx, sy));
            }
            "scaleX" | "scale-x" => {
                if let Some(sx) = args.get(0).and_then(|v| v.parse::<f32>().ok()) {
                    ops.push(TransformOp::Scale(sx, 1.0));
                }
            }
            "scaleY" | "scale-y" => {
                if let Some(sy) = args.get(0).and_then(|v| v.parse::<f32>().ok()) {
                    ops.push(TransformOp::Scale(1.0, sy));
                }
            }
            "translate" => {
                let tx = args.get(0).and_then(|v| parse_length(v)).unwrap_or(0.0);
                let ty = args.get(1).and_then(|v| parse_length(v)).unwrap_or(0.0);
                ops.push(TransformOp::Translate(tx, ty));
            }
            "translateX" | "translate-x" => {
                if let Some(tx) = args.get(0).and_then(|v| parse_length(v)) {
                    ops.push(TransformOp::Translate(tx, 0.0));
                }
            }
            "translateY" | "translate-y" => {
                if let Some(ty) = args.get(0).and_then(|v| parse_length(v)) {
                    ops.push(TransformOp::Translate(0.0, ty));
                }
            }
            "skew" => {
                let ax = args.get(0).and_then(|v| parse_angle(v)).unwrap_or(0.0);
                let ay = args.get(1).and_then(|v| parse_angle(v)).unwrap_or(0.0);
                ops.push(TransformOp::Skew(ax, ay));
            }
            "skewX" | "skew-x" => {
                if let Some(ax) = args.get(0).and_then(|v| parse_angle(v)) {
                    ops.push(TransformOp::Skew(ax, 0.0));
                }
            }
            "skewY" | "skew-y" => {
                if let Some(ay) = args.get(0).and_then(|v| parse_angle(v)) {
                    ops.push(TransformOp::Skew(0.0, ay));
                }
            }
            "matrix" => {
                if args.len() == 6 {
                    let mut values = [0.0f32; 6];
                    let mut ok = true;
                    for (idx, arg) in args.iter().enumerate() {
                        if let Ok(v) = arg.parse::<f32>() {
                            values[idx] = v;
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    if ok {
                        ops.push(TransformOp::Matrix(values));
                    }
                }
            }
            _ => {}
        }
    }
    ops
}

fn parse_angle(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some(v) = value.strip_suffix("deg") {
        return v.trim().parse::<f32>().ok().map(|n| n.to_radians());
    }
    if let Some(v) = value.strip_suffix("rad") {
        return v.trim().parse::<f32>().ok();
    }
    value.parse::<f32>().ok().map(|n| n.to_radians())
}

fn parse_object_position(value: &str) -> Option<ObjectPosition> {
    parse_position_pair(value).map(|(x, y)| ObjectPosition { x, y })
}

fn parse_transform_origin(value: &str) -> Option<TransformOrigin> {
    parse_position_pair(value).map(|(x, y)| TransformOrigin { x, y })
}

fn parse_position_pair(value: &str) -> Option<(f32, f32)> {
    let tokens: Vec<&str> = value
        .split(|c| c == ',' || c == ' ')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.is_empty() {
        return None;
    }

    let mut x = None;
    let mut y = None;

    for token in &tokens {
        let lower = token.to_lowercase();
        let token = lower.as_str();
        let parsed_x = match token {
            "left" => Some(0.0),
            "center" => Some(0.5),
            "right" => Some(1.0),
            _ => parse_percent(token),
        };
        let parsed_y = match token {
            "top" => Some(0.0),
            "center" => Some(0.5),
            "bottom" => Some(1.0),
            _ => parse_percent(token),
        };

        if matches!(token, "left" | "center" | "right") || parsed_x.is_some() {
            if x.is_none() {
                x = parsed_x;
                continue;
            }
        }
        if matches!(token, "top" | "center" | "bottom") || parsed_y.is_some() {
            if y.is_none() {
                y = parsed_y;
            }
        }
    }

    if tokens.len() == 1 {
        let token = tokens[0].to_lowercase();
        if matches!(token.as_str(), "top" | "bottom") {
            x = Some(0.5);
        }
        if matches!(token.as_str(), "left" | "right") {
            y = Some(0.5);
        }
        if token == "center" {
            x = Some(0.5);
            y = Some(0.5);
        }
    }

    Some((x.unwrap_or(0.5), y.unwrap_or(0.5)))
}

fn parse_percent(value: &str) -> Option<f32> {
    if let Some(v) = value.strip_suffix('%') {
        if let Ok(num) = v.trim().parse::<f32>() {
            return Some((num / 100.0).clamp(0.0, 1.0));
        }
    }
    None
}

