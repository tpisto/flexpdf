use unicode_normalization::char::is_combining_mark;

use crate::fonts::{GlyphSegment, TextSegment};

#[derive(Clone, Copy)]
pub(super) struct JustifyItem {
    pub(super) is_whitespace: bool,
    pub(super) is_mark: bool,
}

#[derive(Clone, Copy)]
struct JustifyFactor {
    before: f32,
    after: f32,
    priority: usize,
    unconstrained: bool,
}

const WHITESPACE_PRIORITY: usize = 1;
const LETTER_PRIORITY: usize = 2;
const KASHIDA_PRIORITY: usize = 0;
const NULL_PRIORITY: usize = 3;

const EXPAND_WHITESPACE_FACTOR: JustifyFactor = JustifyFactor {
    before: 0.5,
    after: 0.5,
    priority: WHITESPACE_PRIORITY,
    unconstrained: false,
};
const EXPAND_CHAR_FACTOR: JustifyFactor = JustifyFactor {
    before: 0.14453125,
    after: 0.14453125,
    priority: LETTER_PRIORITY,
    unconstrained: false,
};
const SHRINK_WHITESPACE_FACTOR: JustifyFactor = JustifyFactor {
    before: -0.5,
    after: -0.5,
    priority: WHITESPACE_PRIORITY,
    unconstrained: false,
};
const SHRINK_CHAR_FACTOR: JustifyFactor = JustifyFactor {
    before: -0.04296875,
    after: -0.04296875,
    priority: LETTER_PRIORITY,
    unconstrained: false,
};

pub(super) fn item_for_segment(segment: &TextSegment) -> JustifyItem {
    JustifyItem {
        is_whitespace: segment_is_whitespace(segment),
        is_mark: segment_is_mark(segment),
    }
}

pub(super) fn item_for_glyph(segment: &GlyphSegment) -> JustifyItem {
    JustifyItem {
        is_whitespace: segment.is_whitespace,
        is_mark: segment.is_mark,
    }
}

pub(super) fn justify_distances(items: &[JustifyItem], gap: f32) -> Vec<f32> {
    if items.is_empty() {
        return Vec::new();
    }

    let (whitespace_factor, char_factor) = if gap > 0.0 {
        (EXPAND_WHITESPACE_FACTOR, EXPAND_CHAR_FACTOR)
    } else {
        (SHRINK_WHITESPACE_FACTOR, SHRINK_CHAR_FACTOR)
    };

    let mut factors: Vec<JustifyFactor> = Vec::with_capacity(items.len());
    for (idx, item) in items.iter().enumerate() {
        let mut factor = if item.is_whitespace {
            whitespace_factor
        } else if item.is_mark && idx > 0 {
            let mut inherited = factors[idx - 1];
            inherited.before = 0.0;
            factors[idx - 1].after = 0.0;
            inherited
        } else {
            char_factor
        };

        if item.is_whitespace && idx + 1 == items.len() {
            factor.before = 0.0;
            if let Some(prev) = factors.last_mut() {
                prev.after = 0.0;
            }
        }

        factors.push(factor);
    }

    if let Some(first) = factors.first_mut() {
        first.before = 0.0;
    }
    if let Some(last) = factors.last_mut() {
        last.after = 0.0;
    }

    let mut total = 0.0;
    let mut priorities = [0.0; NULL_PRIORITY + 1];
    let mut unconstrained = [0.0; NULL_PRIORITY + 1];

    for factor in &factors {
        let sum = factor.before + factor.after;
        total += sum;
        priorities[factor.priority] += sum;
        if factor.unconstrained {
            unconstrained[factor.priority] += sum;
        }
    }

    let mut highest_priority: Option<usize> = None;
    let mut highest_priority_sum = 0.0;
    let mut remaining_gap = gap;
    let mut stop_priority = NULL_PRIORITY + 1;

    for priority in KASHIDA_PRIORITY..=NULL_PRIORITY {
        let priority_sum = priorities[priority];
        if priority_sum != 0.0 {
            if highest_priority.is_none() {
                highest_priority = Some(priority);
                highest_priority_sum = priority_sum;
            }

            if remaining_gap.abs() <= priority_sum.abs() {
                priorities[priority] = remaining_gap / priority_sum;
                unconstrained[priority] = 0.0;
                remaining_gap = 0.0;
                stop_priority = priority;
                break;
            }

            priorities[priority] = 1.0;
            remaining_gap -= priority_sum;

            if unconstrained[priority] != 0.0 {
                unconstrained[priority] = remaining_gap / unconstrained[priority];
                remaining_gap = 0.0;
                stop_priority = priority;
                break;
            }
        }
    }

    if stop_priority <= NULL_PRIORITY {
        for p in (stop_priority + 1)..=NULL_PRIORITY {
            priorities[p] = 0.0;
            unconstrained[p] = 0.0;
        }
    }

    if remaining_gap > 0.0 {
        if let Some(priority) = highest_priority {
            priorities[priority] =
                (highest_priority_sum + (gap - total)) / highest_priority_sum;
        }
    }

    let mut distances = Vec::with_capacity(factors.len());
    for index in 0..factors.len() {
        let factor = factors[index];
        let mut dist = factor.after * priorities[factor.priority];
        if let Some(next) = factors.get(index + 1) {
            dist += next.before * priorities[next.priority];
        }

        if factor.unconstrained {
            dist += factor.after * unconstrained[factor.priority];
            if let Some(next) = factors.get(index + 1) {
                dist += next.before * unconstrained[next.priority];
            }
        }

        distances.push(dist);
    }

    distances
}

