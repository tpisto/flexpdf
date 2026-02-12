//! Rendering pipeline that turns a document into PDF bytes.

mod borders;
mod image;
mod imported_pdf;
mod pagination;
mod text;

use std::collections::{HashMap, HashSet};
use taffy::NodeId;

use crate::components::{Component, Document, DocumentSection, HyphenationLang, Page};
use crate::fonts::embed::{embed_cid_font, embed_standard_font};
use crate::fonts::{measure_wrapped_text, FontKey, FontSystem};
use crate::standard_fonts;
use crate::image::{load_image, LoadedImage};
use crate::layout::{ComponentType, LayoutEngine, LayoutRect};
use crate::pdf::{ContentStream, DictBuilder, ObjectRef, PdfObject, PdfWriter};
use crate::style::{
    apply_text_transform,
    Color,
    Display,
    FontStyle,
    Overflow,
    Style,
    TextAlign,
    TextOverflow,
    TransformOp,
};
use borders::render_view_borders;
use image::render_image;
use pagination::paginate_page;
use text::{render_text, render_text_with_spans, resolve_placeholders};

/// Render a document to PDF bytes
pub fn render_document(doc: &Document) -> Result<Vec<u8>, RenderError> {
    let mut writer = PdfWriter::new();
    let font_system = FontSystem::new();

    // Register fonts from document definitions (or load default if none)
    font_system.register_fonts(&doc.fonts);

    // Reserve IDs for document structure
    let catalog_id = writer.reserve_id();
    let pages_id = writer.reserve_id();

    // Build ordered sections (backward compatible: fall back to doc.pages)
    let ordered_sections: Vec<DocumentSection> = if doc.sections.is_empty() {
        doc.pages
            .iter()
            .cloned()
            .map(DocumentSection::Page)
            .collect()
    } else {
        doc.sections.clone()
    };

    // Flatten generated pages (handling page breaks for wrap=true pages)
    let mut pages_to_render: Vec<Page> = Vec::new();
    let mut generated_counts_per_section = Vec::with_capacity(ordered_sections.len());
    for section in &ordered_sections {
        match section {
            DocumentSection::Page(page) => {
                if page.wrap {
                    let split = paginate_page(page, &font_system)?;
                    generated_counts_per_section.push(split.len());
                    pages_to_render.extend(split);
                } else {
                    generated_counts_per_section.push(1);
                    pages_to_render.push(page.clone());
                }
            }
            DocumentSection::ImportPdf(_) => generated_counts_per_section.push(0),
        }
    }

    let total_pages = pages_to_render.len().max(1);
    let mut page_refs = Vec::new();
    let mut bookmarks: Vec<Bookmark> = Vec::new();

    for (index, page) in pages_to_render.iter().enumerate() {
        let page_ref = render_page(
            &mut writer,
            page,
            pages_id,
            &font_system,
            index + 1,
            total_pages,
            &mut bookmarks,
        )?;
        page_refs.push(page_ref);
    }

    // Write Pages dictionary
    let kids: Vec<PdfObject> = page_refs.iter().map(|r| PdfObject::Reference(*r)).collect();

    let pages_dict = DictBuilder::new()
        .entry("Type", PdfObject::Name("Pages".to_string()))
        .entry("Kids", PdfObject::Array(kids))
        .entry("Count", PdfObject::Integer(page_refs.len() as i64))
        .build();

    writer.write_object_at(pages_id, &pages_dict);

    // Write Outlines if bookmarks are present
    let outlines_ref = write_outlines(&mut writer, &bookmarks);

    // Write Catalog
    let mut catalog_builder = DictBuilder::new()
        .entry("Type", PdfObject::Name("Catalog".to_string()))
        .entry("Pages", PdfObject::Reference(pages_id));
    if let Some(outlines) = outlines_ref {
        catalog_builder = catalog_builder.entry("Outlines", PdfObject::Reference(outlines));
    }
    if let Some(ref page_mode) = doc.page_mode {
        if let Some(mode_name) = page_mode_name(page_mode) {
            catalog_builder = catalog_builder.entry("PageMode", PdfObject::Name(mode_name));
        }
    }
    let catalog_dict = catalog_builder.build();
    writer.write_object_at(catalog_id, &catalog_dict);

    // Write Info dictionary
    let mut info_entries = Vec::new();
    if let Some(ref title) = doc.title {
        info_entries.push(("Title".to_string(), PdfObject::String(title.clone())));
    }
    if let Some(ref author) = doc.author {
        info_entries.push(("Author".to_string(), PdfObject::String(author.clone())));
    }
    if let Some(ref subject) = doc.subject {
        info_entries.push(("Subject".to_string(), PdfObject::String(subject.clone())));
    }
    if let Some(ref keywords) = doc.keywords {
        info_entries.push(("Keywords".to_string(), PdfObject::String(keywords.clone())));
    }
    info_entries.push((
        "Producer".to_string(),
        PdfObject::String("flexpdf".to_string()),
    ));

    let info_ref = if !info_entries.is_empty() {
        Some(writer.write_object(&PdfObject::Dictionary(info_entries)))
    } else {
        None
    };

    // Finalize PDF
    let rendered = writer.finish(catalog_id, info_ref);

    if !ordered_sections
        .iter()
        .any(|section| matches!(section, DocumentSection::ImportPdf(_)))
    {
        return Ok(rendered);
    }

    imported_pdf::merge_document_flow(&rendered, &ordered_sections, &generated_counts_per_section)
}

/// Collected image to be embedded
struct CollectedImage {
    name: String,
    src: String,
}

/// Embedded image info for rendering
struct EmbeddedImageInfo {
    name: String,
    width: u32,
    height: u32,
}

