//! Content stream builder for drawing operations.

use std::io::Write;

use super::util::{write_number, write_numbers};
use crate::fonts::{GlyphSegment, TextSegment};
use crate::standard_fonts::StandardFontMetrics;

/// Content stream builder for drawing operations.
pub struct ContentStream {
    buffer: Vec<u8>,
}

fn write_pdf_string(buffer: &mut Vec<u8>, text: &str) {
    buffer.push(b'(');
    for ch in text.chars() {
        let code = crate::standard_fonts::win_ansi_code(ch).unwrap_or(b'?');
        match code {
            b'(' => buffer.extend_from_slice(b"\\("),
            b')' => buffer.extend_from_slice(b"\\)"),
            b'\\' => buffer.extend_from_slice(b"\\\\"),
            c => buffer.push(c),
        }
    }
    buffer.push(b')');
}

fn write_pdf_hex_string(buffer: &mut Vec<u8>, bytes: &[u8]) {
    buffer.push(b'<');
    for b in bytes {
        write!(buffer, "{:02X}", b).unwrap();
    }
    buffer.push(b'>');
}

impl ContentStream {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Save graphics state.
    pub fn save(&mut self) {
        self.buffer.extend_from_slice(b"q\n");
    }

    /// Restore graphics state.
    pub fn restore(&mut self) {
        self.buffer.extend_from_slice(b"Q\n");
    }

    /// Set stroke color (RGB).
    pub fn set_stroke_color(&mut self, r: f32, g: f32, b: f32) {
        write_numbers(&mut self.buffer, &[r, g, b]);
        self.buffer.extend_from_slice(b" RG\n");
    }

    /// Set fill color (RGB).
    pub fn set_fill_color(&mut self, r: f32, g: f32, b: f32) {
        write_numbers(&mut self.buffer, &[r, g, b]);
        self.buffer.extend_from_slice(b" rg\n");
    }

    /// Set line width.
    pub fn set_line_width(&mut self, width: f32) {
        write_number(&mut self.buffer, width);
        self.buffer.extend_from_slice(b" w\n");
    }

    /// Set line cap style (0=butt, 1=round, 2=projecting square).
    pub fn set_line_cap(&mut self, cap: u8) {
        let cap = cap.min(2);
        self.buffer.extend_from_slice(cap.to_string().as_bytes());
        self.buffer.extend_from_slice(b" J\n");
    }

    /// Set line dash pattern.
    pub fn set_line_dash(&mut self, pattern: &[f32], phase: f32) {
        self.buffer.push(b'[');
        for (idx, value) in pattern.iter().enumerate() {
            if idx > 0 {
                self.buffer.push(b' ');
            }
            write_number(&mut self.buffer, *value);
        }
        self.buffer.push(b']');
        self.buffer.push(b' ');
        write_number(&mut self.buffer, phase);
        self.buffer.extend_from_slice(b" d\n");
    }

    /// Move to position.
    pub fn move_to(&mut self, x: f32, y: f32) {
        write_numbers(&mut self.buffer, &[x, y]);
        self.buffer.extend_from_slice(b" m\n");
    }

    /// Line to position.
    pub fn line_to(&mut self, x: f32, y: f32) {
        write_numbers(&mut self.buffer, &[x, y]);
        self.buffer.extend_from_slice(b" l\n");
    }

