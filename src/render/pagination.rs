use std::collections::HashMap;

use taffy::NodeId;

use crate::components::{BreakType, Component, HyphenationLang, Page, Text, TextSpan, View};
use crate::fonts::{measure_wrapped_text, FontSystem};
use crate::layout::{LayoutEngine, LayoutRect};
use crate::style::{Dimension, Display, FontStyle, Position, Style, TextAlign, TextOverflow};

use super::text::resolve_placeholders;
use super::RenderError;

// Prevent splitting due to low decimal numbers.
const SAFETY_THRESHOLD: f32 = 0.001;

pub(super) fn paginate_page(page: &Page, font_system: &FontSystem) -> Result<Vec<Page>, RenderError> {
    if !page.wrap || page.children.is_empty() {
        let mut single = page.clone();
        single.wrap = false;
        return Ok(vec![single]);
    }

    let mut out = Vec::new();
    let mut current = page.clone();

    // Guard against accidental infinite pagination loops.
    let mut finished = false;
    for page_index in 0..10_000usize {
        let (current_page, next_page) =
            split_page_once(&current, font_system, page_index + 1, 1)?;
        out.push(current_page);
        match next_page {
            None => {
                finished = true;
                break;
            }
            Some(next) => current = next,
        }
    }

    if !finished {
        return Err(RenderError(
            "Pagination error: exceeded maximum page split iterations".to_string(),
        ));
    }

    if out.is_empty() {
        let mut single = page.clone();
        single.wrap = false;
        return Ok(vec![single]);
    }

    // Output pages must not be re-paginated again.
    for page in &mut out {
        page.wrap = false;
    }

    Ok(out)
}

fn split_page_once(
    page: &Page,
    font_system: &FontSystem,
    page_number_guess: usize,
    total_pages_guess: usize,
) -> Result<(Page, Option<Page>), RenderError> {
    let hyphenation = page.hyphenation;
    let measure_text =
        |text: &str,
         font_family: Option<&str>,
         font_size: f32,
         font_weight: Option<u16>,
         font_style: Option<FontStyle>,
         line_height: Option<f32>,
         text_align: Option<TextAlign>,
         letter_spacing: Option<f32>,
         text_indent: Option<f32>,
         max_lines: Option<usize>,
         text_overflow: Option<TextOverflow>,
         max_width: f32| {
            let resolved = resolve_placeholders(text, page_number_guess, total_pages_guess);
            let line_height = font_system.resolve_line_height(
                line_height,
                font_family,
                font_weight,
                font_style,
            );
            measure_wrapped_text(
                &resolved,
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
                font_system,
            )
        };

    let mut engine = LayoutEngine::new();
    let (root, layouts) = engine
        .compute_page_layout(page, &measure_text)
        .map_err(|e| RenderError(format!("Layout error: {}", e)))?;

    let root_children = engine
        .children(root)
        .map_err(|e| RenderError(format!("Layout error: {}", e)))?;
    if root_children.len() != page.children.len() {
        return Err(RenderError(
            "Internal pagination error: layout children mismatch".to_string(),
        ));
    }

    let root_layout = layouts.get(&root).ok_or_else(|| {
        RenderError("Internal pagination error: missing root layout".to_string())
    })?;
    let wrap_bottom = root_layout.content_y + root_layout.content_height;
    let content_area_height = root_layout.content_height;

    let (current_children, next_children) = split_nodes(
        &engine,
        root,
        &page.children,
        &layouts,
        wrap_bottom,
        content_area_height,
        font_system,
        hyphenation,
        page_number_guess,
        total_pages_guess,
    )?;

    let current_page = Page {
        size: page.size,
        orientation: page.orientation,
        style: page.style.clone(),
        children: current_children,
        wrap: false,
        hyphenation: page.hyphenation,
    };

    if next_children.is_empty() || all_fixed(&next_children) {
        return Ok((current_page, None));
    }

    let next_page = Page {
        size: page.size,
        orientation: page.orientation,
        style: page.style.clone(),
        children: next_children,
        wrap: true,
        hyphenation: page.hyphenation,
    };

    Ok((current_page, Some(next_page)))
}