#[derive(Clone)]
struct Bookmark {
    title: String,
    page_ref: ObjectRef,
    x: f32,
    y: f32,
}

#[derive(Clone)]
enum LinkDestination {
    Uri(String),
    Internal { page_ref: ObjectRef, x: f32, y: f32 },
}

#[derive(Clone)]
enum Annotation {
    Link { rect: [f32; 4], dest: LinkDestination },
    Note { rect: [f32; 4], content: String },
}

fn render_page(
    writer: &mut PdfWriter,
    page: &Page,
    pages_ref: ObjectRef,
    font_system: &FontSystem,
    page_number: usize,
    total_pages: usize,
    bookmarks: &mut Vec<Bookmark>,
) -> Result<ObjectRef, RenderError> {
    let (page_width, page_height) = page.dimensions();
    let hyphenation = page.hyphenation;

    // Layout the page
    let mut layout_engine = LayoutEngine::new();

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
            let resolved = resolve_placeholders(text, page_number, total_pages);
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

    let (root, layouts) = layout_engine
        .compute_page_layout(page, &measure_text)
        .map_err(|e| RenderError(format!("Layout error: {}", e)))?;

    // Build a mapping from NodeId to component reference
    let mut node_components: HashMap<NodeId, ComponentRef> = HashMap::new();
    build_component_map(root, &layout_engine, &page.children, &mut node_components);

    let used_fonts = collect_used_fonts(&node_components, font_system, page_number, total_pages);
    let used_glyphs =
        collect_used_glyphs(&node_components, &layouts, font_system, page_number, total_pages, hyphenation);
    let (font_map, font_entries) = build_font_resources(writer, font_system, &used_fonts, &used_glyphs);
    let opacity_values = collect_opacity_values(&node_components);
    let (opacity_map, opacity_entries) = build_opacity_resources(writer, &opacity_values);

    // Reserve page reference early for annotations/bookmarks
    let page_ref = writer.reserve_id();

    if std::env::var_os("FLEX_PDF_DEBUG_LAYOUT").is_some() {
        log::debug!("--- Layout dump (page {}) ---", page_number);
        debug_dump_layout(&layout_engine, root, &layouts, &node_components, 0);
    }

    let anchor_map = collect_anchor_map(&node_components, &layouts, page_height, page_ref);
    let mut annotations: Vec<Annotation> = Vec::new();
    collect_annotations(
        &layout_engine,
        root,
        &node_components,
        &layouts,
        &anchor_map,
        page_height,
        page_number,
        total_pages,
        &mut annotations,
    );
    collect_bookmarks(&node_components, &layouts, page_height, page_ref, bookmarks);

    // Collect unique image sources
    let mut collected_images: Vec<CollectedImage> = Vec::new();
    let mut seen_srcs: std::collections::HashSet<String> = std::collections::HashSet::new();
    collect_images(&layout_engine, root, &node_components, &mut collected_images, &mut seen_srcs);

    // Load all images and build info map: src -> (name, width, height)
    let mut image_info_map: HashMap<String, EmbeddedImageInfo> = HashMap::new();
    let mut loaded_images: Vec<(String, LoadedImage)> = Vec::new();

    for img in &collected_images {
        match load_image(&img.src) {
            Ok(loaded) => {
                image_info_map.insert(
                    img.src.clone(),
                    EmbeddedImageInfo {
                        name: img.name.clone(),
                        width: loaded.width,
                        height: loaded.height,
                    },
                );
                loaded_images.push((img.name.clone(), loaded));
            }
            Err(e) => {
                if strict_assets_enabled() {
                    return Err(RenderError(format!(
                        "Failed to load image '{}': {}",
                        img.src, e
                    )));
                }
                log::warn!("Failed to load image '{}': {}", img.src, e);
            }
        }
    }

    // Create content stream
    let mut content = ContentStream::new();

    // Render all nodes
    render_node_tree(
        &mut content,
        &layout_engine,
        root,
        &layouts,
        &node_components,
        page_height,
        font_system,
        &font_map,
        &image_info_map,
        &opacity_map,
        page_number,
        total_pages,
        hyphenation,
    )?;

    // Write content stream
    let content_ref = writer.write_stream(&content.finish(), true);

    // Fonts are already embedded and listed in font_entries above.

    // Embed images as XObjects (using pre-loaded images)
    let mut xobject_entries: Vec<(String, PdfObject)> = Vec::new();
    for (name, loaded) in &loaded_images {
        let xobject_ref = embed_image(writer, loaded);
        xobject_entries.push((name.clone(), PdfObject::Reference(xobject_ref)));
    }

    // Resources dictionary
    let mut resources_entries = vec![
        ("Font".to_string(), PdfObject::Dictionary(font_entries)),
    ];

    if !xobject_entries.is_empty() {
        resources_entries.push((
            "XObject".to_string(),
            PdfObject::Dictionary(xobject_entries),
        ));
    }
    if !opacity_entries.is_empty() {
        resources_entries.push((
            "ExtGState".to_string(),
            PdfObject::Dictionary(opacity_entries),
        ));
    }

    let resources_dict = PdfObject::Dictionary(resources_entries);
    let resources_ref = writer.write_object(&resources_dict);

    let annot_refs = write_annotations(writer, &annotations);

    // Page dictionary
    let mut page_builder = DictBuilder::new()
        .entry("Type", PdfObject::Name("Page".to_string()))
        .entry("Parent", PdfObject::Reference(pages_ref))
        .entry(
            "MediaBox",
            PdfObject::Array(vec![
                PdfObject::Real(0.0),
                PdfObject::Real(0.0),
                PdfObject::Real(page_width as f64),
                PdfObject::Real(page_height as f64),
            ]),
        )
        .entry("Contents", PdfObject::Reference(content_ref))
        .entry("Resources", PdfObject::Reference(resources_ref));

    if !annot_refs.is_empty() {
        let annot_objs = annot_refs
            .into_iter()
            .map(PdfObject::Reference)
            .collect();
        page_builder = page_builder.entry("Annots", PdfObject::Array(annot_objs));
    }
    if let Some(lang) = page.hyphenation {
        page_builder = page_builder.entry("Lang", PdfObject::String(lang.pdf_lang().to_string()));
    }

    let page_dict = page_builder.build();
    writer.write_object_at(page_ref, &page_dict);
    Ok(page_ref)
}

