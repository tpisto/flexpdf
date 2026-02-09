use std::collections::HashMap;

use crate::fonts::{GlyphSegment, TextSegment};
use crate::standard_fonts;

use super::justify::{item_for_glyph, item_for_segment, JustifyItem};

#[derive(Clone)]
pub(super) enum SegmentKind {
    Text(TextSegment),
    Glyph(GlyphSegment),
}

#[derive(Clone)]
pub(super) struct LineSegment {
    pub(super) span_index: usize,
    pub(super) segment: SegmentKind,
    pub(super) font_size: f32,
}

pub(super) fn segment_kind_to_item(segment: &SegmentKind) -> JustifyItem {
    match segment {
        SegmentKind::Text(seg) => item_for_segment(seg),
        SegmentKind::Glyph(seg) => item_for_glyph(seg),
    }
}

pub(super) fn build_standard_segments(
    text: &str,
    metrics: &standard_fonts::StandardFontMetrics,
) -> Vec<TextSegment> {
    let mut segments: Vec<TextSegment> = Vec::new();
    let mut prev_code: Option<u8> = None;

    for ch in text.chars() {
        let code = standard_fonts::win_ansi_code(ch);
        if let (Some(prev), Some(curr)) = (prev_code, code) {
            if let Some(kern) = metrics.kerning.get(&(prev, curr)) {
                if let Some(prev_seg) = segments.last_mut() {
                    prev_seg.adjust = -(*kern as f32);
                }
            }
        }

        segments.push(TextSegment {
            text: ch.to_string(),
            adjust: 0.0,
        });
        prev_code = code;
    }

    segments
}

pub(super) fn build_kerning_segments(
    text: &str,
    kerning: &HashMap<(u8, u8), f32>,
) -> Vec<TextSegment> {
    let mut segments: Vec<TextSegment> = Vec::new();
    let mut prev_code: Option<u8> = None;

    for ch in text.chars() {
        let code = standard_fonts::win_ansi_code(ch);
        if let (Some(prev), Some(curr)) = (prev_code, code) {
            if let Some(kern) = kerning.get(&(prev, curr)) {
                if let Some(prev_seg) = segments.last_mut() {
                    prev_seg.adjust = -(*kern);
                }
            }
        }

        segments.push(TextSegment {
            text: ch.to_string(),
            adjust: 0.0,
        });
        prev_code = code;
    }

    segments
}