fn split_nodes(
    engine: &LayoutEngine,
    parent: NodeId,
    components: &[Component],
    layouts: &HashMap<NodeId, LayoutRect>,
    wrap_bottom: f32,
    content_area_height: f32,
    font_system: &FontSystem,
    hyphenation: Option<HyphenationLang>,
    page_number_guess: usize,
    total_pages_guess: usize,
) -> Result<(Vec<Component>, Vec<Component>), RenderError> {
    if components.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let node_ids = engine
        .children(parent)
        .map_err(|e| RenderError(format!("Layout error: {}", e)))?;
    if node_ids.len() != components.len() {
        return Err(RenderError(
            "Internal pagination error: node/component mismatch".to_string(),
        ));
    }

    let mut current_children: Vec<Component> = Vec::new();
    let mut next_children: Vec<Component> = Vec::new();

    for i in 0..components.len() {
        let component = &components[i];
        let style = component.style();

        // These do not participate in pagination flow.
        if matches!(style.display, Some(Display::None)) {
            current_children.push(component.clone());
            continue;
        }
        if is_fixed(component) {
            current_children.push(component.clone());
            next_children.push(component.clone());
            continue;
        }

        if style.position == Some(Position::Absolute) {
            current_children.push(component.clone());
            continue;
        }

        let node_id = node_ids[i];
        let layout = layouts.get(&node_id).ok_or_else(|| {
            RenderError("Internal pagination error: missing child layout".to_string())
        })?;
        let node_top = layout.y;
        let node_height = layout.height;
        let node_bottom = node_top + node_height;

        let is_outside = wrap_bottom <= node_top + SAFETY_THRESHOLD;
        let should_split = wrap_bottom + SAFETY_THRESHOLD < node_bottom;
        let can_wrap = can_wrap(component);
        let fits_inside_page = node_height <= content_area_height + SAFETY_THRESHOLD;

        let future = &components[i + 1..];
        let future_fixed: Vec<Component> = future
            .iter()
            .filter(|c| is_fixed(c))
            .cloned()
            .collect();

        let should_break = should_node_break(
            component,
            layout,
            future,
            &node_ids[i + 1..],
            layouts,
            wrap_bottom,
            &current_children,
        );

        if is_outside {
            next_children.push(component.clone());
            continue;
        }

        if !fits_inside_page && !can_wrap {
            // Non-wrappable elements that are larger than the available page height must be placed
            // somewhere to avoid infinite pagination loops.
            current_children.push(component.clone());
            current_children.extend(future_fixed);
            next_children.extend(future.iter().cloned());
            break;
        }

        if should_break {
            let mut moved = component.clone();
            consume_break_before(&mut moved);
            current_children.extend(future_fixed);
            next_children.push(moved);
            next_children.extend(future.iter().cloned());
            break;
        }

        if should_split {
            if !can_wrap {
                current_children.extend(future_fixed);
                next_children.push(component.clone());
                next_children.extend(future.iter().cloned());
                break;
            }

            let Some((split_current, split_next)) = split_component(
                engine,
                node_id,
                component,
                layout,
                layouts,
                wrap_bottom,
                content_area_height,
                font_system,
                hyphenation,
                page_number_guess,
                total_pages_guess,
            )?
            else {
                // Couldn't split (orphans/widows etc). If the node itself is taller than the
                // available content area, keep it on the current page to avoid pagination loops.
                if !fits_inside_page {
                    current_children.push(component.clone());
                    current_children.extend(future_fixed);
                    next_children.extend(future.iter().cloned());
                    break;
                }

                current_children.extend(future_fixed);
                next_children.push(component.clone());
                next_children.extend(future.iter().cloned());
                break;
            };

            if component_has_children(component) && component_children_len(&split_current) == 0 {
                // All children were moved to the next page; showing an empty container is
                // typically not desired.
                if current_children.is_empty() {
                    current_children.push(component.clone());
                    current_children.extend(future_fixed);
                    next_children.extend(future.iter().cloned());
                } else {
                    current_children.extend(future_fixed);
                    next_children.push(component.clone());
                    next_children.extend(future.iter().cloned());
                }
                break;
            }

            current_children.push(split_current);
            next_children.push(split_next);
            continue;
        }

        current_children.push(component.clone());
    }

    Ok((current_children, next_children))
}

