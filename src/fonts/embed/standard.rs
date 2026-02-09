use crate::pdf::{DictBuilder, ObjectRef, PdfObject, PdfWriter};

/// Embed a standard PDF font by name (Type1, no embedding needed).
pub fn embed_standard_font(writer: &mut PdfWriter, base_name: &str) -> ObjectRef {
    let font_dict = DictBuilder::new()
        .entry("Type", PdfObject::Name("Font".to_string()))
        .entry("Subtype", PdfObject::Name("Type1".to_string()))
        .entry("BaseFont", PdfObject::Name(base_name.to_string()))
        .entry("Encoding", PdfObject::Name("WinAnsiEncoding".to_string()))
        .build();

    writer.write_object(&font_dict)
}

/// Embed Noto Sans Regular font - kept for backwards compatibility
/// but now redirects to use Roboto from Google Fonts.
pub fn embed_noto_sans(writer: &mut PdfWriter) -> ObjectRef {
    // Use Helvetica as fallback since we no longer bundle Noto Sans.
    let font_dict = DictBuilder::new()
        .entry("Type", PdfObject::Name("Font".to_string()))
        .entry("Subtype", PdfObject::Name("Type1".to_string()))
        .entry("BaseFont", PdfObject::Name("Helvetica".to_string()))
        .entry("Encoding", PdfObject::Name("WinAnsiEncoding".to_string()))
        .build();

    writer.write_object(&font_dict)
}