/// Collect all unique image sources from the component tree
fn collect_images(
    engine: &LayoutEngine,
    node: NodeId,
    components: &HashMap<NodeId, ComponentRef>,
    images: &mut Vec<CollectedImage>,
    seen_srcs: &mut std::collections::HashSet<String>,
) {
    if let Some(ComponentRef::Image(src, _, _)) = components.get(&node) {
        if !seen_srcs.contains(src) {
            seen_srcs.insert(src.clone());
            let name = format!("Im{}", images.len() + 1);
            images.push(CollectedImage {
                name,
                src: src.clone(),
            });
        }
    }

    // Recurse to children
    if let Ok(children) = engine.children(node) {
        for child in children {
            collect_images(engine, child, components, images, seen_srcs);
        }
    }
}

/// Embed an image as an XObject and return its reference
fn embed_image(writer: &mut PdfWriter, image: &LoadedImage) -> ObjectRef {
    // Create image XObject dictionary
    let image_dict = DictBuilder::new()
        .entry("Type", PdfObject::Name("XObject".to_string()))
        .entry("Subtype", PdfObject::Name("Image".to_string()))
        .entry("Width", PdfObject::Integer(image.width as i64))
        .entry("Height", PdfObject::Integer(image.height as i64))
        .entry("ColorSpace", PdfObject::Name(image.color_space.clone()))
        .entry("BitsPerComponent", PdfObject::Integer(image.bits_per_component as i64))
        .build();

    // Write the image stream with the dictionary
    writer.write_image_stream(&image.data, image_dict)
}

/// Reference to a component for rendering
#[derive(Clone)]
enum ComponentRef {
    View(Style, Option<String>),
    Text(String, Vec<crate::components::TextSpan>, Style, Option<String>),
    Image(String, crate::components::ObjectFit, Style), // src, object_fit, style
    Link(String, Style),
    Note(String),
}

fn component_style(component: &ComponentRef) -> Option<&Style> {
    match component {
        ComponentRef::View(style, _) => Some(style),
        ComponentRef::Text(_, _, style, _) => Some(style),
        ComponentRef::Image(_, _, style) => Some(style),
        ComponentRef::Link(_, style) => Some(style),
        ComponentRef::Note(_) => None,
    }
}

/// Build mapping from taffy nodes to component data
fn build_component_map(
    node: NodeId,
    engine: &LayoutEngine,
    components: &[Component],
    map: &mut HashMap<NodeId, ComponentRef>,
) {
    // The root node is the page, children map to components
    if let Ok(children) = engine.children(node) {
        for (i, &child_node) in children.iter().enumerate() {
            if i < components.len() {
                map_component_to_node(child_node, engine, &components[i], map);
            }
        }
    }
}

fn map_component_to_node(
    node: NodeId,
    engine: &LayoutEngine,
    component: &Component,
    map: &mut HashMap<NodeId, ComponentRef>,
) {
    match component {
        Component::View(view) => {
            map.insert(node, ComponentRef::View(view.style.clone(), view.id.clone()));

            // Map children
            if let Ok(children) = engine.children(node) {
                for (i, &child_node) in children.iter().enumerate() {
                    if i < view.children.len() {
                        map_component_to_node(child_node, engine, &view.children[i], map);
                    }
                }
            }
        }
        Component::Text(text) => {
            map.insert(
                node,
                ComponentRef::Text(
                    text.content.clone(),
                    text.spans.clone(),
                    text.style.clone(),
                    text.bookmark.clone(),
                ),
            );
        }
        Component::Image(image) => {
            map.insert(
                node,
                ComponentRef::Image(image.src.clone(), image.object_fit, image.style.clone()),
            );
        }
        Component::Link(link) => {
            map.insert(node, ComponentRef::Link(link.src.clone(), link.style.clone()));

            if let Ok(children) = engine.children(node) {
                for (i, &child_node) in children.iter().enumerate() {
                    if i < link.children.len() {
                        map_component_to_node(child_node, engine, &link.children[i], map);
                    }
                }
            }
        }
        Component::Note(note) => {
            map.insert(node, ComponentRef::Note(note.content.clone()));
        }
    }
}

fn collect_used_fonts(
    components: &HashMap<NodeId, ComponentRef>,
    font_system: &FontSystem,
    page_number: usize,
    total_pages: usize,
) -> HashSet<FontKey> {
    let mut used = HashSet::new();
    for component in components.values() {
        if let ComponentRef::Text(text, spans, style, _) = component {
            collect_fonts_for_text(
                text,
                spans,
                style,
                font_system,
                page_number,
                total_pages,
                &mut used,
            );
        }
    }
    used
}