fn split_component(
    engine: &LayoutEngine,
    node_id: NodeId,
    component: &Component,
    layout: &LayoutRect,
    layouts: &HashMap<NodeId, LayoutRect>,
    wrap_bottom: f32,
    content_area_height: f32,
    font_system: &FontSystem,
    hyphenation: Option<HyphenationLang>,
    page_number_guess: usize,
    total_pages_guess: usize,
) -> Result<Option<(Component, Component)>, RenderError> {
    match component {
        Component::Text(text) => split_text_component(
            text,
            layout,
            wrap_bottom,
            font_system,
            hyphenation,
            page_number_guess,
            total_pages_guess,
        )
        .map(|opt| opt.map(|(c, n)| (Component::Text(c), Component::Text(n)))),
        Component::View(view) => {
            split_view_component(
                engine,
                node_id,
                view,
                layout,
                layouts,
                wrap_bottom,
                content_area_height,
                font_system,
                hyphenation,
                page_number_guess,
                total_pages_guess,
            )
            .map(|opt| opt.map(|(c, n)| (Component::View(c), Component::View(n))))
        }
        Component::Link(link) => {
            let (current, next) = split_nodes(
                engine,
                node_id,
                &link.children,
                layouts,
                wrap_bottom,
                content_area_height,
                font_system,
                hyphenation,
                page_number_guess,
                total_pages_guess,
            )?;

            if current.is_empty() && !link.children.is_empty() {
                return Ok(None);
            }

            let mut current_link = link.clone();
            let mut next_link = link.clone();
            current_link.break_before = BreakType::None;
            next_link.break_before = BreakType::None;
            current_link.children = current;
            next_link.children = next;

            trim_style_bottom(&mut current_link.style);
            trim_style_top(&mut next_link.style);
            set_split_height(&mut current_link.style, wrap_bottom - layout.y);
            if has_fixed_height(&link.style) {
                let next_height = (layout.height - (wrap_bottom - layout.y)).max(0.0);
                set_split_height(&mut next_link.style, next_height);
            } else {
                next_link.style.height = None;
                next_link.style.min_height = None;
                next_link.style.max_height = None;
            }

            Ok(Some((Component::Link(current_link), Component::Link(next_link))))
        }
        _ => Ok(None),
    }
}

fn split_view_component(
    engine: &LayoutEngine,
    node_id: NodeId,
    view: &View,
    layout: &LayoutRect,
    layouts: &HashMap<NodeId, LayoutRect>,
    wrap_bottom: f32,
    content_area_height: f32,
    font_system: &FontSystem,
    hyphenation: Option<HyphenationLang>,
    page_number_guess: usize,
    total_pages_guess: usize,
) -> Result<Option<(View, View)>, RenderError> {
    let (current_children, next_children) = split_nodes(
        engine,
        node_id,
        &view.children,
        layouts,
        wrap_bottom,
        content_area_height,
        font_system,
        hyphenation,
        page_number_guess,
        total_pages_guess,
    )?;

    if current_children.is_empty() && !view.children.is_empty() {
        return Ok(None);
    }

    let mut current = view.clone();
    let mut next = view.clone();

    // Avoid duplicating anchors on split fragments.
    next.id = None;

    current.children = current_children;
    next.children = next_children;

    trim_style_bottom(&mut current.style);
    trim_style_top(&mut next.style);

    // Match react-pdf behavior by limiting the current fragment height to the remaining space.
    set_split_height(&mut current.style, wrap_bottom - layout.y);

    if has_fixed_height(&view.style) {
        let next_height = (layout.height - (wrap_bottom - layout.y)).max(0.0);
        set_split_height(&mut next.style, next_height);
    } else {
        next.style.height = None;
        next.style.min_height = None;
        next.style.max_height = None;
    }

    Ok(Some((current, next)))
}

