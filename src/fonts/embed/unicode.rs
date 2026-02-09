use read_fonts::tables::cmap::{CmapIterLimits, CmapSubtable};
use read_fonts::FontRef;
use read_fonts::TableProvider;
use std::collections::{HashMap, HashSet};

pub(super) fn build_to_unicode_cmap(
    font_data: &[u8],
    used_glyphs: Option<&[u16]>,
) -> Option<Vec<u8>> {
    let font = FontRef::from_index(font_data, 0).ok()?;
    let cmap = font.cmap().ok()?;
    let (_idx, _record, subtable) = cmap.best_subtable()?;

    let mut used_lookup: Option<HashSet<u16>> =
        used_glyphs.map(|glyphs| glyphs.iter().copied().collect());

    let mut glyph_to_codepoint: HashMap<u16, u32> = HashMap::new();
    let mut consider = |codepoint: u32, gid: u16, used: &mut Option<HashSet<u16>>| {
        if codepoint == 0 {
            return;
        }
        if let Some(ref set) = used {
            if !set.contains(&gid) {
                return;
            }
        }
        glyph_to_codepoint
            .entry(gid)
            .and_modify(|existing| {
                if codepoint < *existing {
                    *existing = codepoint;
                }
            })
            .or_insert(codepoint);
    };

    match subtable {
        CmapSubtable::Format4(format4) => {
            for (codepoint, gid) in format4.iter() {
                let gid = gid.to_u32();
                if gid <= u16::MAX as u32 {
                    consider(codepoint, gid as u16, &mut used_lookup);
                }
            }
        }
        CmapSubtable::Format12(format12) => {
            let limits = CmapIterLimits::default_for_font(&font);
            for (codepoint, gid) in format12.iter_with_limits(limits) {
                let gid = gid.to_u32();
                if gid <= u16::MAX as u32 {
                    consider(codepoint, gid as u16, &mut used_lookup);
                }
            }
        }
        CmapSubtable::Format13(format13) => {
            let limits = CmapIterLimits::default_for_font(&font);
            for (codepoint, gid) in format13.iter_with_limits(limits) {
                let gid = gid.to_u32();
                if gid <= u16::MAX as u32 {
                    consider(codepoint, gid as u16, &mut used_lookup);
                }
            }
        }
        CmapSubtable::Format0(format0) => {
            for codepoint in 0u32..=255 {
                if let Some(gid) = format0.map_codepoint(codepoint) {
                    let gid = gid.to_u32();
                    if gid <= u16::MAX as u32 {
                        consider(codepoint, gid as u16, &mut used_lookup);
                    }
                }
            }
        }
        CmapSubtable::Format6(format6) => {
            let first = format6.first_code() as u32;
            let count = format6.entry_count() as u32;
            let glyphs = format6.glyph_id_array();
            for i in 0..count {
                if let Some(gid) = glyphs.get(i as usize) {
                    let gid = gid.get() as u32;
                    if gid <= u16::MAX as u32 {
                        consider(first + i, gid as u16, &mut used_lookup);
                    }
                }
            }
        }
        _ => {}
    }

    if glyph_to_codepoint.is_empty() {
        return None;
    }

    let mut mappings: Vec<(u16, u32)> = glyph_to_codepoint.into_iter().collect();
    mappings.sort_by_key(|(gid, _)| *gid);

    let mut cmap = String::new();
    cmap.push_str("/CIDInit /ProcSet findresource begin\n");
    cmap.push_str("12 dict begin\n");
    cmap.push_str("begincmap\n");
    cmap.push_str(
        "/CIDSystemInfo <<\n  /Registry (Adobe)\n  /Ordering (UCS)\n  /Supplement 0\n>> def\n",
    );
    cmap.push_str("/CMapName /Adobe-Identity-UCS def\n");
    cmap.push_str("/CMapType 2 def\n");
    cmap.push_str("1 begincodespacerange\n<0000><FFFF>\nendcodespacerange\n");

    for chunk in mappings.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (gid, codepoint) in chunk {
            let unicode_hex = encode_unicode_hex(*codepoint);
            cmap.push_str(&format!("<{:04X}><{}>\n", gid, unicode_hex));
        }
        cmap.push_str("endbfchar\n");
    }

    cmap.push_str("endcmap\n");
    cmap.push_str("CMapName currentdict /CMap defineresource pop\n");
    cmap.push_str("end\nend\n");

    Some(cmap.into_bytes())
}

fn encode_unicode_hex(codepoint: u32) -> String {
    if codepoint <= 0xFFFF {
        return format!("{:04X}", codepoint);
    }
    let cp = codepoint - 0x10000;
    let high = 0xD800 + ((cp >> 10) & 0x3FF);
    let low = 0xDC00 + (cp & 0x3FF);
    format!("{:04X}{:04X}", high, low)
}