fn collect_opacity_values(components: &HashMap<NodeId, ComponentRef>) -> Vec<f32> {
    let mut values = Vec::new();
    for component in components.values() {
        if let Some(style) = component_style(component) {
            if let Some(opacity) = style.opacity {
                let clamped = opacity.clamp(0.0, 1.0);
                if clamped < 1.0 {
                    values.push(clamped);
                }
            }
        }
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.dedup_by(|a, b| (*a - *b).abs() < 0.0001);
    values
}

fn build_opacity_resources(
    writer: &mut PdfWriter,
    opacities: &[f32],
) -> (HashMap<u32, String>, Vec<(String, PdfObject)>) {
    let mut map = HashMap::new();
    let mut entries = Vec::new();

    for (idx, opacity) in opacities.iter().enumerate() {
        let name = format!("GS{}", idx + 1);
        let dict = DictBuilder::new()
            .entry("Type", PdfObject::Name("ExtGState".to_string()))
            .entry("ca", PdfObject::Real(*opacity as f64))
            .entry("CA", PdfObject::Real(*opacity as f64))
            .build();
        let gs_ref = writer.write_object(&dict);
        entries.push((name.clone(), PdfObject::Reference(gs_ref)));
        map.insert(opacity.to_bits(), name);
    }

    (map, entries)
}

fn collect_used_glyphs(
    components: &HashMap<NodeId, ComponentRef>,
    layouts: &HashMap<NodeId, LayoutRect>,
    font_system: &FontSystem,
    page_number: usize,
    total_pages: usize,
    hyphenation: Option<HyphenationLang>,
) -> HashMap<FontKey, HashSet<u16>> {
    let mut used: HashMap<FontKey, HashSet<u16>> = HashMap::new();

    for (node_id, component) in components {
        let ComponentRef::Text(text, spans, style, _) = component else {
            continue;
        };
        let layout = match layouts.get(node_id) {
            Some(layout) => layout,
            None => continue,
        };

        if spans.is_empty() {
            let resolved = resolve_placeholders(text, page_number, total_pages);
            let resolved = apply_text_transform(&resolved, style.text_transform);
            if resolved.is_empty() {
                continue;
            }

            let font_weight = style.font_weight.unwrap_or(400);
            let font_style = style.font_style;
            let font_family = style.font_family.as_deref();
            let line_height = font_system.resolve_line_height(
                style.line_height,
                font_family,
                Some(font_weight),
                font_style,
            );
            let text_layout = font_system.layout_text(
                &resolved,
                font_family,
                style.font_size.unwrap_or(12.0),
                Some(font_weight),
                font_style,
                line_height,
                hyphenation,
                layout.width,
                style.text_align,
                style.letter_spacing,
                style.text_indent,
                style.max_lines,
                style.text_overflow,
            );

            let font_key = font_system.resolve_font_key(font_family, Some(font_weight), font_style);
            for line in text_layout.lines() {
                if let Some(ref glyphs) = line.glyphs {
                    let entry = used.entry(font_key.clone()).or_default();
                    for glyph in glyphs {
                        entry.insert(glyph.glyph_id);
                    }
                }
            }
        } else {
            let parent_font_size = style.font_size.unwrap_or(12.0);
            let parent_font_weight = style.font_weight.unwrap_or(400);
            let parent_font_style = style.font_style;
            let parent_font_family = style.font_family.clone();
            let line_height = font_system.resolve_line_height(
                style.line_height,
                parent_font_family.as_deref(),
                Some(parent_font_weight),
                parent_font_style,
            );

            for span in spans {
                let resolved = resolve_placeholders(&span.content, page_number, total_pages);
                let span_transform = span.style.text_transform.or(style.text_transform);
                let resolved = apply_text_transform(&resolved, span_transform);
                if resolved.is_empty() {
                    continue;
                }
                let font_size = span.style.font_size.unwrap_or(parent_font_size);
                let font_weight = span.style.font_weight.unwrap_or(parent_font_weight);
                let font_style = span.style.font_style.or(parent_font_style);
                let font_family = span
                    .style
                    .font_family
                    .clone()
                    .or_else(|| parent_font_family.clone());
                let letter_spacing = span.style.letter_spacing.or(style.letter_spacing);

                let inline_glyphs = font_system.layout_inline_glyphs(
                    &resolved,
                    font_family.as_deref(),
                    font_size,
                    Some(font_weight),
                    font_style,
                    line_height,
                    letter_spacing,
                );

                if let Some(glyphs) = inline_glyphs {
                    let font_key =
                        font_system.resolve_font_key(font_family.as_deref(), Some(font_weight), font_style);
                    let entry = used.entry(font_key).or_default();
                    for glyph in glyphs {
                        entry.insert(glyph.glyph_id);
                    }
                }
            }
        }
    }

    used
}

fn collect_fonts_for_text(
    text: &str,
    spans: &[crate::components::TextSpan],
    style: &Style,
    font_system: &FontSystem,
    page_number: usize,
    total_pages: usize,
    used: &mut HashSet<FontKey>,
) {
    if spans.is_empty() {
        let resolved = resolve_placeholders(text, page_number, total_pages);
        if resolved.is_empty() {
            return;
        }
        let font_weight = style.font_weight.unwrap_or(400);
        let font_style = style.font_style;
        let font_family = style.font_family.as_deref();
        used.insert(font_system.resolve_font_key(font_family, Some(font_weight), font_style));
        return;
    }

    let parent_font_weight = style.font_weight.unwrap_or(400);
    let parent_font_style = style.font_style;
    let parent_font_family = style.font_family.clone();

    for span in spans {
        let resolved = resolve_placeholders(&span.content, page_number, total_pages);
        if resolved.is_empty() {
            continue;
        }

        let font_weight = span.style.font_weight.unwrap_or(parent_font_weight);
        let font_style = span.style.font_style.or(parent_font_style);
        let font_family = span
            .style
            .font_family
            .clone()
            .or_else(|| parent_font_family.clone());
        used.insert(font_system.resolve_font_key(font_family.as_deref(), Some(font_weight), font_style));
    }
}

fn build_font_resources(
    writer: &mut PdfWriter,
    font_system: &FontSystem,
    used_fonts: &HashSet<FontKey>,
    used_glyphs: &HashMap<FontKey, HashSet<u16>>,
) -> (HashMap<FontKey, String>, Vec<(String, PdfObject)>) {
    let mut ordered_fonts: Vec<FontKey> = used_fonts.iter().cloned().collect();
    ordered_fonts.sort_by(|a, b| {
        (a.family.as_str(), a.weight, a.is_italic)
            .cmp(&(b.family.as_str(), b.weight, b.is_italic))
    });

    let mut font_map: HashMap<FontKey, String> = HashMap::new();
    let mut font_entries: Vec<(String, PdfObject)> = Vec::new();
    let mut font_index = 1;

    for font_key in ordered_fonts {
        let font_name = format!("F{}", font_index);
        font_index += 1;

        let font_ref = if let Some(custom) = font_system.get_font_variant(
            Some(font_key.family.as_str()),
            Some(font_key.weight),
            font_key.is_italic,
        ) {
            let mut glyphs_vec = used_glyphs
                .get(&font_key)
                .map(|set| set.iter().copied().collect::<Vec<_>>());
            if let Some(ref mut glyphs) = glyphs_vec {
                glyphs.sort_unstable();
                glyphs.dedup();
            }
            embed_cid_font(
                writer,
                &custom.data,
                &custom.family,
                custom.glyph_widths.as_ref(),
                glyphs_vec.as_deref(),
            )
        } else if let Some(variant) = standard_fonts::resolve_standard_variant(
            font_key.family.as_str(),
            font_key.weight,
            font_key.is_italic,
        ) {
            embed_standard_font(writer, variant.name)
        } else {
            log::warn!(
                "Could not resolve font '{}'; falling back to Helvetica",
                font_key.family
            );
            embed_standard_font(writer, "Helvetica")
        };

        font_entries.push((font_name.clone(), PdfObject::Reference(font_ref)));
        font_map.insert(font_key, font_name);
    }

    (font_map, font_entries)
}

fn debug_dump_layout(
    engine: &LayoutEngine,
    node: NodeId,
    layouts: &HashMap<NodeId, LayoutRect>,
    components: &HashMap<NodeId, ComponentRef>,
    depth: usize,
) {
    let indent = "  ".repeat(depth);
    if let Some(layout) = layouts.get(&node) {
        let raw_layout = engine.raw_layout(node);
        let raw_style = engine.raw_style(node);
        let label = match components.get(&node) {
            Some(ComponentRef::View(style, id)) => {
                let base = if let Some(id) = id {
                    format!("View#{}", id)
                } else {
                    "View".to_string()
                };
                let height = style
                    .height
                    .as_ref()
                    .map(|h| format!("{:?}", h))
                    .unwrap_or_else(|| "auto".to_string());
                format!(
                    "{}(pt={:.1} pr={:.1} pb={:.1} pl={:.1} h={})",
                    base,
                    style.padding_top(),
                    style.padding_right(),
                    style.padding_bottom(),
                    style.padding_left(),
                    height
                )
            }
            Some(ComponentRef::Text(text, _, _, _)) => {
                let snippet: String = text.chars().take(40).collect();
                format!("Text(\"{}\")", snippet)
            }
            Some(ComponentRef::Image(src, _, _)) => {
                format!("Image({})", src)
            }
            Some(ComponentRef::Link(src, _)) => {
                format!("Link({})", src)
            }
            Some(ComponentRef::Note(_)) => "Note".to_string(),
            None => {
                if let Some(ctx) = engine.get_context(node) {
                    format!("{:?}", ctx.component_type)
                } else {
                    "Unknown".to_string()
                }
            }
        };

        let raw_info = raw_layout.map(|raw| {
            format!(
                " pad=[{:.1},{:.1},{:.1},{:.1}] border=[{:.1},{:.1},{:.1},{:.1}] margin=[{:.1},{:.1},{:.1},{:.1}]",
                raw.padding.top,
                raw.padding.right,
                raw.padding.bottom,
                raw.padding.left,
                raw.border.top,
                raw.border.right,
                raw.border.bottom,
                raw.border.left,
                raw.margin.top,
                raw.margin.right,
                raw.margin.bottom,
                raw.margin.left,
            )
        });
        let style_info = raw_style.map(|style| {
            format!(
                " size={:?} min={:?} flex=({:.1},{:.1},{:?}) gap=({:?},{:?}) dir={:?}",
                style.size,
                style.min_size,
                style.flex_grow,
                style.flex_shrink,
                style.flex_basis,
                style.gap.width,
                style.gap.height,
                style.flex_direction
            )
        });

        log::debug!(
            "{}{} x={:.2} y={:.2} w={:.2} h={:.2}{}{}",
            indent,
            label,
            layout.x,
            layout.y,
            layout.width,
            layout.height,
            raw_info.unwrap_or_default(),
            style_info.unwrap_or_default()
        );
    }

    if let Ok(children) = engine.children(node) {
        for child in children {
            debug_dump_layout(engine, child, layouts, components, depth + 1);
        }
    }
}

fn strict_assets_enabled() -> bool {
    match std::env::var("FLEXPDF_STRICT_ASSETS") {
        Ok(value) => {
            let value = value.trim().to_lowercase();
            !value.is_empty() && value != "0" && value != "false"
        }
        Err(_) => false,
    }
}

fn render_node_tree(
    content: &mut ContentStream,
    engine: &LayoutEngine,
    node: NodeId,
    layouts: &HashMap<NodeId, LayoutRect>,
    components: &HashMap<NodeId, ComponentRef>,
    page_height: f32,
    font_system: &FontSystem,
    font_map: &HashMap<FontKey, String>,
    image_info_map: &HashMap<String, EmbeddedImageInfo>,
    opacity_map: &HashMap<u32, String>,
    page_number: usize,
    total_pages: usize,
    hyphenation: Option<HyphenationLang>,
) -> Result<(), RenderError> {
    let layout = layouts
        .get(&node)
        .ok_or_else(|| RenderError("Missing layout".to_string()))?;

    let style = components.get(&node).and_then(component_style);
    if let Some(style) = style {
        if matches!(style.display, Some(Display::None)) {
            return Ok(());
        }
    }

    let mut outer_saved = false;
    if let Some(style) = style {
        let opacity = style.opacity.unwrap_or(1.0);
        let has_opacity = opacity < 1.0;
        let has_transform = !style.transform.is_empty();
        if has_opacity || has_transform {
            content.save();
            outer_saved = true;
            if has_opacity {
                if let Some(name) = opacity_map.get(&opacity.to_bits()) {
                    content.set_graphics_state(name);
                }
            }
            if has_transform {
                apply_transform(content, layout, style, page_height);
            }
        }
    }

    let ctx = engine.get_context(node);
    if let Some(ctx) = ctx {
        match ctx.component_type {
            ComponentType::Page => {
                // Just render children
            }
            ComponentType::View => {
                if let Some(ComponentRef::View(style, _)) = components.get(&node) {
                    render_view_borders(content, layout, style, page_height);
                }
            }
            ComponentType::Text => {
                if let Some(ComponentRef::Text(text, spans, style, _)) = components.get(&node) {
                    // Render background/borders first (like View/Image/Link)
                    render_view_borders(content, layout, style, page_height);

                    let text_layout = layout.content_rect();
                    let font_size = style.font_size.unwrap_or(12.0);
                    let font_weight = style.font_weight.unwrap_or(400);
                    let font_style = style.font_style;
                    let font_family = style.font_family.as_deref();
                    let color = style.color.unwrap_or(Color::black());
                    let text_align = style.text_align;
                    let text_decoration = style.text_decoration;
                    let text_decoration_style = style.text_decoration_style;
                    let text_decoration_color = style.text_decoration_color;
                    let letter_spacing = style.letter_spacing;
                    let text_indent = style.text_indent;
                    let text_transform = style.text_transform;
                    let max_lines = style.max_lines;
                    let text_overflow = style.text_overflow;
                    let line_height = font_system.resolve_line_height(
                        style.line_height,
                        font_family,
                        Some(font_weight),
                        font_style,
                    );

                    if spans.is_empty() {
                        // Simple text without spans
                        render_text(
                            content,
                            text,
                            &text_layout,
                            font_family,
                            font_size,
                            font_weight,
                            font_style,
                            line_height,
                            color,
                            text_align,
                            text_decoration,
                            text_decoration_style,
                            text_decoration_color,
                            letter_spacing,
                            text_indent,
                            max_lines,
                            text_overflow,
                            text_transform,
                            page_height,
                            font_system,
                            font_map,
                            page_number,
                            total_pages,
                            hyphenation,
                        );
                    } else {
                        // Text with inline spans
                        render_text_with_spans(
                            content,
                            spans,
                            style,
                            &text_layout,
                            page_height,
                            font_system,
                            font_map,
                            page_number,
                            total_pages,
                            hyphenation,
                        );
                    }
                }
            }
            ComponentType::Image => {
                if let Some(ComponentRef::Image(src, object_fit, style)) = components.get(&node) {
                    // First render any background/border
                    render_view_borders(content, layout, style, page_height);

                    // Then render the image if we have it in our map
                    if let Some(image_info) = image_info_map.get(src) {
                        render_image(
                            content,
                            &image_info.name,
                            layout,
                            page_height,
                            image_info.width,
                            image_info.height,
                            *object_fit,
                            style.object_position,
                        );
                    }
                }
            }
            ComponentType::Link => {
                if let Some(ComponentRef::Link(_, style)) = components.get(&node) {
                    render_view_borders(content, layout, style, page_height);
                }
            }
            ComponentType::Note => {
                // Note is an annotation only; no visible drawing here.
            }
        }
    }

    if let Ok(children) = engine.children(node) {
        let overflow_hidden = style.map_or(false, |s| matches!(s.overflow, Some(Overflow::Hidden)));
        let mut indexed: Vec<(usize, NodeId)> = children.into_iter().enumerate().collect();
        indexed.sort_by(|(a_idx, a), (b_idx, b)| {
            let za = node_z_index(components, *a);
            let zb = node_z_index(components, *b);
            match (za, zb) {
                (None, None) => a_idx.cmp(b_idx),
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(za), Some(zb)) => {
                    if za == zb {
                        a_idx.cmp(b_idx)
                    } else {
                        zb.cmp(&za)
                    }
                }
            }
        });

        let render_children = |content: &mut ContentStream| -> Result<(), RenderError> {
            for (_, child) in indexed.iter() {
                render_node_tree(
                    content,
                    engine,
                    *child,
                    layouts,
                    components,
                    page_height,
                    font_system,
                    font_map,
                    image_info_map,
                    opacity_map,
                    page_number,
                    total_pages,
                    hyphenation,
                )?;
            }
            Ok(())
        };

        if overflow_hidden {
            if let Some(style) = style {
                let bw_left = style.border_left_width();
                let bw_right = style.border_right_width();
                let bw_top = style.border_top_width();
                let bw_bottom = style.border_bottom_width();
                let clip_w = (layout.width - bw_left - bw_right).max(0.0);
                let clip_h = (layout.height - bw_top - bw_bottom).max(0.0);
                let clip_x = layout.x + bw_left;
                let clip_y = layout.y + bw_top;
                if clip_w > 0.0 && clip_h > 0.0 {
                    let pdf_y = page_height - clip_y - clip_h;
                    content.save();
                    content.rect(clip_x, pdf_y, clip_w, clip_h);
                    content.clip();
                    render_children(content)?;
                    content.restore();
                }
            }
        } else {
            render_children(content)?;
        }
    }

    if outer_saved {
        content.restore();
    }

    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct Matrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Matrix {
    fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn translate(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: tx,
            f: ty,
        }
    }

    fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: 0.0,
            f: 0.0,
        }
    }

    fn rotate(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    fn skew(ax: f32, ay: f32) -> Self {
        Self {
            a: 1.0,
            b: ay.tan(),
            c: ax.tan(),
            d: 1.0,
            e: 0.0,
            f: 0.0,
        }
    }

    fn multiply(self, other: Matrix) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }
}