fn split_text_component(
    text: &Text,
    layout: &LayoutRect,
    wrap_bottom: f32,
    font_system: &FontSystem,
    _hyphenation: Option<HyphenationLang>,
    _page_number_guess: usize,
    _total_pages_guess: usize,
) -> Result<Option<(Text, Text)>, RenderError> {
    let content_rect = layout.content_rect();
    let max_width = content_rect.width.max(0.0);
    if max_width <= 0.0 {
        return Ok(None);
    }

    // Split using the raw content so placeholders remain available for render-time resolution.
    let (full_text, span_ranges) = build_full_text_and_ranges(text);
    if full_text.is_empty() {
        return Ok(None);
    }

    let font_family = text.style.font_family.as_deref();
    let font_size = text.style.font_size.unwrap_or(12.0);
    let font_weight = text.style.font_weight;
    let font_style = text.style.font_style;
    let letter_spacing = text.style.letter_spacing;
    let text_indent = text.style.text_indent;
    let max_lines = text.style.max_lines;
    let text_overflow = text.style.text_overflow;
    let text_align = text.style.text_align;
    let line_height = font_system.resolve_line_height(
        text.style.line_height,
        font_family,
        font_weight,
        font_style,
    );

    let text_layout = font_system.layout_text(
        &full_text,
        font_family,
        font_size,
        font_weight,
        font_style,
        line_height,
        // Hyphenation complicates mapping line text back to the original spans because it may
        // insert additional hyphens. Prefer stable splitting here.
        None,
        max_width,
        text_align,
        letter_spacing,
        text_indent,
        max_lines,
        text_overflow,
    );
    let lines = text_layout.lines();
    if lines.is_empty() {
        return Ok(None);
    }

    let available_height = (wrap_bottom - content_rect.y).max(0.0);
    let line_height_points = text_layout.line_height.max(0.0);
    if line_height_points <= 0.0 {
        return Ok(None);
    }

    let sliced_line = ((available_height + SAFETY_THRESHOLD) / line_height_points).floor() as usize;
    let mut break_index = get_text_line_break(
        lines.len(),
        sliced_line,
        text.orphans,
        text.widows,
    );

    // If widows/orphans prevent splitting but some lines still fit, fall back to splitting at the
    // maximum fitting line to avoid pagination loops for oversized paragraphs.
    if break_index == 0 {
        if sliced_line > 0 && sliced_line < lines.len() {
            break_index = sliced_line;
        } else {
            return Ok(None);
        }
    }
    if break_index >= lines.len() {
        return Ok(None);
    }

    let line_texts: Vec<&str> = lines.iter().map(|line| line.text.as_str()).collect();
    let line_starts = line_starts_from_text(&full_text, &line_texts);
    if break_index >= line_starts.len() {
        return Ok(None);
    }

    let split_at = line_starts[break_index].min(full_text.len());
    if split_at == 0 || split_at >= full_text.len() {
        return Ok(None);
    }

    let (current_spans, next_spans, current_content, next_content) =
        split_text_content(text, &span_ranges, split_at);

    let mut current = text.clone();
    let mut next = text.clone();
    current.break_before = BreakType::None;
    next.break_before = BreakType::None;
    next.bookmark = None;

    trim_style_bottom(&mut current.style);
    trim_style_top(&mut next.style);

    current.content = current_content;
    current.spans = current_spans;
    next.content = next_content;
    next.spans = next_spans;

    if current.content.is_empty() && current.spans.is_empty() {
        return Ok(None);
    }
    if next.content.is_empty() && next.spans.is_empty() {
        return Ok(None);
    }

    Ok(Some((current, next)))
}

fn get_text_line_break(
    line_count: usize,
    sliced_line: usize,
    orphans: usize,
    widows: usize,
) -> usize {
    if sliced_line == 0 {
        return 0;
    }

    if line_count < orphans {
        return line_count;
    }

    if sliced_line < orphans || line_count < orphans.saturating_add(widows) {
        return 0;
    }

    if line_count == orphans.saturating_add(widows) {
        return orphans;
    }

    if line_count.saturating_sub(sliced_line) < widows {
        return line_count.saturating_sub(widows);
    }

    sliced_line.min(line_count)
}