    /// Rectangle.
    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        write_numbers(&mut self.buffer, &[x, y, width, height]);
        self.buffer.extend_from_slice(b" re\n");
    }

    /// Rounded rectangle using Bezier curves.
    /// Uses the standard approximation for circular arcs with Bezier curves.
    pub fn rounded_rect(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32) {
        // Clamp radius to half the smallest dimension
        let radius = radius.min(width / 2.0).min(height / 2.0);

        if radius <= 0.0 {
            // Fall back to regular rectangle
            self.rect(x, y, width, height);
            return;
        }

        // Bezier curve control point distance for approximating a quarter circle
        // k = 4 * (sqrt(2) - 1) / 3 ≈ 0.5523
        let k = 0.5523 * radius;

        // Start at bottom-left, after the corner
        self.move_to(x + radius, y);

        // Bottom edge
        self.line_to(x + width - radius, y);

        // Bottom-right corner (curved)
        self.curve_to(
            x + width - radius + k,
            y,
            x + width,
            y + radius - k,
            x + width,
            y + radius,
        );

        // Right edge
        self.line_to(x + width, y + height - radius);

        // Top-right corner (curved)
        self.curve_to(
            x + width,
            y + height - radius + k,
            x + width - radius + k,
            y + height,
            x + width - radius,
            y + height,
        );

        // Top edge
        self.line_to(x + radius, y + height);

        // Top-left corner (curved)
        self.curve_to(
            x + radius - k,
            y + height,
            x,
            y + height - radius + k,
            x,
            y + height - radius,
        );

        // Left edge
        self.line_to(x, y + radius);

        // Bottom-left corner (curved)
        self.curve_to(
            x,
            y + radius - k,
            x + radius - k,
            y,
            x + radius,
            y,
        );

        // Close path
        self.close_path();
    }

    /// Rounded rectangle with per-corner radii (top-left, top-right, bottom-right, bottom-left).
    pub fn rounded_rect_corners(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        r_tl: f32,
        r_tr: f32,
        r_br: f32,
        r_bl: f32,
    ) {
        let max_r = (width / 2.0).min(height / 2.0);
        let r_tl = r_tl.min(max_r).max(0.0);
        let r_tr = r_tr.min(max_r).max(0.0);
        let r_br = r_br.min(max_r).max(0.0);
        let r_bl = r_bl.min(max_r).max(0.0);

        if r_tl == 0.0 && r_tr == 0.0 && r_br == 0.0 && r_bl == 0.0 {
            self.rect(x, y, width, height);
            return;
        }

        let k = 0.5523_f32;

        self.move_to(x + r_bl, y);
        self.line_to(x + width - r_br, y);
        if r_br > 0.0 {
            let k = k * r_br;
            self.curve_to(
                x + width - r_br + k,
                y,
                x + width,
                y + r_br - k,
                x + width,
                y + r_br,
            );
        } else {
            self.line_to(x + width, y);
        }

        self.line_to(x + width, y + height - r_tr);
        if r_tr > 0.0 {
            let k = k * r_tr;
            self.curve_to(
                x + width,
                y + height - r_tr + k,
                x + width - r_tr + k,
                y + height,
                x + width - r_tr,
                y + height,
            );
        } else {
            self.line_to(x + width, y + height);
        }

        self.line_to(x + r_tl, y + height);
        if r_tl > 0.0 {
            let k = k * r_tl;
            self.curve_to(
                x + r_tl - k,
                y + height,
                x,
                y + height - r_tl + k,
                x,
                y + height - r_tl,
            );
        } else {
            self.line_to(x, y + height);
        }

        self.line_to(x, y + r_bl);
        if r_bl > 0.0 {
            let k = k * r_bl;
            self.curve_to(
                x,
                y + r_bl - k,
                x + r_bl - k,
                y,
                x + r_bl,
                y,
            );
        } else {
            self.line_to(x, y);
        }

        self.close_path();
    }

    /// Bezier curve (cubic).
    pub fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) {
        write_numbers(&mut self.buffer, &[x1, y1, x2, y2, x3, y3]);
        self.buffer.extend_from_slice(b" c\n");
    }

    /// Close path.
    pub fn close_path(&mut self) {
        self.buffer.extend_from_slice(b"h\n");
    }

    /// Stroke path.
    pub fn stroke(&mut self) {
        self.buffer.extend_from_slice(b"S\n");
    }

    /// Fill path.
    pub fn fill(&mut self) {
        self.buffer.extend_from_slice(b"f\n");
    }

    /// Clip to current path (non-zero winding rule).
    pub fn clip(&mut self) {
        self.buffer.extend_from_slice(b"W n\n");
    }

    /// Begin text object.
    pub fn begin_text(&mut self) {
        self.buffer.extend_from_slice(b"BT\n");
    }

    /// End text object.
    pub fn end_text(&mut self) {
        self.buffer.extend_from_slice(b"ET\n");
    }

    /// Set font and size.
    pub fn set_font(&mut self, font_name: &str, size: f32) {
        self.buffer.extend_from_slice(b"/");
        self.buffer.extend_from_slice(font_name.as_bytes());
        self.buffer.push(b' ');
        write_number(&mut self.buffer, size);
        self.buffer.extend_from_slice(b" Tf\n");
    }

    /// Set text position.
    pub fn text_position(&mut self, x: f32, y: f32) {
        // Use absolute text matrix so callers can position lines directly.
        self.buffer.extend_from_slice(b"1 0 0 1 ");
        write_numbers(&mut self.buffer, &[x, y]);
        self.buffer.extend_from_slice(b" Tm\n");
    }

    /// Set text leading (line spacing).
    pub fn set_leading(&mut self, leading: f32) {
        write_number(&mut self.buffer, leading);
        self.buffer.extend_from_slice(b" TL\n");
    }

    /// Set word spacing (text space units).
    pub fn set_word_spacing(&mut self, spacing: f32) {
        write_number(&mut self.buffer, spacing);
        self.buffer.extend_from_slice(b" Tw\n");
    }

    /// Set character spacing (text space units).
    pub fn set_char_spacing(&mut self, spacing: f32) {
        write_number(&mut self.buffer, spacing);
        self.buffer.extend_from_slice(b" Tc\n");
    }

    /// Set graphics state (ExtGState).
    pub fn set_graphics_state(&mut self, name: &str) {
        self.buffer.extend_from_slice(b"/");
        self.buffer.extend_from_slice(name.as_bytes());
        self.buffer.extend_from_slice(b" gs\n");
    }

    /// Show text.
    pub fn show_text(&mut self, text: &str) {
        write_pdf_string(&mut self.buffer, text);
        self.buffer.extend_from_slice(b" Tj\n");
    }

    /// Show text and move to next line.
    pub fn show_text_newline(&mut self, text: &str) {
        write_pdf_string(&mut self.buffer, text);
        self.buffer.extend_from_slice(b" '\n");
    }

    /// Show text using TJ with kerning adjustments.
    pub fn show_text_with_kerning(&mut self, text: &str, metrics: &StandardFontMetrics) {
        if text.is_empty() {
            self.buffer.extend_from_slice(b"() Tj\n");
            return;
        }

        self.buffer.push(b'[');
        let mut segment = String::new();
        let mut prev_code: Option<u8> = None;

        for ch in text.chars() {
            let code = crate::standard_fonts::win_ansi_code(ch);
            if let (Some(prev), Some(curr)) = (prev_code, code) {
                if let Some(kern) = metrics.kerning.get(&(prev, curr)) {
                    if !segment.is_empty() {
                        write_pdf_string(&mut self.buffer, &segment);
                        segment.clear();
                    }
                    self.buffer.push(b' ');
                    write_number(&mut self.buffer, -(*kern as f32));
                    self.buffer.push(b' ');
                }
                segment.push(ch);
                prev_code = Some(curr);
            } else {
                if !segment.is_empty() {
                    write_pdf_string(&mut self.buffer, &segment);
                    segment.clear();
                }
                segment.push(ch);
                prev_code = code;
            }
        }

        if !segment.is_empty() {
            write_pdf_string(&mut self.buffer, &segment);
        }

        self.buffer.extend_from_slice(b"] TJ\n");
    }

    /// Show text using TJ with kerning adjustments map (values in 1000/em units).
    pub fn show_text_with_kerning_map(
        &mut self,
        text: &str,
        kerning: &std::collections::HashMap<(u8, u8), f32>,
    ) {
        if text.is_empty() {
            self.buffer.extend_from_slice(b"() Tj\n");
            return;
        }

        self.buffer.push(b'[');
        let mut segment = String::new();
        let mut prev_code: Option<u8> = None;

        for ch in text.chars() {
            let code = crate::standard_fonts::win_ansi_code(ch);
            if let (Some(prev), Some(curr)) = (prev_code, code) {
                if let Some(kern) = kerning.get(&(prev, curr)) {
                    if !segment.is_empty() {
                        write_pdf_string(&mut self.buffer, &segment);
                        segment.clear();
                    }
                    self.buffer.push(b' ');
                    write_number(&mut self.buffer, -kern);
                    self.buffer.push(b' ');
                }
                segment.push(ch);
                prev_code = Some(curr);
            } else {
                if !segment.is_empty() {
                    write_pdf_string(&mut self.buffer, &segment);
                    segment.clear();
                }
                segment.push(ch);
                prev_code = code;
            }
        }

        if !segment.is_empty() {
            write_pdf_string(&mut self.buffer, &segment);
        }

        self.buffer.extend_from_slice(b"] TJ\n");
    }

    /// Show text using TJ with precomputed segment adjustments (values in 1000/em units).
    pub fn show_text_with_segments(&mut self, segments: &[TextSegment]) {
        if segments.is_empty() {
            self.buffer.extend_from_slice(b"() Tj\n");
            return;
        }

        self.buffer.push(b'[');
        for segment in segments {
            if segment.text.is_empty() {
                continue;
            }
            write_pdf_string(&mut self.buffer, &segment.text);
            if segment.adjust.abs() > 0.01 {
                self.buffer.push(b' ');
                write_number(&mut self.buffer, segment.adjust);
                self.buffer.push(b' ');
            }
        }
        self.buffer.extend_from_slice(b"] TJ\n");
    }

    /// Show text using TJ with precomputed glyph adjustments (values in 1000/em units).
    pub fn show_cid_glyphs_with_segments(&mut self, segments: &[GlyphSegment]) {
        if segments.is_empty() {
            self.buffer.extend_from_slice(b"<> Tj\n");
            return;
        }

        self.buffer.push(b'[');
        for segment in segments {
            let bytes = [(segment.glyph_id >> 8) as u8, (segment.glyph_id & 0xFF) as u8];
            write_pdf_hex_string(&mut self.buffer, &bytes);
            if segment.adjust.abs() > 0.01 {
                self.buffer.push(b' ');
                write_number(&mut self.buffer, segment.adjust);
                self.buffer.push(b' ');
            }
        }
        self.buffer.extend_from_slice(b"] TJ\n");
    }

    /// Move to next line.
    pub fn next_line(&mut self) {
        self.buffer.extend_from_slice(b"T*\n");
    }

    /// Apply transformation matrix.
    pub fn transform_matrix(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
        write_numbers(&mut self.buffer, &[a, b, c, d, e, f]);
        self.buffer.extend_from_slice(b" cm\n");
    }

    /// Draw an XObject (image) by name.
    pub fn draw_xobject(&mut self, name: &str) {
        write!(self.buffer, "/{} Do\n", name).unwrap();
    }

    /// Get the content bytes.
    pub fn finish(self) -> Vec<u8> {
        self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::{write_pdf_string, ContentStream};
    use crate::fonts::GlyphSegment;

    #[test]
    fn write_pdf_string_encodes_win_ansi() {
        let mut buf = Vec::new();
        write_pdf_string(&mut buf, "\u{2022}");
        assert_eq!(buf, vec![b'(', 149, b')']);
    }

    #[test]
    fn show_cid_glyphs_writes_hex_strings() {
        let mut content = ContentStream::new();
        content.show_cid_glyphs_with_segments(&[GlyphSegment {
            glyph_id: 0x0041,
            adjust: 0.0,
            is_whitespace: false,
            is_mark: false,
        }]);
        let output = String::from_utf8(content.finish()).unwrap();
        assert!(output.contains("<0041>"));
    }

    #[test]
    fn show_cid_glyphs_writes_adjustments() {
        let mut content = ContentStream::new();
        content.show_cid_glyphs_with_segments(&[GlyphSegment {
            glyph_id: 0x0042,
            adjust: -120.5,
            is_whitespace: false,
            is_mark: false,
        }]);
        let output = String::from_utf8(content.finish()).unwrap();
        assert!(output.contains("<0042> -120.5"));
    }
}