fn matrix_for_transform(op: &TransformOp) -> Matrix {
    match op {
        TransformOp::Translate(tx, ty) => Matrix::translate(*tx, -*ty),
        TransformOp::Scale(sx, sy) => Matrix::scale(*sx, *sy),
        TransformOp::Rotate(angle) => Matrix::rotate(-*angle),
        TransformOp::Skew(ax, ay) => Matrix::skew(-*ax, -*ay),
        TransformOp::Matrix(values) => Matrix {
            a: values[0],
            b: -values[1],
            c: -values[2],
            d: values[3],
            e: values[4],
            f: -values[5],
        },
    }
}

fn apply_transform(content: &mut ContentStream, layout: &LayoutRect, style: &Style, page_height: f32) {
    if style.transform.is_empty() {
        return;
    }

    let origin = style.transform_origin.unwrap_or_default();
    let origin_x = layout.x + layout.width * origin.x;
    let origin_y = layout.y + layout.height * origin.y;
    let origin_y = page_height - origin_y;

    let mut matrix = Matrix::identity();
    for op in &style.transform {
        matrix = matrix.multiply(matrix_for_transform(op));
    }

    let matrix = Matrix::translate(origin_x, origin_y)
        .multiply(matrix)
        .multiply(Matrix::translate(-origin_x, -origin_y));

    content.transform_matrix(matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f);
}