fn build_full_text_and_ranges(text: &Text) -> (String, Vec<(usize, usize)>) {
    if text.spans.is_empty() {
        return (text.content.clone(), Vec::new());
    }

    let mut full = String::new();
    let mut ranges = Vec::with_capacity(text.spans.len());
    for span in &text.spans {
        let start = full.len();
        full.push_str(&span.content);
        let end = full.len();
        ranges.push((start, end));
    }
    (full, ranges)
}

fn split_text_content(
    text: &Text,
    span_ranges: &[(usize, usize)],
    split_at: usize,
) -> (Vec<TextSpan>, Vec<TextSpan>, String, String) {
    if text.spans.is_empty() {
        let current = text.content[..split_at].to_string();
        let next = text.content[split_at..].to_string();
        return (Vec::new(), Vec::new(), current, next);
    }

    let mut current_spans = Vec::new();
    let mut next_spans = Vec::new();

    for (span, (start, end)) in text.spans.iter().zip(span_ranges.iter()) {
        if *end <= split_at {
            current_spans.push(span.clone());
            continue;
        }
        if *start >= split_at {
            next_spans.push(span.clone());
            continue;
        }

        let rel_split = split_at.saturating_sub(*start);
        let (a, b) = span.content.split_at(rel_split.min(span.content.len()));
        if !a.is_empty() {
            current_spans.push(TextSpan {
                content: a.to_string(),
                style: span.style.clone(),
            });
        }
        if !b.is_empty() {
            next_spans.push(TextSpan {
                content: b.to_string(),
                style: span.style.clone(),
            });
        }
    }

    (current_spans, next_spans, String::new(), String::new())
}

fn line_starts_from_text(full_text: &str, line_texts: &[&str]) -> Vec<usize> {
    let mut starts = Vec::with_capacity(line_texts.len());
    let mut cursor = 0usize;

    for &line_text in line_texts {
        starts.push(cursor);
        if line_text.is_empty() {
            continue;
        }
        if cursor >= full_text.len() {
            continue;
        }
        let mut line_end = if full_text[cursor..].starts_with(line_text) {
            cursor + line_text.len()
        } else {
            advance_by_chars(full_text, cursor, line_text.chars().count())
        };
        if line_end > full_text.len() {
            line_end = full_text.len();
        }
        cursor = skip_whitespace(full_text, line_end);
    }

    starts
}

fn advance_by_chars(text: &str, start: usize, count: usize) -> usize {
    if count == 0 || start >= text.len() {
        return start;
    }

    let mut idx = start;
    let mut remaining = count;
    while remaining > 0 && idx < text.len() {
        let ch = text[idx..].chars().next().unwrap();
        idx += ch.len_utf8();
        remaining -= 1;
    }
    idx
}

fn skip_whitespace(text: &str, start: usize) -> usize {
    let mut idx = start;
    while idx < text.len() {
        let ch = text[idx..].chars().next().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        idx += ch.len_utf8();
    }
    idx
}

fn trim_style_bottom(style: &mut Style) {
    style.margin_bottom = Some(0.0);
    style.padding_bottom = Some(0.0);
    style.border_bottom_width = Some(0.0);
    style.border_bottom_left_radius = Some(0.0);
    style.border_bottom_right_radius = Some(0.0);
}

fn trim_style_top(style: &mut Style) {
    style.margin_top = Some(0.0);
    style.padding_top = Some(0.0);
    style.border_top_width = Some(0.0);
    style.border_top_left_radius = Some(0.0);
    style.border_top_right_radius = Some(0.0);
}

fn set_split_height(style: &mut Style, height: f32) {
    let height = height.max(0.0);
    style.height = Some(Dimension::Points(height));
    // Prevent original min/max constraints from forcing the fragment back to its original size.
    style.min_height = Some(Dimension::Points(height));
    style.max_height = Some(Dimension::Points(height));
}

fn has_fixed_height(style: &Style) -> bool {
    matches!(style.height, Some(d) if d != Dimension::Auto)
}

