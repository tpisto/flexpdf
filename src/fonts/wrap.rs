//! Text wrapping and line breaking utilities used by the font system.

use super::{hyphenator_for, ELLIPSIS_STR};
use crate::components::HyphenationLang;
use crate::standard_fonts;
use crate::style::TextOverflow;
use hyphenation::{Hyphenator, Standard};

pub(super) fn wrap_paragraph(
    paragraph: &str,
    metrics: &standard_fonts::StandardFontMetrics,
    font_size: f32,
    wrap_width: Option<f32>,
    hyphenation: Option<HyphenationLang>,
    letter_spacing: f32,
) -> Vec<String> {
    if paragraph.is_empty() {
        return vec![String::new()];
    }

    if wrap_width.is_none() {
        return vec![paragraph.to_string()];
    }

    let wrap_width_units = wrap_width.unwrap() * 1000.0 / font_size;
    let letter_spacing_units = if letter_spacing != 0.0 && font_size > 0.0 {
        letter_spacing * 1000.0 / font_size
    } else {
        0.0
    };
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_units = 0.0;
    let mut current_last: Option<u8> = None;
    let hyphenator = hyphenation.and_then(hyphenator_for);

    for word in paragraph.split(' ') {
        if word.is_empty() {
            continue;
        }

        let try_hyphenate = |base_units: f32, base_last: Option<u8>| -> Option<(String, String)> {
            let hyphenator = hyphenator?;
            if !word.chars().all(|ch| ch.is_alphabetic() || ch == '\u{00ad}') {
                return None;
            }
            let breaks = &hyphenator.hyphenate(word).breaks;
            if breaks.is_empty() {
                return None;
            }
            for &break_idx in breaks.iter().rev() {
                if break_idx == 0 || break_idx >= word.len() {
                    continue;
                }
                if !word.is_char_boundary(break_idx) {
                    continue;
                }
                let prefix = &word[..break_idx];
                let suffix = &word[break_idx..];
                if prefix.is_empty() || suffix.is_empty() {
                    continue;
                }
            let (prefix_units, prefix_last) = append_width_units(
                base_units,
                base_last,
                prefix,
                metrics,
                letter_spacing_units,
            );
            let (prefix_units, _last) =
                    append_width_units(prefix_units, prefix_last, "-", metrics, letter_spacing_units);
            if prefix_units <= wrap_width_units + 0.5 {
                return Some((prefix.to_string(), suffix.to_string()));
            }
        }
        None
        };

        if current.is_empty() {
            let (units, last) =
                append_width_units(0.0, None, word, metrics, letter_spacing_units);
            if units <= wrap_width_units + 0.5 {
                current.push_str(word);
                current_units = units;
                current_last = last;
            } else if let Some((prefix, suffix)) = try_hyphenate(0.0, None) {
                current.push_str(&prefix);
                current.push('-');
                lines.push(current);
                current = suffix;
                let (units, last) =
                    append_width_units(0.0, None, &current, metrics, letter_spacing_units);
                current_units = units;
                current_last = last;
            } else {
                current.push_str(word);
                current_units = units;
                current_last = last;
            }
            continue;
        }

        let (space_units, space_last) = append_width_units(
            current_units,
            current_last,
            " ",
            metrics,
            letter_spacing_units,
        );
        let (candidate_units, candidate_last) =
            append_width_units(space_units, space_last, word, metrics, letter_spacing_units);

        if candidate_units <= wrap_width_units + 0.5 || current_units == 0.0 {
            current.push(' ');
            current.push_str(word);
            current_units = candidate_units;
            current_last = candidate_last;
        } else if let Some((prefix, suffix)) = try_hyphenate(space_units, space_last) {
            current.push(' ');
            current.push_str(&prefix);
            current.push('-');
            lines.push(current);
            current = suffix;
            let (units, last) =
                append_width_units(0.0, None, &current, metrics, letter_spacing_units);
            current_units = units;
            current_last = last;
        } else {
            lines.push(current);
            current = word.to_string();
            let (units, last) =
                append_width_units(0.0, None, word, metrics, letter_spacing_units);
            current_units = units;
            current_last = last;
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

pub(super) fn truncate_lines_with_ellipsis(
    mut lines: Vec<String>,
    max_lines: Option<usize>,
    text_overflow: Option<TextOverflow>,
    max_width: Option<f32>,
    measure_line: &dyn Fn(&str) -> f32,
) -> Vec<String> {
    let Some(max_lines) = max_lines else {
        return lines;
    };
    if max_lines == 0 || lines.len() <= max_lines {
        return lines;
    }

    lines.truncate(max_lines);

    if !matches!(text_overflow, Some(TextOverflow::Ellipsis)) {
        return lines;
    }

    let Some(max_width) = max_width else {
        return lines;
    };
    if lines.is_empty() {
        return lines;
    }

    let ellipsis_width = measure_line(ELLIPSIS_STR);
    if ellipsis_width > max_width {
        lines[max_lines - 1].clear();
        return lines;
    }

    let mut base = lines[max_lines - 1].trim_end().to_string();
    if base.is_empty() {
        lines[max_lines - 1] = ELLIPSIS_STR.to_string();
        return lines;
    }

    loop {
        let candidate = format!("{}{}", base, ELLIPSIS_STR);
        if measure_line(&candidate) <= max_width || base.is_empty() {
            lines[max_lines - 1] = if base.is_empty() {
                ELLIPSIS_STR.to_string()
            } else {
                candidate
            };
            break;
        }
        base.pop();
    }

    lines
}

#[derive(Clone, Debug)]
struct TokenRange {
    text: String,
    start: usize,
    end: usize,
    is_space: bool,
}

fn split_tokens_with_ranges(paragraph: &str) -> Vec<TokenRange> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_start = 0usize;
    let mut in_space: Option<bool> = None;

    for (byte_idx, ch) in paragraph.char_indices() {
        let is_space = ch.is_whitespace();
        match in_space {
            None => {
                in_space = Some(is_space);
                current_start = byte_idx;
                current.push(ch);
            }
            Some(flag) if flag == is_space => {
                current.push(ch);
            }
            Some(_) => {
                tokens.push(TokenRange {
                    text: current,
                    start: current_start,
                    end: byte_idx,
                    is_space: in_space.unwrap_or(false),
                });
                current = String::new();
                current_start = byte_idx;
                current.push(ch);
                in_space = Some(is_space);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(TokenRange {
            text: current,
            start: current_start,
            end: paragraph.len(),
            is_space: in_space.unwrap_or(false),
        });
    }

    tokens
}

fn advance_width_between(advances: &[f32], start: usize, end: usize) -> f32 {
    if advances.is_empty() || start >= end {
        return 0.0;
    }
    let end = end.min(advances.len().saturating_sub(1));
    let start = start.min(end);
    advances[end] - advances[start]
}

fn is_hyphenation_candidate(word: &str) -> bool {
    word.chars().any(|ch| ch.is_alphabetic() || ch == '\u{00ad}')
}

fn hyphenate_word_segments(word: &str, hyphenator: &Standard) -> Option<Vec<String>> {
    if !is_hyphenation_candidate(word) {
        return None;
    }
    let breaks = &hyphenator.hyphenate(word).breaks;
    if breaks.is_empty() {
        return None;
    }
    let mut segments = Vec::with_capacity(breaks.len() + 1);
    let mut start = 0usize;
    for &break_idx in breaks {
        if break_idx == 0 || break_idx >= word.len() {
            continue;
        }
        if !word.is_char_boundary(break_idx) {
            continue;
        }
        if start >= break_idx {
            continue;
        }
        segments.push(word[start..break_idx].to_string());
        start = break_idx;
    }
    if start < word.len() {
        segments.push(word[start..].to_string());
    }
    if segments.len() > 1 {
        Some(segments)
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinebreakNodeKind {
    Box,
    Glue,
    Penalty,
}

#[derive(Clone, Copy, Debug)]
struct LinebreakNode {
    kind: LinebreakNodeKind,
    width: f32,
    stretch: f32,
    shrink: f32,
    penalty: f32,
    flagged: f32,
    token_end: usize,
    insert_hyphen: bool,
}

impl LinebreakNode {
    fn r#box(width: f32, token_end: usize) -> Self {
        Self {
            kind: LinebreakNodeKind::Box,
            width,
            stretch: 0.0,
            shrink: 0.0,
            penalty: 0.0,
            flagged: 0.0,
            token_end,
            insert_hyphen: false,
        }
    }

    fn glue(width: f32, stretch: f32, shrink: f32, token_end: usize) -> Self {
        Self {
            kind: LinebreakNodeKind::Glue,
            width,
            stretch,
            shrink,
            penalty: 0.0,
            flagged: 0.0,
            token_end,
            insert_hyphen: false,
        }
    }

    fn penalty(width: f32, penalty: f32, flagged: f32, token_end: usize, insert_hyphen: bool) -> Self {
        Self {
            kind: LinebreakNodeKind::Penalty,
            width,
            stretch: 0.0,
            shrink: 0.0,
            penalty,
            flagged,
            token_end,
            insert_hyphen,
        }
    }
}

const LINEBREAK_INFINITY: f32 = 10000.0;

#[derive(Clone, Copy, Debug, Default)]
struct LinebreakSum {
    width: f32,
    stretch: f32,
    shrink: f32,
}

#[derive(Clone, Debug)]
struct LinebreakBreakpoint {
    position: usize,
    demerits: f32,
    line: usize,
    fitness_class: usize,
    totals: LinebreakSum,
    previous: Option<usize>,
}

#[derive(Clone, Debug)]
struct LinebreakActiveNode {
    data: LinebreakBreakpoint,
    prev: Option<usize>,
    next: Option<usize>,
}

struct LinebreakActiveList {
    nodes: Vec<LinebreakActiveNode>,
    head: Option<usize>,
    tail: Option<usize>,
}

impl LinebreakActiveList {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            head: None,
            tail: None,
        }
    }

    fn first(&self) -> Option<usize> {
        self.head
    }

    fn push(&mut self, data: LinebreakBreakpoint) -> usize {
        let index = self.nodes.len();
        let node = LinebreakActiveNode {
            data,
            prev: self.tail,
            next: None,
        };
        if let Some(tail) = self.tail {
            self.nodes[tail].next = Some(index);
        } else {
            self.head = Some(index);
        }
        self.tail = Some(index);
        self.nodes.push(node);
        index
    }

    fn insert_before(&mut self, before: usize, data: LinebreakBreakpoint) -> usize {
        let index = self.nodes.len();
        let prev = self.nodes[before].prev;
        let node = LinebreakActiveNode {
            data,
            prev,
            next: Some(before),
        };
        if let Some(prev_idx) = prev {
            self.nodes[prev_idx].next = Some(index);
        } else {
            self.head = Some(index);
        }
        self.nodes[before].prev = Some(index);
        self.nodes.push(node);
        index
    }

    fn remove(&mut self, index: usize) {
        let prev = self.nodes[index].prev;
        let next = self.nodes[index].next;
        if let Some(prev_idx) = prev {
            self.nodes[prev_idx].next = next;
        } else {
            self.head = next;
        }
        if let Some(next_idx) = next {
            self.nodes[next_idx].prev = prev;
        } else {
            self.tail = prev;
        }
        self.nodes[index].prev = None;
        self.nodes[index].next = None;
    }
}

#[derive(Clone, Copy, Debug)]
struct LinebreakCandidate {
    active: Option<usize>,
    demerits: f32,
}

impl LinebreakCandidate {
    fn new() -> Self {
        Self {
            active: None,
            demerits: f32::INFINITY,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct LinebreakOptions {
    demerits_line: f32,
    demerits_flagged: f32,
    demerits_fitness: f32,
    tolerance: f32,
}

fn linebreak_compute_cost(
    nodes: &[LinebreakNode],
    line_lengths: &[f32],
    sum: LinebreakSum,
    end: usize,
    active: &LinebreakBreakpoint,
    current_line: usize,
) -> f32 {
    let mut width = sum.width - active.totals.width;
    let line_length = if current_line <= line_lengths.len() {
        line_lengths[current_line - 1]
    } else {
        *line_lengths.last().unwrap_or(&0.0)
    };

    if nodes[end].kind == LinebreakNodeKind::Penalty {
        width += nodes[end].width;
    }

    if width < line_length {
        let stretch = sum.stretch - active.totals.stretch;
        if stretch > 0.0 {
            return (line_length - width) / stretch;
        }
        return LINEBREAK_INFINITY;
    }

    if width > line_length {
        let shrink = sum.shrink - active.totals.shrink;
        if shrink > 0.0 {
            return (line_length - width) / shrink;
        }
        return LINEBREAK_INFINITY;
    }

    0.0
}

fn linebreak_compute_sum(nodes: &[LinebreakNode], sum: LinebreakSum, break_index: usize) -> LinebreakSum {
    let mut result = sum;
    for (i, node) in nodes.iter().enumerate().skip(break_index) {
        match node.kind {
            LinebreakNodeKind::Glue => {
                result.width += node.width;
                result.stretch += node.stretch;
                result.shrink += node.shrink;
            }
            LinebreakNodeKind::Box => {
                break;
            }
            LinebreakNodeKind::Penalty => {
                if node.penalty == -LINEBREAK_INFINITY && i > break_index {
                    break;
                }
            }
        }
    }
    result
}

fn linebreak_main_loop(
    node: &LinebreakNode,
    index: usize,
    nodes: &[LinebreakNode],
    active_nodes: &mut LinebreakActiveList,
    sum: LinebreakSum,
    line_lengths: &[f32],
    options: LinebreakOptions,
) {
    let mut active = active_nodes.first();

    while let Some(active_idx) = active {
        let mut candidates = [LinebreakCandidate::new(); 4];

        let mut inner = Some(active_idx);
        while let Some(inner_idx) = inner {
            let active_data = active_nodes.nodes[inner_idx].data.clone();
            let current_line = active_data.line + 1;

            let ratio = linebreak_compute_cost(nodes, line_lengths, sum, index, &active_data, current_line);
            let next = active_nodes.nodes[inner_idx].next;

            if ratio < -1.0 || (node.kind == LinebreakNodeKind::Penalty && node.penalty == -LINEBREAK_INFINITY) {
                active_nodes.remove(inner_idx);
            }

            if ratio >= -1.0 && ratio <= options.tolerance {
                let badness = 100.0 * ratio.abs().powi(3);
                let mut demerits = if node.kind == LinebreakNodeKind::Penalty && node.penalty >= 0.0 {
                    (options.demerits_line + badness).powi(2) + node.penalty.powi(2)
                } else if node.kind == LinebreakNodeKind::Penalty && node.penalty != -LINEBREAK_INFINITY {
                    (options.demerits_line + badness).powi(2) - node.penalty.powi(2)
                } else {
                    (options.demerits_line + badness).powi(2)
                };

                if node.kind == LinebreakNodeKind::Penalty
                    && nodes[active_data.position].kind == LinebreakNodeKind::Penalty
                {
                    demerits += options.demerits_flagged * node.flagged * nodes[active_data.position].flagged;
                }

                let current_class = if ratio < -0.5 {
                    0
                } else if ratio <= 0.5 {
                    1
                } else if ratio <= 1.0 {
                    2
                } else {
                    3
                };

                if (current_class as i32 - active_data.fitness_class as i32).abs() > 1 {
                    demerits += options.demerits_fitness;
                }

                demerits += active_data.demerits;

                if demerits < candidates[current_class].demerits {
                    candidates[current_class] = LinebreakCandidate {
                        active: Some(inner_idx),
                        demerits,
                    };
                }
            }

            inner = next;
            if let Some(next_idx) = inner {
                if active_nodes.nodes[next_idx].data.line >= current_line {
                    break;
                }
            }
        }

        let insert_before = inner;
        let totals = linebreak_compute_sum(nodes, sum, index);
        for fitness_class in 0..4 {
            let candidate = candidates[fitness_class];
            if candidate.demerits.is_infinite() {
                continue;
            }
            let Some(prev_idx) = candidate.active else {
                continue;
            };
            let line = active_nodes.nodes[prev_idx].data.line + 1;
            let new_breakpoint = LinebreakBreakpoint {
                position: index,
                demerits: candidate.demerits,
                line,
                fitness_class,
                totals,
                previous: Some(prev_idx),
            };
            if let Some(before) = insert_before {
                active_nodes.insert_before(before, new_breakpoint);
            } else {
                active_nodes.push(new_breakpoint);
            }
        }

        active = insert_before;
    }
}

fn linebreak_find_best_breakpoints(active_nodes: &LinebreakActiveList) -> Vec<usize> {
    let mut best: Option<usize> = None;
    let mut current = active_nodes.first();
    while let Some(idx) = current {
        let demerits = active_nodes.nodes[idx].data.demerits;
        if best.is_none() || demerits < active_nodes.nodes[best.unwrap()].data.demerits {
            best = Some(idx);
        }
        current = active_nodes.nodes[idx].next;
    }

    let mut breakpoints = Vec::new();
    let mut node = best;
    while let Some(idx) = node {
        breakpoints.push(active_nodes.nodes[idx].data.position);
        node = active_nodes.nodes[idx].data.previous;
    }
    breakpoints.reverse();
    breakpoints
}

fn linebreak_knuth_plass(
    nodes: &[LinebreakNode],
    line_lengths: &[f32],
    tolerance: f32,
) -> Vec<usize> {
    let options = LinebreakOptions {
        demerits_line: 10.0,
        demerits_flagged: 100.0,
        demerits_fitness: 3000.0,
        tolerance,
    };

    let mut active_nodes = LinebreakActiveList::new();
    active_nodes.push(LinebreakBreakpoint {
        position: 0,
        demerits: 0.0,
        line: 0,
        fitness_class: 0,
        totals: LinebreakSum::default(),
        previous: None,
    });

    let mut sum = LinebreakSum::default();

    for (index, node) in nodes.iter().enumerate() {
        match node.kind {
            LinebreakNodeKind::Box => {
                sum.width += node.width;
            }
            LinebreakNodeKind::Glue => {
                let precedes_box = index > 0 && nodes[index - 1].kind == LinebreakNodeKind::Box;
                if precedes_box {
                    linebreak_main_loop(node, index, nodes, &mut active_nodes, sum, line_lengths, options);
                }
                sum.width += node.width;
                sum.stretch += node.stretch;
                sum.shrink += node.shrink;
            }
            LinebreakNodeKind::Penalty => {
                if node.penalty != LINEBREAK_INFINITY {
                    linebreak_main_loop(node, index, nodes, &mut active_nodes, sum, line_lengths, options);
                }
            }
        }
    }

    linebreak_find_best_breakpoints(&active_nodes)
}

fn linebreak_best_fit(nodes: &[LinebreakNode], widths: &[f32]) -> Vec<usize> {
    fn next_breakpoint(nodes: &[LinebreakNode], widths: &[f32], line_number: usize) -> Option<usize> {
        let mut position: Option<usize> = None;
        let mut minimum_badness = f32::INFINITY;
        let mut sum = LinebreakSum::default();
        let line_length = widths
            .get(line_number)
            .copied()
            .unwrap_or_else(|| *widths.last().unwrap_or(&0.0));

        let calculate_ratio = |node: &LinebreakNode, sum: &LinebreakSum| -> f32 {
            if sum.width < line_length {
                if node.stretch == 0.0 {
                    return LINEBREAK_INFINITY;
                }
                return if sum.stretch - node.stretch > 0.0 {
                    (line_length - sum.width) / sum.stretch
                } else {
                    LINEBREAK_INFINITY
                };
            }

            if sum.width > line_length {
                if node.shrink == 0.0 {
                    return LINEBREAK_INFINITY;
                }
                return if sum.shrink - node.shrink > 0.0 {
                    (line_length - sum.width) / sum.shrink
                } else {
                    LINEBREAK_INFINITY
                };
            }

            0.0
        };

        for (i, node) in nodes.iter().enumerate() {
            match node.kind {
                LinebreakNodeKind::Box => {
                    sum.width += node.width;
                }
                LinebreakNodeKind::Glue => {
                    sum.width += node.width;
                    sum.stretch += node.stretch;
                    sum.shrink += node.shrink;
                }
                LinebreakNodeKind::Penalty => {}
            }

            if sum.width - sum.shrink > line_length {
                if position.is_none() {
                    let mut j = if i == 0 { i + 1 } else { i };
                    while j < nodes.len()
                        && matches!(nodes[j].kind, LinebreakNodeKind::Glue | LinebreakNodeKind::Penalty)
                    {
                        j += 1;
                    }
                    position = Some(j.saturating_sub(1));
                }
                break;
            }

            if matches!(node.kind, LinebreakNodeKind::Penalty | LinebreakNodeKind::Glue) {
                let ratio = calculate_ratio(node, &sum);
                let penalty = if node.kind == LinebreakNodeKind::Penalty {
                    node.penalty
                } else {
                    0.0
                };
                let badness = 100.0 * ratio.abs().powi(3) + penalty;
                if minimum_badness >= badness {
                    position = Some(i);
                    minimum_badness = badness;
                }
            }
        }

        if sum.width - sum.shrink > line_length {
            position
        } else {
            None
        }
    }

    let mut count = 0usize;
    let mut line_number = 0usize;
    let mut subnodes = nodes;
    let mut breakpoints = vec![0usize];

    while !subnodes.is_empty() {
        let breakpoint = next_breakpoint(subnodes, widths, line_number);
        if let Some(breakpoint) = breakpoint {
            count += breakpoint;
            breakpoints.push(count);
            if breakpoint + 1 >= subnodes.len() {
                break;
            }
            subnodes = &subnodes[breakpoint + 1..];
            count += 1;
            line_number += 1;
        } else {
            break;
        }
    }

    breakpoints
}

fn linebreak_paragraph(nodes: &[LinebreakNode], widths: &[f32]) -> Vec<usize> {
    let mut tolerance = 4.0;
    let mut breaks = linebreak_knuth_plass(nodes, widths, tolerance);
    while breaks.is_empty() && tolerance < 50.0 {
        tolerance += 5.0;
        breaks = linebreak_knuth_plass(nodes, widths, tolerance);
    }

    if breaks.is_empty() || (breaks.len() == 1 && breaks[0] == 0) {
        breaks = linebreak_best_fit(nodes, widths);
    }

    breaks
}

pub(super) fn wrap_paragraph_custom(
    paragraph: &str,
    max_width: f32,
    measure: &dyn Fn(&str) -> f32,
    hyphenation: Option<HyphenationLang>,
    advance_map: Option<&[f32]>,
    hyphen_penalty: f32,
) -> Vec<String> {
    if paragraph.trim().is_empty() {
        return vec![String::new()];
    }

    let raw_tokens = split_tokens_with_ranges(paragraph);
    if raw_tokens.is_empty() {
        return vec![String::new()];
    }

    let hyphenator = hyphenation.and_then(hyphenator_for);
    let hyphen_width = if hyphenator.is_some() { 5.0 } else { 0.0 };

    let mut tokens = Vec::new();
    let mut nodes = Vec::new();
    for token in raw_tokens.iter() {
        let width = if let Some(advance_map) = advance_map {
            advance_width_between(advance_map, token.start, token.end)
        } else {
            measure(&token.text)
        };
        let is_space = token.is_space;
        if is_space {
            tokens.push(token.text.clone());
            let stretch = width * 3.0 / 6.0;
            let shrink = width * 3.0 / 9.0;
            nodes.push(LinebreakNode::glue(width, stretch, shrink, tokens.len()));
            continue;
        }

        if let Some(hyphenator) = hyphenator {
            if let Some(segments) = hyphenate_word_segments(&token.text, hyphenator) {
                if let Some(advance_map) = advance_map {
                    let mut segment_start = token.start;
                    for (index, segment) in segments.iter().enumerate() {
                        let segment_end = (segment_start + segment.len()).min(token.end);
                        let segment_width = advance_width_between(advance_map, segment_start, segment_end);
                        tokens.push(segment.clone());
                        nodes.push(LinebreakNode::r#box(segment_width, tokens.len()));
                        if index + 1 < segments.len() {
                            nodes.push(LinebreakNode::penalty(
                                hyphen_width,
                                hyphen_penalty,
                                1.0,
                                tokens.len(),
                                true,
                            ));
                        }
                        segment_start = segment_end;
                    }
                } else {
                    let mut segment_widths = Vec::with_capacity(segments.len());
                    for segment in &segments {
                        segment_widths.push(measure(segment));
                    }

                    let mut boundary_adjusts = Vec::with_capacity(segments.len().saturating_sub(1));
                    for i in 0..segments.len().saturating_sub(1) {
                        let mut pair = String::with_capacity(segments[i].len() + segments[i + 1].len());
                        pair.push_str(&segments[i]);
                        pair.push_str(&segments[i + 1]);
                        let pair_width = measure(&pair);
                        let adjust = pair_width - segment_widths[i] - segment_widths[i + 1];
                        boundary_adjusts.push(adjust);
                    }

                    for (index, segment) in segments.iter().enumerate() {
                        let mut segment_width = segment_widths[index];
                        let adjust = boundary_adjusts.get(index).copied().unwrap_or(0.0);
                        segment_width += adjust;
                        tokens.push(segment.clone());
                        nodes.push(LinebreakNode::r#box(segment_width, tokens.len()));
                        if index + 1 < segments.len() {
                            let penalty_width = hyphen_width - adjust;
                            nodes.push(LinebreakNode::penalty(
                                penalty_width,
                                hyphen_penalty,
                                1.0,
                                tokens.len(),
                                true,
                            ));
                        }
                    }
                }
                continue;
            }
        }

        tokens.push(token.text.clone());
        nodes.push(LinebreakNode::r#box(width, tokens.len()));
        }

    let token_count = tokens.len();
    nodes.push(LinebreakNode::glue(0.0, LINEBREAK_INFINITY, 0.0, token_count));
    nodes.push(LinebreakNode::penalty(
        0.0,
        -LINEBREAK_INFINITY,
        1.0,
        token_count,
        false,
    ));

    let breakpoints = linebreak_paragraph(&nodes, &[max_width]);

    let mut lines = Vec::new();
    let mut start = 0usize;
    for &breakpoint in breakpoints.iter().skip(1) {
        if breakpoint >= nodes.len().saturating_sub(1) {
            break;
        }
        let node = &nodes[breakpoint];
        let end = if node.kind == LinebreakNodeKind::Penalty {
            nodes.get(breakpoint.saturating_sub(1)).map(|prev| prev.token_end).unwrap_or(0)
        } else {
            node.token_end
        };
        let mut line = if end > start {
            tokens[start..end].concat()
        } else {
            String::new()
        };
        if node.kind == LinebreakNodeKind::Penalty && node.insert_hyphen {
            line.push('-');
        }
        lines.push(line);
        start = end;
    }

    let tail = tokens[start..].concat();
    if !tail.is_empty() || lines.is_empty() {
        lines.push(tail);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

pub(super) fn text_width_scaled(
    text: &str,
    metrics: &standard_fonts::StandardFontMetrics,
    font_size: f32,
    letter_spacing: f32,
) -> f32 {
    let units = text_width_units(text, metrics, letter_spacing, font_size);
    units * font_size / 1000.0
}

fn text_width_units(
    text: &str,
    metrics: &standard_fonts::StandardFontMetrics,
    letter_spacing: f32,
    font_size: f32,
) -> f32 {
    let width = 0.0;
    let prev_code: Option<u8> = None;
    let letter_spacing_units = if letter_spacing != 0.0 && font_size > 0.0 {
        letter_spacing * 1000.0 / font_size
    } else {
        0.0
    };
    let (width, _last) = append_width_units(width, prev_code, text, metrics, letter_spacing_units);
    width
}

pub(super) fn text_width_units_custom(
    text: &str,
    widths: &[u16; 256],
    letter_spacing: f32,
    font_size: f32,
) -> f32 {
    let mut width = 0.0;
    let mut prev = false;
    let letter_spacing_units = if letter_spacing != 0.0 && font_size > 0.0 {
        letter_spacing * 1000.0 / font_size
    } else {
        0.0
    };
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        if let Some(code) = standard_fonts::win_ansi_code(ch) {
            if prev {
                width += letter_spacing_units;
            }
            width += widths[code as usize] as f32;
            prev = true;
        }
    }
    width
}

fn append_width_units(
    mut width: f32,
    mut prev_code: Option<u8>,
    text: &str,
    metrics: &standard_fonts::StandardFontMetrics,
    letter_spacing_units: f32,
) -> (f32, Option<u8>) {
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }

        let code = match standard_fonts::win_ansi_code(ch) {
            Some(code) => code,
            None => {
                prev_code = None;
                continue;
            }
        };

        if let Some(prev) = prev_code {
            if let Some(kern) = metrics.kerning.get(&(prev, code)) {
                width += *kern as f32;
            }
            if letter_spacing_units != 0.0 {
                width += letter_spacing_units;
            }
        }

        width += metrics.widths[code as usize] as f32;
        prev_code = Some(code);
    }

    (width, prev_code)
}