fn node_z_index(components: &HashMap<NodeId, ComponentRef>, node: NodeId) -> Option<i32> {
    components
        .get(&node)
        .and_then(component_style)
        .and_then(|style| style.z_index)
        .and_then(|z_index| if z_index == 0 { None } else { Some(z_index) })
}

fn layout_to_pdf_rect(layout: &LayoutRect, page_height: f32) -> [f32; 4] {
    let x1 = layout.x;
    let y1 = page_height - layout.y - layout.height;
    let x2 = x1 + layout.width;
    let y2 = y1 + layout.height;
    [x1, y1, x2, y2]
}

fn collect_anchor_map(
    components: &HashMap<NodeId, ComponentRef>,
    layouts: &HashMap<NodeId, LayoutRect>,
    page_height: f32,
    page_ref: ObjectRef,
) -> HashMap<String, (ObjectRef, f32, f32)> {
    let mut anchors = HashMap::new();

    for (node_id, component) in components {
        if let ComponentRef::View(_, Some(id)) = component {
            if let Some(layout) = layouts.get(node_id) {
                let x = layout.x;
                let y = page_height - layout.y;
                anchors.insert(id.clone(), (page_ref, x, y));
            }
        }
    }

    anchors
}

fn collect_annotations(
    engine: &LayoutEngine,
    node: NodeId,
    components: &HashMap<NodeId, ComponentRef>,
    layouts: &HashMap<NodeId, LayoutRect>,
    anchors: &HashMap<String, (ObjectRef, f32, f32)>,
    page_height: f32,
    page_number: usize,
    total_pages: usize,
    annotations: &mut Vec<Annotation>,
) {
    if let Some(component) = components.get(&node) {
        match component {
            ComponentRef::Link(src, _) => {
                if let Some(layout) = layouts.get(&node) {
                    if layout.width <= 0.0 || layout.height <= 0.0 {
                        // Skip zero-sized link annotations.
                    } else {
                        let rect = layout_to_pdf_rect(layout, page_height);
                        if !src.is_empty() {
                            let dest = if src.starts_with('#') {
                                let target = src.trim_start_matches('#');
                                anchors.get(target).map(|(page_ref, x, y)| {
                                    LinkDestination::Internal {
                                        page_ref: *page_ref,
                                        x: *x,
                                        y: *y,
                                    }
                                })
                            } else {
                                Some(LinkDestination::Uri(src.clone()))
                            };

                            if let Some(dest) = dest {
                                annotations.push(Annotation::Link { rect, dest });
                            }
                        }
                    }
                }
            }
            ComponentRef::Note(content) => {
                if let Some(layout) = layouts.get(&node) {
                    let mut rect = layout_to_pdf_rect(layout, page_height);
                    if rect[2] - rect[0] <= 0.0 || rect[3] - rect[1] <= 0.0 {
                        let size = 10.0;
                        rect = [
                            layout.x,
                            page_height - layout.y - size,
                            layout.x + size,
                            page_height - layout.y,
                        ];
                    }
                    let resolved = resolve_placeholders(content, page_number, total_pages);
                    annotations.push(Annotation::Note {
                        rect,
                        content: resolved,
                    });
                }
            }
            _ => {}
        }
    }

    if let Ok(children) = engine.children(node) {
        for child in children {
            collect_annotations(
                engine,
                child,
                components,
                layouts,
                anchors,
                page_height,
                page_number,
                total_pages,
                annotations,
            );
        }
    }
}

