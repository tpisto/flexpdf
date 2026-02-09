use std::collections::HashMap;

use crate::components::HyphenationLang;
use crate::fonts::{FontKey, FontSystem, TextSegment};
use crate::layout::LayoutRect;
use crate::pdf::ContentStream;
use crate::standard_fonts;
use crate::style::{
    apply_text_transform, Color, FontStyle, Style, TextAlign, TextDecoration,
    TextDecorationStyle, TextOverflow, TextTransform,
};

use super::font::find_best_font;
use super::justify::{
    apply_justification_to_glyphs, apply_justification_to_segments, item_for_glyph,
    item_for_segment, justify_distances,
};
use super::lines::split_lines_from_text;
use super::segments::{
    build_kerning_segments, build_standard_segments, segment_kind_to_item, LineSegment,
    SegmentKind,
};

pub(in crate::render) fn resolve_placeholders(
    text: &str,
    page_number: usize,
    total_pages: usize,
) -> String {
    if !text.contains("{pageNumber}") && !text.contains("{totalPages}") {
        return text.to_string();
    }

    text.replace("{pageNumber}", &page_number.to_string())
        .replace("{totalPages}", &total_pages.to_string())
}

pub(in crate::render) fn render_text(
    content: &mut ContentStream,
    text: &str,
    layout: &LayoutRect,
    font_family: Option<&str>,
    font_size: f32,
    font_weight: u16,
    font_style: Option<FontStyle>,
    line_height: crate::fonts::ResolvedLineHeight,
    color: Color,
    text_align: Option<TextAlign>,
    text_decoration: Option<TextDecoration>,
    text_decoration_style: Option<TextDecorationStyle>,
    text_decoration_color: Option<Color>,
    letter_spacing: Option<f32>,
    text_indent: Option<f32>,
    max_lines: Option<usize>,
    text_overflow: Option<TextOverflow>,
    text_transform: Option<TextTransform>,
    page_height: f32,
    font_system: &FontSystem,
    font_map: &HashMap<FontKey, String>,
    page_number: usize,
    total_pages: usize,
    hyphenation: Option<HyphenationLang>,
) {
    let resolved = resolve_placeholders(text, page_number, total_pages);
    let transformed = apply_text_transform(&resolved, text_transform);
    if transformed.is_empty() {
        return;
    }

    let text_layout = font_system.layout_text(
        &transformed,
        font_family,
        font_size,
        Some(font_weight),
        font_style,
        line_height,
        hyphenation,
        layout.width,
        text_align,
        letter_spacing,
        text_indent,
        max_lines,
        text_overflow,
    );
    let lines = text_layout.lines();

    if lines.is_empty() {
        return;
    }

    let letter_spacing = letter_spacing.unwrap_or(0.0);
    let use_char_spacing = letter_spacing.abs() > f32::EPSILON
        && lines
            .iter()
            .all(|line| line.glyphs.is_none() && line.segments.is_none());

    let font_key = font_system.resolve_font_key(font_family, Some(font_weight), font_style);
    let standard_variant = standard_fonts::resolve_standard_variant(
        font_key.family.as_str(),
        font_key.weight,
        font_key.is_italic,
    );
    let kerning = font_system.resolve_kerning(font_family, Some(font_weight), font_style);
    let pdf_font_name = find_best_font(font_map, font_system, font_family, font_weight, font_style);

    content.begin_text();
    content.set_fill_color(color.r, color.g, color.b);
    content.set_font(&pdf_font_name, font_size);
    if use_char_spacing {
        content.set_char_spacing(letter_spacing / font_size);
    }

    // Position each line individually to support alignment
    // Each line has its own X offset from Parley based on alignment
    let mut line_positions: Vec<(f32, f32, f32)> = Vec::new();
    let justify = matches!(text_align, Some(TextAlign::Justify));
    for (idx, line) in lines.iter().enumerate() {
        let is_last_line = idx + 1 == lines.len();
        let should_justify = justify && !is_last_line;
        let should_shrink = line.width > layout.width;
        let apply_justification = (should_justify || should_shrink) && !line.text.is_empty();
        let gap = layout.width - line.width;
        content.set_word_spacing(0.0);
        let line_x = layout.x + line.x;
        let line_y = page_height - layout.y - line.y;
        content.text_position(line_x, line_y);
        if let Some(ref glyphs) = line.glyphs {
            let glyphs = if apply_justification {
                let items = glyphs.iter().map(item_for_glyph).collect::<Vec<_>>();
                let distances = justify_distances(&items, gap);
                apply_justification_to_glyphs(glyphs, &distances, font_size)
            } else {
                glyphs.clone()
            };
            content.show_cid_glyphs_with_segments(&glyphs);
        } else if let Some(ref segments) = line.segments {
            let segments = if apply_justification {
                let items = segments.iter().map(item_for_segment).collect::<Vec<_>>();
                let distances = justify_distances(&items, gap);
                apply_justification_to_segments(segments, &distances, font_size)
            } else {
                segments.clone()
            };
            content.show_text_with_segments(&segments);
        } else if let Some(variant) = standard_variant {
            if let Some(metrics) = standard_fonts::metrics_for(variant.name) {
                let mut segments = build_standard_segments(&line.text, metrics);
                if apply_justification {
                    let items = segments.iter().map(item_for_segment).collect::<Vec<_>>();
                    let distances = justify_distances(&items, gap);
                    segments = apply_justification_to_segments(&segments, &distances, font_size);
                }
                content.show_text_with_segments(&segments);
            } else {
                content.show_text(&line.text);
            }
        } else if let Some(ref kerning) = kerning {
            let mut segments = build_kerning_segments(&line.text, kerning);
            if apply_justification {
                let items = segments.iter().map(item_for_segment).collect::<Vec<_>>();
                let distances = justify_distances(&items, gap);
                segments = apply_justification_to_segments(&segments, &distances, font_size);
            }
            content.show_text_with_segments(&segments);
        } else {
            content.show_text(&line.text);
        }
        line_positions.push((line_x, line_y, line.width));
    }

    if use_char_spacing {
        content.set_char_spacing(0.0);
    }
    content.end_text();

    if let Some(decoration) = text_decoration {
        match decoration {
            TextDecoration::Underline | TextDecoration::LineThrough => {
                let stroke_width = (font_size * 0.05).max(0.5);
                let y_offset = match decoration {
                    TextDecoration::Underline => -font_size * 0.1,
                    TextDecoration::LineThrough => font_size * 0.3,
                    TextDecoration::None => 0.0,
                };
                let decoration_color = text_decoration_color.unwrap_or(color);
                let decoration_style = text_decoration_style.unwrap_or(TextDecorationStyle::Solid);

                content.save();
                content.set_stroke_color(decoration_color.r, decoration_color.g, decoration_color.b);
                content.set_line_width(stroke_width);
                match decoration_style {
                    TextDecorationStyle::Solid => {
                        content.set_line_dash(&[], 0.0);
                        content.set_line_cap(0);
                    }
                    TextDecorationStyle::Dashed => {
                        let dash = (stroke_width * 3.0).max(1.0);
                        let gap = (stroke_width * 2.0).max(1.0);
                        content.set_line_dash(&[dash, gap], 0.0);
                        content.set_line_cap(0);
                    }
                    TextDecorationStyle::Dotted => {
                        let dot = stroke_width.max(1.0);
                        let gap = (stroke_width * 1.5).max(1.0);
                        content.set_line_dash(&[dot, gap], 0.0);
                        content.set_line_cap(1);
                    }
                }

                for (line_x, line_y, line_width) in line_positions {
                    let y = line_y + y_offset;
                    content.move_to(line_x, y);
                    content.line_to(line_x + line_width, y);
                    content.stroke();
                }

                content.restore();
            }
            TextDecoration::None => {}
        }
    }
}