fn should_node_break(
    child: &Component,
    child_layout: &LayoutRect,
    future: &[Component],
    future_node_ids: &[NodeId],
    layouts: &HashMap<NodeId, LayoutRect>,
    wrap_bottom: f32,
    previous: &[Component],
) -> bool {
    if is_fixed(child) {
        return false;
    }

    let should_split = wrap_bottom < child_layout.y + child_layout.height;
    let can_wrap = can_wrap(child);

    let breaking_improves_presence = previous
        .iter()
        .any(|node| !is_fixed(node) && !matches!(node.style().display, Some(Display::None)));

    let end_of_min_presence_ahead = child_layout.y
        + child_layout.height
        + child.style().margin_bottom()
        + min_presence_ahead(child);

    let furthest_future_end = future
        .iter()
        .zip(future_node_ids.iter())
        .filter(|(node, _)| !is_fixed(node))
        .filter_map(|(_, node_id)| layouts.get(node_id))
        .map(|layout| layout.y + layout.height)
        .reduce(f32::max)
        .unwrap_or(f32::NEG_INFINITY);

    let end_of_presence = end_of_min_presence_ahead.min(furthest_future_end);

    break_before(child) == BreakType::Page
        || (should_split && !can_wrap)
        || (!should_split && end_of_presence > wrap_bottom && breaking_improves_presence)
}

fn all_fixed(nodes: &[Component]) -> bool {
    nodes.iter().all(|node| {
        is_fixed(node) || matches!(node.style().display, Some(Display::None))
    })
}

fn is_fixed(component: &Component) -> bool {
    match component {
        Component::View(v) => v.fixed,
        Component::Text(t) => t.fixed,
        Component::Image(i) => i.fixed,
        Component::Link(l) => l.fixed,
        Component::Note(n) => n.fixed,
    }
}

fn break_before(component: &Component) -> BreakType {
    match component {
        Component::View(v) => v.break_before,
        Component::Text(t) => t.break_before,
        Component::Image(i) => i.break_before,
        Component::Link(l) => l.break_before,
        Component::Note(n) => n.break_before,
    }
}

fn min_presence_ahead(component: &Component) -> f32 {
    match component {
        Component::View(v) => v.min_presence_ahead,
        Component::Text(t) => t.min_presence_ahead,
        Component::Image(i) => i.min_presence_ahead,
        Component::Link(l) => l.min_presence_ahead,
        Component::Note(n) => n.min_presence_ahead,
    }
}

fn can_wrap(component: &Component) -> bool {
    match component {
        Component::Image(_) | Component::Note(_) => false,
        Component::View(v) => v.wrap.unwrap_or(true),
        Component::Text(t) => t.wrap.unwrap_or(true),
        Component::Link(l) => l.wrap.unwrap_or(true),
    }
}

fn consume_break_before(component: &mut Component) {
    match component {
        Component::View(v) => v.break_before = BreakType::None,
        Component::Text(t) => t.break_before = BreakType::None,
        Component::Image(i) => i.break_before = BreakType::None,
        Component::Link(l) => l.break_before = BreakType::None,
        Component::Note(n) => n.break_before = BreakType::None,
    }
}

fn component_has_children(component: &Component) -> bool {
    match component {
        Component::View(v) => !v.children.is_empty(),
        Component::Link(l) => !l.children.is_empty(),
        _ => false,
    }
}