fn collect_bookmarks(
    components: &HashMap<NodeId, ComponentRef>,
    layouts: &HashMap<NodeId, LayoutRect>,
    page_height: f32,
    page_ref: ObjectRef,
    bookmarks: &mut Vec<Bookmark>,
) {
    for (node_id, component) in components {
        if let ComponentRef::Text(_, _, _, Some(title)) = component {
            if let Some(layout) = layouts.get(node_id) {
                let x = layout.x;
                let y = page_height - layout.y;
                bookmarks.push(Bookmark {
                    title: title.clone(),
                    page_ref,
                    x,
                    y,
                });
            }
        }
    }
}

fn write_annotations(writer: &mut PdfWriter, annotations: &[Annotation]) -> Vec<ObjectRef> {
    let mut refs = Vec::new();
    for ann in annotations {
        let rect = match ann {
            Annotation::Link { rect, .. } => rect,
            Annotation::Note { rect, .. } => rect,
        };

        let rect_array = PdfObject::Array(vec![
            PdfObject::Real(rect[0] as f64),
            PdfObject::Real(rect[1] as f64),
            PdfObject::Real(rect[2] as f64),
            PdfObject::Real(rect[3] as f64),
        ]);

        let obj = match ann {
            Annotation::Link { dest, .. } => {
                let mut dict = DictBuilder::new()
                    .entry("Type", PdfObject::Name("Annot".to_string()))
                    .entry("Subtype", PdfObject::Name("Link".to_string()))
                    .entry("Rect", rect_array)
                    .entry(
                        "Border",
                        PdfObject::Array(vec![
                            PdfObject::Integer(0),
                            PdfObject::Integer(0),
                            PdfObject::Integer(0),
                        ]),
                    );

                match dest {
                    LinkDestination::Uri(uri) => {
                        let action = PdfObject::Dictionary(vec![
                            ("S".to_string(), PdfObject::Name("URI".to_string())),
                            ("URI".to_string(), PdfObject::String(uri.clone())),
                        ]);
                        dict = dict.entry("A", action);
                    }
                    LinkDestination::Internal { page_ref, x, y } => {
                        let dest_array = PdfObject::Array(vec![
                            PdfObject::Reference(*page_ref),
                            PdfObject::Name("XYZ".to_string()),
                            PdfObject::Real(*x as f64),
                            PdfObject::Real(*y as f64),
                            PdfObject::Null,
                        ]);
                        dict = dict.entry("Dest", dest_array);
                    }
                }

                dict.build()
            }
            Annotation::Note { content, .. } => {
                DictBuilder::new()
                    .entry("Type", PdfObject::Name("Annot".to_string()))
                    .entry("Subtype", PdfObject::Name("Text".to_string()))
                    .entry("Rect", rect_array)
                    .entry("Contents", PdfObject::String(content.clone()))
                    .build()
            }
        };

        refs.push(writer.write_object(&obj));
    }

    refs
}