/// Render text with inline spans (mixed styles).
pub(in crate::render) fn render_text_with_spans(
    content: &mut ContentStream,
    spans: &[crate::components::TextSpan],
    parent_style: &Style,
    layout: &LayoutRect,
    page_height: f32,
    font_system: &FontSystem,
    font_map: &HashMap<FontKey, String>,
    page_number: usize,
    total_pages: usize,
    hyphenation: Option<HyphenationLang>,
) {
    if spans.is_empty() {
        return;
    }

    #[derive(Clone)]
    struct RenderSpan {
        content: String,
        start: usize,
        end: usize,
        font_size: f32,
        font_weight: u16,
        font_style: Option<FontStyle>,
        font_family: Option<String>,
        color: Color,
        letter_spacing: f32,
    }

    // Get parent style defaults
    let parent_font_size = parent_style.font_size.unwrap_or(12.0);
    let parent_font_weight = parent_style.font_weight.unwrap_or(400);
    let parent_font_style = parent_style.font_style;
    let parent_color = parent_style.color.unwrap_or(Color::black());
    let parent_font_family = parent_style.font_family.as_deref();
    let text_align = parent_style.text_align;
    let parent_letter_spacing = parent_style.letter_spacing.unwrap_or(0.0);
    let text_indent = parent_style.text_indent;
    let text_transform = parent_style.text_transform;
    let max_lines = parent_style.max_lines;
    let text_overflow = parent_style.text_overflow;
    let line_height = font_system.resolve_line_height(
        parent_style.line_height,
        parent_font_family,
        Some(parent_font_weight),
        parent_font_style,
    );

    let mut full_text = String::new();
    let mut render_spans: Vec<RenderSpan> = Vec::new();

    for span in spans {
        let resolved = resolve_placeholders(&span.content, page_number, total_pages);
        let span_transform = span.style.text_transform.or(text_transform);
        let resolved = apply_text_transform(&resolved, span_transform);
        if resolved.is_empty() {
            continue;
        }

        let start = full_text.len();
        full_text.push_str(&resolved);
        let end = full_text.len();

        render_spans.push(RenderSpan {
            content: resolved,
            start,
            end,
            font_size: span.style.font_size.unwrap_or(parent_font_size),
            font_weight: span.style.font_weight.unwrap_or(parent_font_weight),
            font_style: span.style.font_style.or(parent_font_style),
            font_family: span
                .style
                .font_family
                .clone()
                .or_else(|| parent_style.font_family.clone()),
            color: span.style.color.unwrap_or(parent_color),
            letter_spacing: span.style.letter_spacing.unwrap_or(parent_letter_spacing),
        });
    }

    if full_text.is_empty() {
        return;
    }

    let text_layout = font_system.layout_text(
        &full_text,
        parent_font_family,
        parent_font_size,
        Some(parent_font_weight),
        parent_font_style,
        line_height,
        hyphenation,
        layout.width,
        text_align,
        Some(parent_letter_spacing),
        text_indent,
        max_lines,
        text_overflow,
    );
    let lines = text_layout.lines();
    if lines.is_empty() {
        return;
    }

    let line_texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
    let line_ranges = split_lines_from_text(&full_text, &line_texts);

    content.begin_text();
    for (line_index, line) in lines.iter().enumerate() {
        let line_text = line.text.as_str();
        if line_text.is_empty() {
            continue;
        }

        let Some((line_start, line_end)) =
            line_ranges.get(line_index).and_then(|range| *range)
        else {
            continue;
        };

        let is_last_line = line_index + 1 == lines.len();
        let should_justify = matches!(text_align, Some(TextAlign::Justify)) && !is_last_line;
        let should_shrink = line.width > layout.width;
        let apply_justification = should_justify || should_shrink;
        let gap = layout.width - line.width;
        content.set_word_spacing(0.0);

        let line_x = layout.x + line.x;
        let line_y = page_height - layout.y - line.y;
        content.text_position(line_x, line_y);

        let mut line_segments: Vec<LineSegment> = Vec::new();

        for (span_index, span) in render_spans.iter().enumerate() {
            if span.end <= line_start || span.start >= line_end {
                continue;
            }

            let sub_start = line_start.max(span.start);
            let sub_end = line_end.min(span.end);
            if sub_start >= sub_end {
                continue;
            }

            let rel_start = sub_start - span.start;
            let rel_end = sub_end - span.start;
            let segment_text = &span.content[rel_start..rel_end];
            if segment_text.is_empty() {
                continue;
            }

            let font_family = span.font_family.as_deref();
            let font_key =
                font_system.resolve_font_key(font_family, Some(span.font_weight), span.font_style);
            let standard_variant = standard_fonts::resolve_standard_variant(
                font_key.family.as_str(),
                font_key.weight,
                font_key.is_italic,
            );
            let kerning =
                font_system.resolve_kerning(font_family, Some(span.font_weight), span.font_style);
            let inline_glyphs = font_system.layout_inline_glyphs(
                segment_text,
                font_family,
                span.font_size,
                Some(span.font_weight),
                span.font_style,
                line_height,
                Some(span.letter_spacing),
            );
            let inline_segments = font_system.layout_inline_segments(
                segment_text,
                font_family,
                span.font_size,
                Some(span.font_weight),
                span.font_style,
                line_height,
                Some(span.letter_spacing),
            );

            let segments: Vec<SegmentKind> = if let Some(glyphs) = inline_glyphs {
                glyphs.into_iter().map(SegmentKind::Glyph).collect()
            } else if let Some(segments) = inline_segments {
                segments.into_iter().map(SegmentKind::Text).collect()
            } else if let Some(variant) = standard_variant {
                if let Some(metrics) = standard_fonts::metrics_for(variant.name) {
                    build_standard_segments(segment_text, metrics)
                        .into_iter()
                        .map(SegmentKind::Text)
                        .collect()
                } else {
                    vec![SegmentKind::Text(TextSegment {
                        text: segment_text.to_string(),
                        adjust: 0.0,
                    })]
                }
            } else if let Some(ref kerning) = kerning {
                build_kerning_segments(segment_text, kerning)
                    .into_iter()
                    .map(SegmentKind::Text)
                    .collect()
            } else {
                vec![SegmentKind::Text(TextSegment {
                    text: segment_text.to_string(),
                    adjust: 0.0,
                })]
            };

            if segments.is_empty() {
                continue;
            }

            for segment in segments {
                line_segments.push(LineSegment {
                    span_index,
                    segment,
                    font_size: span.font_size,
                });
            }
        }

        if line_segments.is_empty() {
            continue;
        }

        if apply_justification {
            let items = line_segments
                .iter()
                .map(|seg| segment_kind_to_item(&seg.segment))
                .collect::<Vec<_>>();
            let distances = justify_distances(&items, gap);
            if distances.len() == line_segments.len() {
                for (idx, distance) in distances.iter().enumerate() {
                    let scale = 1000.0 / line_segments[idx].font_size;
                    match &mut line_segments[idx].segment {
                        SegmentKind::Text(segment) => {
                            segment.adjust -= distance * scale;
                        }
                        SegmentKind::Glyph(segment) => {
                            segment.adjust -= distance * scale;
                        }
                    }
                }
            }
        }

        let mut idx = 0;
        while idx < line_segments.len() {
            let span_index = line_segments[idx].span_index;
            let span = &render_spans[span_index];
            let mut segments = Vec::new();
            while idx < line_segments.len() && line_segments[idx].span_index == span_index {
                segments.push(line_segments[idx].segment.clone());
                idx += 1;
            }

            if segments.is_empty() {
                continue;
            }

            let font_family = span.font_family.as_deref();
            let pdf_font_name = find_best_font(
                font_map,
                font_system,
                font_family,
                span.font_weight,
                span.font_style,
            );
            content.set_fill_color(span.color.r, span.color.g, span.color.b);
            content.set_font(&pdf_font_name, span.font_size);
            let span_char_spacing = if span.letter_spacing.abs() > f32::EPSILON {
                span.letter_spacing / span.font_size
            } else {
                0.0
            };
            let mut current_char_spacing: Option<f32> = None;
            let mut seg_index = 0;
            while seg_index < segments.len() {
                match &segments[seg_index] {
                    SegmentKind::Glyph(_) => {
                        if current_char_spacing != Some(0.0) {
                            content.set_char_spacing(0.0);
                            current_char_spacing = Some(0.0);
                        }
                        let mut glyphs = Vec::new();
                        while seg_index < segments.len() {
                            match segments[seg_index].clone() {
                                SegmentKind::Glyph(segment) => {
                                    glyphs.push(segment);
                                    seg_index += 1;
                                }
                                SegmentKind::Text(_) => break,
                            }
                        }
                        if !glyphs.is_empty() {
                            content.show_cid_glyphs_with_segments(&glyphs);
                        }
                    }
                    SegmentKind::Text(_) => {
                        if current_char_spacing != Some(span_char_spacing) {
                            content.set_char_spacing(span_char_spacing);
                            current_char_spacing = Some(span_char_spacing);
                        }
                        let mut text_segments = Vec::new();
                        while seg_index < segments.len() {
                            match segments[seg_index].clone() {
                                SegmentKind::Text(segment) => {
                                    text_segments.push(segment);
                                    seg_index += 1;
                                }
                                SegmentKind::Glyph(_) => break,
                            }
                        }
                        if !text_segments.is_empty() {
                            content.show_text_with_segments(&text_segments);
                        }
                    }
                }
            }
        }
    }

    content.set_char_spacing(0.0);
    content.end_text();
}