pub(super) fn apply_justification_to_segments(
    segments: &[TextSegment],
    distances: &[f32],
    font_size: f32,
) -> Vec<TextSegment> {
    if segments.len() != distances.len() {
        return segments.to_vec();
    }

    let scale = 1000.0 / font_size;
    segments
        .iter()
        .zip(distances.iter())
        .map(|(segment, distance)| TextSegment {
            text: segment.text.clone(),
            adjust: segment.adjust - distance * scale,
        })
        .collect()
}

pub(super) fn apply_justification_to_glyphs(
    segments: &[GlyphSegment],
    distances: &[f32],
    font_size: f32,
) -> Vec<GlyphSegment> {
    if segments.len() != distances.len() {
        return segments.to_vec();
    }

    let scale = 1000.0 / font_size;
    segments
        .iter()
        .zip(distances.iter())
        .map(|(segment, distance)| GlyphSegment {
            glyph_id: segment.glyph_id,
            adjust: segment.adjust - distance * scale,
            is_whitespace: segment.is_whitespace,
            is_mark: segment.is_mark,
        })
        .collect()
}

fn segment_is_whitespace(segment: &TextSegment) -> bool {
    segment.text.chars().all(|ch| ch.is_whitespace())
}

fn segment_is_mark(segment: &TextSegment) -> bool {
    if segment.text.is_empty() {
        return false;
    }

    segment.text.chars().all(is_combining_mark)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_justification_to_glyphs, apply_justification_to_segments, justify_distances,
        JustifyItem,
    };
    use crate::fonts::{GlyphSegment, TextSegment};

    #[test]
    fn justify_distances_expands_overflow_gap() {
        let items = vec![
            JustifyItem {
                is_whitespace: false,
                is_mark: false,
            },
            JustifyItem {
                is_whitespace: true,
                is_mark: false,
            },
            JustifyItem {
                is_whitespace: false,
                is_mark: false,
            },
        ];
        let distances = justify_distances(&items, 10.0);
        assert_eq!(distances.len(), 3);
        assert!((distances[0] - 5.0).abs() < 0.0001);
        assert!((distances[1] - 5.0).abs() < 0.0001);
        assert!((distances[2] - 0.0).abs() < 0.0001);
    }

    #[test]
    fn apply_justification_scales_adjustments() {
        let segments = vec![TextSegment {
            text: "A".to_string(),
            adjust: 0.0,
        }];
        let distances = vec![5.0];
        let adjusted = apply_justification_to_segments(&segments, &distances, 10.0);
        assert_eq!(adjusted.len(), 1);
        assert!((adjusted[0].adjust + 500.0).abs() < 0.0001);
    }

    #[test]
    fn apply_justification_scales_glyph_adjustments() {
        let segments = vec![GlyphSegment {
            glyph_id: 10,
            adjust: 0.0,
            is_whitespace: true,
            is_mark: false,
        }];
        let distances = vec![5.0];
        let adjusted = apply_justification_to_glyphs(&segments, &distances, 10.0);
        assert_eq!(adjusted.len(), 1);
        assert!((adjusted[0].adjust + 500.0).abs() < 0.0001);
    }
}