fn write_outlines(writer: &mut PdfWriter, bookmarks: &[Bookmark]) -> Option<ObjectRef> {
    if bookmarks.is_empty() {
        return None;
    }

    let mut items = bookmarks.to_vec();
    items.sort_by(|a, b| {
        let page_cmp = a.page_ref.0.cmp(&b.page_ref.0);
        if page_cmp != std::cmp::Ordering::Equal {
            return page_cmp;
        }
        b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal)
    });

    let outline_ref = writer.reserve_id();
    let mut item_refs = Vec::new();
    for _ in &items {
        item_refs.push(writer.reserve_id());
    }

    for (idx, bookmark) in items.iter().enumerate() {
        let mut dict = DictBuilder::new()
            .entry("Title", PdfObject::String(bookmark.title.clone()))
            .entry("Parent", PdfObject::Reference(outline_ref))
            .entry(
                "Dest",
                PdfObject::Array(vec![
                    PdfObject::Reference(bookmark.page_ref),
                    PdfObject::Name("XYZ".to_string()),
                    PdfObject::Real(bookmark.x as f64),
                    PdfObject::Real(bookmark.y as f64),
                    PdfObject::Null,
                ]),
            );

        if idx > 0 {
            dict = dict.entry("Prev", PdfObject::Reference(item_refs[idx - 1]));
        }
        if idx + 1 < item_refs.len() {
            dict = dict.entry("Next", PdfObject::Reference(item_refs[idx + 1]));
        }

        let item_dict = dict.build();
        writer.write_object_at(item_refs[idx], &item_dict);
    }

    let outline_dict = DictBuilder::new()
        .entry("Type", PdfObject::Name("Outlines".to_string()))
        .entry("First", PdfObject::Reference(item_refs[0]))
        .entry("Last", PdfObject::Reference(item_refs[item_refs.len() - 1]))
        .entry("Count", PdfObject::Integer(item_refs.len() as i64))
        .build();
    writer.write_object_at(outline_ref, &outline_dict);

    Some(outline_ref)
}

fn page_mode_name(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut chars = trimmed.chars();
    let first = chars.next()?.to_uppercase().to_string();
    Some(format!("{}{}", first, chars.as_str()))
}

#[derive(Debug)]
pub struct RenderError(pub String);

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RenderError {}

impl From<crate::layout::LayoutError> for RenderError {
    fn from(e: crate::layout::LayoutError) -> Self {
        RenderError(e.0)
    }
}
