use crate::pdf::{DictBuilder, ObjectRef, PdfObject, PdfWriter};
use read_fonts::TableProvider;

use super::unicode::build_to_unicode_cmap;

/// Embed a TrueType font as a CIDFont (Type0 wrapper) for full glyph access.
pub fn embed_cid_font(
    writer: &mut PdfWriter,
    font_data: &[u8],
    font_name: &str,
    glyph_widths: &[f32],
    used_glyphs: Option<&[u16]>,
) -> ObjectRef {
    let font = match skrifa::FontRef::new(font_data) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("Failed to parse font: {}", e);
            return super::truetype::embed_fallback_font(writer);
        }
    };

    let units_per_em = font
        .head()
        .map(|h| h.units_per_em() as f32)
        .unwrap_or(1000.0);
    let scale = if units_per_em > 0.0 { 1000.0 / units_per_em } else { 1.0 };

    let (ascent, descent) = font
        .hhea()
        .map(|hhea| {
            let asc = (hhea.ascender().to_i16() as f32 * scale).round() as i64;
            let desc = (hhea.descender().to_i16() as f32 * scale).round() as i64;
            (asc, desc)
        })
        .unwrap_or((800, -200));

    let base_name = font_name.replace(' ', "-");
    let subset_tag = used_glyphs
        .filter(|glyphs| !glyphs.is_empty())
        .map(|glyphs| subset_tag_for_glyphs(&base_name, glyphs));
    let pdf_font_name = if let Some(tag) = subset_tag {
        format!("{}+{}", tag, base_name)
    } else {
        base_name
    };

    let font_file_ref = writer.write_stream(font_data, true);

    let font_descriptor = DictBuilder::new()
        .entry("Type", PdfObject::Name("FontDescriptor".to_string()))
        .entry("FontName", PdfObject::Name(pdf_font_name.clone()))
        .entry("Flags", PdfObject::Integer(32))
        .entry(
            "FontBBox",
            PdfObject::Array(vec![
                PdfObject::Integer(-200),
                PdfObject::Integer(descent),
                PdfObject::Integer(1200),
                PdfObject::Integer(ascent),
            ]),
        )
        .entry("ItalicAngle", PdfObject::Integer(0))
        .entry("Ascent", PdfObject::Integer(ascent))
        .entry("Descent", PdfObject::Integer(descent))
        .entry(
            "CapHeight",
            PdfObject::Integer((ascent as f64 * 0.7) as i64),
        )
        .entry("StemV", PdfObject::Integer(80))
        .entry("FontFile2", PdfObject::Reference(font_file_ref))
        .build();

    let descriptor_ref = writer.write_object(&font_descriptor);

    let widths = build_cid_widths(glyph_widths, used_glyphs);

    let to_unicode_stream = build_to_unicode_cmap(font_data, used_glyphs);
    let to_unicode_ref = to_unicode_stream
        .as_ref()
        .map(|data| writer.write_stream(data, false));

    let cid_system_info = PdfObject::Dictionary(vec![
        ("Registry".to_string(), PdfObject::String("Adobe".to_string())),
        ("Ordering".to_string(), PdfObject::String("Identity".to_string())),
        ("Supplement".to_string(), PdfObject::Integer(0)),
    ]);

    let cid_font = DictBuilder::new()
        .entry("Type", PdfObject::Name("Font".to_string()))
        .entry("Subtype", PdfObject::Name("CIDFontType2".to_string()))
        .entry("BaseFont", PdfObject::Name(pdf_font_name.clone()))
        .entry("CIDSystemInfo", cid_system_info)
        .entry("FontDescriptor", PdfObject::Reference(descriptor_ref))
        .entry("W", widths)
        .entry("CIDToGIDMap", PdfObject::Name("Identity".to_string()))
        .build();

    let cid_ref = writer.write_object(&cid_font);

    let mut type0_builder = DictBuilder::new()
        .entry("Type", PdfObject::Name("Font".to_string()))
        .entry("Subtype", PdfObject::Name("Type0".to_string()))
        .entry("BaseFont", PdfObject::Name(pdf_font_name))
        .entry("Encoding", PdfObject::Name("Identity-H".to_string()))
        .entry(
            "DescendantFonts",
            PdfObject::Array(vec![PdfObject::Reference(cid_ref)]),
        );

    if let Some(to_unicode_ref) = to_unicode_ref {
        type0_builder = type0_builder.entry("ToUnicode", PdfObject::Reference(to_unicode_ref));
    }

    let type0 = type0_builder.build();

    writer.write_object(&type0)
}

fn build_cid_widths(glyph_widths: &[f32], used_glyphs: Option<&[u16]>) -> PdfObject {
    if let Some(glyphs) = used_glyphs.filter(|glyphs| !glyphs.is_empty()) {
        let mut sorted: Vec<u16> = glyphs
            .iter()
            .copied()
            .filter(|gid| (*gid as usize) < glyph_widths.len())
            .collect();
        sorted.sort_unstable();
        sorted.dedup();

        let mut entries: Vec<PdfObject> = Vec::new();
        let mut idx = 0usize;
        while idx < sorted.len() {
            let start = sorted[idx];
            let mut end_idx = idx + 1;
            while end_idx < sorted.len() && sorted[end_idx] == sorted[end_idx - 1] + 1 {
                end_idx += 1;
            }

            let mut width_values = Vec::with_capacity(end_idx - idx);
            for gid in &sorted[idx..end_idx] {
                let width = glyph_widths
                    .get(*gid as usize)
                    .copied()
                    .unwrap_or(0.0);
                width_values.push(PdfObject::Real(width as f64));
            }
            entries.push(PdfObject::Integer(start as i64));
            entries.push(PdfObject::Array(width_values));

            idx = end_idx;
        }

        return PdfObject::Array(entries);
    }

    let width_values = glyph_widths
        .iter()
        .map(|w| PdfObject::Real(*w as f64))
        .collect::<Vec<_>>();
    PdfObject::Array(vec![
        PdfObject::Integer(0),
        PdfObject::Array(width_values),
    ])
}

fn subset_tag_for_glyphs(font_name: &str, glyphs: &[u16]) -> String {
    let mut sorted: Vec<u16> = glyphs.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in font_name.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    for gid in sorted {
        for byte in gid.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }

    let mut tag = String::with_capacity(6);
    let mut value = hash;
    for _ in 0..6 {
        let idx = (value % 26) as u8;
        tag.push((b'A' + idx) as char);
        value /= 26;
    }
    tag
}
