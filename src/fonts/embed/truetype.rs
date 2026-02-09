use crate::pdf::{DictBuilder, ObjectRef, PdfObject, PdfWriter};
use read_fonts::TableProvider;
use skrifa::MetadataProvider;

/// Embed a TrueType font and return the font dictionary reference.
/// Takes the raw font data and font name.
pub fn embed_font(writer: &mut PdfWriter, font_data: &[u8], font_name: &str) -> ObjectRef {
    // Parse font to get metrics using skrifa
    let font = match skrifa::FontRef::new(font_data) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("Failed to parse font: {}", e);
            // Return a basic font reference without embedding
            return embed_fallback_font(writer);
        }
    };

    // Get units per em from font
    let units_per_em = font
        .head()
        .map(|h| h.units_per_em() as f32)
        .unwrap_or(1000.0);

    // PDF expects all metrics normalized to 1000 units per em
    let scale = 1000.0 / units_per_em;

    // Get font metrics
    let (ascent, descent) = font
        .hhea()
        .map(|hhea| {
            let asc = (hhea.ascender().to_i16() as f32 * scale).round() as i64;
            let desc = (hhea.descender().to_i16() as f32 * scale).round() as i64;
            (asc, desc)
        })
        .unwrap_or((800, -200));

    // Build character widths array for basic ASCII + Latin-1
    // PDF expects widths normalized to 1000 units per em
    let first_char = 32u8; // space
    let last_char = 255u8; // extended ASCII

    let charmap = font.charmap();
    let glyph_metrics = font.glyph_metrics(
        skrifa::instance::Size::unscaled(),
        skrifa::instance::LocationRef::default(),
    );

    let mut widths = Vec::new();
    for code in first_char..=last_char {
        let ch = code as char;
        let glyph_id = charmap.map(ch).unwrap_or_default();
        let advance = glyph_metrics.advance_width(glyph_id).unwrap_or(0.0);
        // Scale width to 1000 units per em
        let pdf_width = (advance * scale).round() as i64;
        widths.push(PdfObject::Integer(pdf_width));
    }

    // Sanitize font name for PDF (replace spaces with dashes)
    let pdf_font_name = font_name.replace(' ', "-");

    // Write the font file stream (compressed)
    let font_file_ref = writer.write_stream(font_data, true);

    // Create FontDescriptor
    let font_descriptor = DictBuilder::new()
        .entry("Type", PdfObject::Name("FontDescriptor".to_string()))
        .entry("FontName", PdfObject::Name(pdf_font_name.clone()))
        .entry("Flags", PdfObject::Integer(32)) // Nonsymbolic
        .entry(
            "FontBBox",
            PdfObject::Array(vec![
                PdfObject::Integer(-200), // Approximate left bound
                PdfObject::Integer(descent),
                PdfObject::Integer(1200), // Approximate right bound
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

    // Create the Font dictionary (TrueType)
    let font_dict = DictBuilder::new()
        .entry("Type", PdfObject::Name("Font".to_string()))
        .entry("Subtype", PdfObject::Name("TrueType".to_string()))
        .entry("BaseFont", PdfObject::Name(pdf_font_name))
        .entry("FirstChar", PdfObject::Integer(first_char as i64))
        .entry("LastChar", PdfObject::Integer(last_char as i64))
        .entry("Widths", PdfObject::Array(widths))
        .entry("FontDescriptor", PdfObject::Reference(descriptor_ref))
        .entry("Encoding", PdfObject::Name("WinAnsiEncoding".to_string()))
        .build();

    writer.write_object(&font_dict)
}

/// Embed a fallback font (Helvetica - built-in PDF font).
pub(super) fn embed_fallback_font(writer: &mut PdfWriter) -> ObjectRef {
    // Use Helvetica as a fallback (built-in PDF font, no embedding needed).
    let font_dict = DictBuilder::new()
        .entry("Type", PdfObject::Name("Font".to_string()))
        .entry("Subtype", PdfObject::Name("Type1".to_string()))
        .entry("BaseFont", PdfObject::Name("Helvetica".to_string()))
        .entry("Encoding", PdfObject::Name("WinAnsiEncoding".to_string()))
        .build();

    writer.write_object(&font_dict)
}