fn component_children_len(component: &Component) -> usize {
    match component {
        Component::View(v) => v.children.len(),
        Component::Link(l) => l.children.len(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::{Component, PageSize, Text, View};
    use crate::style::{Dimension, Style};

    fn view(id: &str) -> View {
        View {
            id: Some(id.to_string()),
            ..View::default()
        }
    }

    fn sized_view(id: &str, height: f32) -> View {
        let mut v = view(id);
        v.style.height = Some(Dimension::Points(height));
        v.style.min_height = Some(Dimension::Points(height));
        v.style.flex_shrink = Some(0.0);
        v
    }

    fn multiline_text(lines: usize) -> Text {
        let mut content = String::new();
        for i in 0..lines {
            if i > 0 {
                content.push('\n');
            }
            content.push_str(&format!("line{}", i + 1));
        }

        Text {
            content,
            style: Style {
                font_size: Some(10.0),
                line_height: Some(1.0),
                ..Style::default()
            },
            ..Text::default()
        }
    }

    #[test]
    fn page_wrap_defaults_to_true() {
        assert!(Page::default().wrap);
    }

    #[test]
    fn fixed_views_repeat_across_manual_page_breaks() {
        let font_system = FontSystem::new();

        let mut header = view("header");
        header.fixed = true;
        let mut footer = view("footer");
        footer.fixed = true;

        let content_a = view("a");

        let mut break_starts_next_page = view("b");
        break_starts_next_page.break_before = BreakType::Page;

        let content_c = view("c");

        let page = Page {
            size: PageSize::Custom(200.0, 200.0),
            children: vec![
                Component::View(header),
                Component::View(content_a),
                Component::View(break_starts_next_page),
                Component::View(content_c),
                Component::View(footer),
            ],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let ids_page_1: Vec<_> = split[0]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_1, vec!["header", "a", "footer"]);

        let ids_page_2: Vec<_> = split[1]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_2, vec!["header", "b", "c", "footer"]);

        // Ensure break marker is consumed on the second page's first element.
        let first_view = match &split[1].children[1] {
            Component::View(v) => v,
            _ => panic!("expected view"),
        };
        assert_eq!(first_view.break_before, BreakType::None);
    }

    #[test]
    fn overflow_paginates_page_children() {
        let font_system = FontSystem::new();

        let page = Page {
            size: PageSize::Custom(200.0, 80.0),
            children: vec![
                Component::View(sized_view("a", 40.0)),
                Component::View(sized_view("b", 40.0)),
                Component::View(sized_view("c", 40.0)),
            ],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let ids_page_1: Vec<_> = split[0]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_1, vec!["a", "b"]);

        let ids_page_2: Vec<_> = split[1]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_2, vec!["c"]);
    }

    #[test]
    fn break_on_text_forces_new_page_and_consumes_break() {
        let font_system = FontSystem::new();

        let text_a = Text::new("a");
        let mut text_b = Text::new("b");
        text_b.break_before = BreakType::Page;

        let page = Page {
            size: PageSize::Custom(200.0, 200.0),
            children: vec![Component::Text(text_a), Component::Text(text_b)],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let page_1_texts: Vec<_> = split[0]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::Text(t) => Some(t.content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(page_1_texts, vec!["a"]);

        let page_2_texts: Vec<_> = split[1]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::Text(t) => Some(t.content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(page_2_texts, vec!["b"]);

        let first = match &split[1].children[0] {
            Component::Text(t) => t,
            _ => panic!("expected text"),
        };
        assert_eq!(first.break_before, BreakType::None);
    }

    #[test]
    fn fixed_text_repeats_across_overflow_pages() {
        let font_system = FontSystem::new();

        let mut header = Text::new("HEADER");
        header.fixed = true;

        let page = Page {
            size: PageSize::Custom(200.0, 100.0),
            children: vec![
                Component::Text(header),
                Component::View(sized_view("a", 40.0)),
                Component::View(sized_view("b", 40.0)),
                Component::View(sized_view("c", 40.0)),
            ],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        for p in &split {
            let first = match &p.children[0] {
                Component::Text(t) => t,
                other => panic!("expected text header, got {:?}", other),
            };
            assert_eq!(first.content, "HEADER");
            assert!(first.fixed);
        }
    }

    #[test]
    fn wrap_false_view_moves_to_next_page() {
        let font_system = FontSystem::new();

        let lead = sized_view("lead", 60.0);
        let mut block = sized_view("block", 60.0);
        block.wrap = Some(false);

        let page = Page {
            size: PageSize::Custom(200.0, 100.0),
            children: vec![Component::View(lead), Component::View(block)],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let ids_page_1: Vec<_> = split[0]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_1, vec!["lead"]);

        let ids_page_2: Vec<_> = split[1]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_2, vec!["block"]);
    }

    #[test]
    fn oversized_wrap_false_view_does_not_loop() {
        let font_system = FontSystem::new();

        let mut big = sized_view("big", 200.0);
        big.wrap = Some(false);

        let tail = sized_view("tail", 20.0);

        let page = Page {
            size: PageSize::Custom(200.0, 100.0),
            children: vec![Component::View(big), Component::View(tail)],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let ids_page_1: Vec<_> = split[0]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_1, vec!["big"]);

        let ids_page_2: Vec<_> = split[1]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_2, vec!["tail"]);
    }

    #[test]
    fn min_presence_ahead_does_not_break_when_heading_is_first() {
        let font_system = FontSystem::new();

        let mut heading = sized_view("heading", 10.0);
        heading.min_presence_ahead = 200.0;
        let body = sized_view("body", 120.0);

        let page = Page {
            size: PageSize::Custom(200.0, 100.0),
            children: vec![Component::View(heading), Component::View(body)],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let ids_page_1: Vec<_> = split[0]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_1, vec!["heading", "body"]);
    }

    #[test]
    fn text_orphans_and_widows_split_by_lines() {
        let font_system = FontSystem::new();

        let text = multiline_text(6);
        let page = Page {
            size: PageSize::Custom(200.0, 35.0),
            children: vec![Component::Text(text)],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let t1 = match &split[0].children[0] {
            Component::Text(t) => t,
            _ => panic!("expected text"),
        };
        let t2 = match &split[1].children[0] {
            Component::Text(t) => t,
            _ => panic!("expected text"),
        };

        assert_eq!(t1.content.lines().count(), 3);
        assert_eq!(t2.content.lines().count(), 3);
    }

    #[test]
    fn text_orphans_and_widows_exact_case_prefers_orphans() {
        let font_system = FontSystem::new();

        let mut text = multiline_text(4);
        text.orphans = 2;
        text.widows = 2;

        let page = Page {
            size: PageSize::Custom(200.0, 35.0),
            children: vec![Component::Text(text)],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let t1 = match &split[0].children[0] {
            Component::Text(t) => t,
            _ => panic!("expected text"),
        };
        let t2 = match &split[1].children[0] {
            Component::Text(t) => t,
            _ => panic!("expected text"),
        };

        assert_eq!(t1.content.lines().count(), 2);
        assert_eq!(t2.content.lines().count(), 2);
    }

    #[test]
    fn fixed_child_repeats_inside_split_wrapper_view() {
        let font_system = FontSystem::new();

        let mut header = sized_view("header", 10.0);
        header.fixed = true;

        let mut wrapper = view("wrapper");
        wrapper.children = vec![
            Component::View(header),
            Component::View(sized_view("a", 40.0)),
            Component::View(sized_view("b", 40.0)),
            Component::View(sized_view("c", 40.0)),
        ];

        let page = Page {
            size: PageSize::Custom(200.0, 90.0),
            children: vec![Component::View(wrapper)],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let wrapper_1 = match &split[0].children[0] {
            Component::View(v) => v,
            _ => panic!("expected wrapper view"),
        };
        let ids_1: Vec<_> = wrapper_1
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_1, vec!["header", "a", "b"]);

        let wrapper_2 = match &split[1].children[0] {
            Component::View(v) => v,
            _ => panic!("expected wrapper view"),
        };
        let ids_2: Vec<_> = wrapper_2
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_2, vec!["header", "c"]);
    }

    #[test]
    fn min_presence_ahead_breaks_heading_with_overflowing_following_content() {
        let font_system = FontSystem::new();

        let lead = sized_view("lead", 70.0);
        let mut heading = sized_view("heading", 10.0);
        heading.min_presence_ahead = 30.0;
        let body = sized_view("body", 40.0);

        let page = Page {
            size: PageSize::Custom(200.0, 100.0),
            children: vec![
                Component::View(lead),
                Component::View(heading),
                Component::View(body),
            ],
            ..Page::default()
        };

        let split = paginate_page(&page, &font_system).unwrap();
        assert_eq!(split.len(), 2);

        let ids_page_1: Vec<_> = split[0]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_1, vec!["lead"]);

        let ids_page_2: Vec<_> = split[1]
            .children
            .iter()
            .filter_map(|component| match component {
                Component::View(v) => v.id.as_deref(),
                _ => None,
            })
            .collect();
        assert_eq!(ids_page_2, vec!["heading", "body"]);
    }
}
