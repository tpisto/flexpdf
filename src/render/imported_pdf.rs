use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;

use flate2::read::{DeflateDecoder, ZlibDecoder};
use crate::components::{DocumentSection, ImportedPdfPages};

use super::RenderError;

type ObjId = (u32, u16);
type Dict = BTreeMap<String, Primitive>;

#[derive(Clone, Debug)]
enum Primitive {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Name(String),
    StringLiteral(Vec<u8>),
    HexString(Vec<u8>),
    Array(Vec<Primitive>),
    Dictionary(Dict),
    Reference(ObjId),
}

#[derive(Clone, Debug)]
enum IndirectObject {
    Primitive(Primitive),
    Stream { dict: Dict, data: Vec<u8> },
}

#[derive(Clone)]
struct ParsedPdf {
    objects: BTreeMap<ObjId, IndirectObject>,
    pages: Vec<ObjId>,
}

#[derive(Clone, Copy)]
enum XRefEntry {
    InUse { offset: usize },
    Compressed { object_stream: ObjId, index: u32 },
}

pub(super) fn merge_document_flow(
    rendered_pdf: &[u8],
    ordered_sections: &[DocumentSection],
    generated_counts_per_section: &[usize],
) -> Result<Vec<u8>, RenderError> {
    if ordered_sections.len() != generated_counts_per_section.len() {
        return Err(RenderError(
            "Internal error: section and generated page counts mismatch".to_string(),
        ));
    }

    let generated_pdf = ParsedPdf::parse(rendered_pdf)?;
    let mut assembler = PdfAssembler::new();
    let generated_page_map = assembler.import_document(&generated_pdf);

    let mut generated_pages_new_ids = Vec::with_capacity(generated_pdf.pages.len());
    for page_id in &generated_pdf.pages {
        let mapped = generated_page_map
            .get(page_id)
            .copied()
            .ok_or_else(|| RenderError("Internal error: missing generated page mapping".to_string()))?;
        generated_pages_new_ids.push(mapped);
    }

    let mut ordered_output_pages: Vec<ObjId> = Vec::new();
    let mut generated_cursor = 0usize;

    for (index, section) in ordered_sections.iter().enumerate() {
        match section {
            DocumentSection::Page(_) => {
                let count = generated_counts_per_section[index];
                if generated_cursor + count > generated_pages_new_ids.len() {
                    return Err(RenderError(
                        "Internal error: generated page cursor overflow".to_string(),
                    ));
                }
                ordered_output_pages
                    .extend(generated_pages_new_ids[generated_cursor..generated_cursor + count].iter().copied());
                generated_cursor += count;
            }
            DocumentSection::ImportPdf(import) => {
                let import_bytes = load_import_bytes(import)?;
                let imported_pdf = ParsedPdf::parse(&import_bytes)?;
                let page_map = assembler.import_document(&imported_pdf);
                append_selected_import_pages(import, &imported_pdf, &page_map, &mut ordered_output_pages)?;
            }
        }
    }

    if generated_cursor != generated_pages_new_ids.len() {
        return Err(RenderError(
            "Internal error: not all generated pages were consumed".to_string(),
        ));
    }

    if ordered_output_pages.is_empty() {
        return Err(RenderError("Cannot create a PDF with zero pages".to_string()));
    }

    assembler.finalize(ordered_output_pages)
}

fn append_selected_import_pages(
    import: &ImportedPdfPages,
    parsed: &ParsedPdf,
    page_map: &HashMap<ObjId, ObjId>,
    out: &mut Vec<ObjId>,
) -> Result<(), RenderError> {
    if import.src.trim().is_empty() && import.bytes.is_none() {
        return Err(RenderError(
            "ImportPdf source cannot be empty (provide src or bytes)".to_string(),
        ));
    }
    if import.pages.is_empty() {
        let source_label = import_source_label(import);
        return Err(RenderError(format!(
            "ImportPdf '{}' must include at least one page",
            source_label
        )));
    }

    for page_number in &import.pages {
        if *page_number == 0 {
            let source_label = import_source_label(import);
            return Err(RenderError(format!(
                "ImportPdf '{}' contains invalid page number 0",
                source_label
            )));
        }
        let index = (*page_number as usize).saturating_sub(1);
        let source_label = import_source_label(import);
        let old_page_id = parsed.pages.get(index).copied().ok_or_else(|| {
            RenderError(format!(
                "ImportPdf '{}' requested page {}, but source has {} page(s)",
                source_label,
                page_number,
                parsed.pages.len()
            ))
        })?;
        let mapped = page_map.get(&old_page_id).copied().ok_or_else(|| {
            RenderError(format!(
                "Internal error: missing mapped imported page {} from '{}'",
                page_number, source_label
            ))
        })?;
        out.push(mapped);
    }

    Ok(())
}

fn load_import_bytes(import: &ImportedPdfPages) -> Result<Vec<u8>, RenderError> {
    if let Some(bytes) = &import.bytes {
        if bytes.is_empty() {
            return Err(RenderError(
                "ImportPdf bytes cannot be empty".to_string(),
            ));
        }
        return Ok(bytes.clone());
    }

    if import.src.trim().is_empty() {
        return Err(RenderError(
            "ImportPdf source cannot be empty (provide src or bytes)".to_string(),
        ));
    }

    fs::read(&import.src)
        .map_err(|e| RenderError(format!("Failed to read imported PDF '{}': {}", import.src, e)))
}

fn import_source_label(import: &ImportedPdfPages) -> String {
    if import.src.trim().is_empty() {
        "<in-memory PDF>".to_string()
    } else {
        import.src.clone()
    }
}

struct PdfAssembler {
    objects: BTreeMap<ObjId, IndirectObject>,
    next_id: u32,
}

impl PdfAssembler {
    fn new() -> Self {
        Self {
            objects: BTreeMap::new(),
            next_id: 1,
        }
    }

    fn alloc_id(&mut self) -> ObjId {
        let id = (self.next_id, 0);
        self.next_id += 1;
        id
    }

    fn import_document(&mut self, doc: &ParsedPdf) -> HashMap<ObjId, ObjId> {
        let mut id_map = HashMap::new();
        for old_id in doc.objects.keys() {
            id_map.insert(*old_id, self.alloc_id());
        }

        for (old_id, object) in &doc.objects {
            let new_id = id_map[old_id];
            let remapped = remap_indirect_object(object, &id_map);
            self.objects.insert(new_id, remapped);
        }

        let mut page_map = HashMap::new();
        for old_page in &doc.pages {
            if let Some(new_page) = id_map.get(old_page).copied() {
                page_map.insert(*old_page, new_page);
            }
        }
        page_map
    }

    fn finalize(mut self, ordered_pages: Vec<ObjId>) -> Result<Vec<u8>, RenderError> {
        let pages_id = self.alloc_id();
        let catalog_id = self.alloc_id();

        for page_id in &ordered_pages {
            let page_object = self.objects.get_mut(page_id).ok_or_else(|| {
                RenderError("Internal error: selected page object not found".to_string())
            })?;
            let page_dict = object_as_dict_mut(page_object).ok_or_else(|| {
                RenderError("Selected page object is not a dictionary".to_string())
            })?;
            page_dict.insert("Type".to_string(), Primitive::Name("Page".to_string()));
            page_dict.insert("Parent".to_string(), Primitive::Reference(pages_id));
        }

        let kids = ordered_pages
            .into_iter()
            .map(Primitive::Reference)
            .collect::<Vec<_>>();

        let mut pages_dict = Dict::new();
        pages_dict.insert("Type".to_string(), Primitive::Name("Pages".to_string()));
        pages_dict.insert("Kids".to_string(), Primitive::Array(kids));
        pages_dict.insert("Count".to_string(), Primitive::Integer(pages_dict_page_count(&pages_dict)));
        self.objects
            .insert(pages_id, IndirectObject::Primitive(Primitive::Dictionary(pages_dict)));

        let mut catalog_dict = Dict::new();
        catalog_dict.insert("Type".to_string(), Primitive::Name("Catalog".to_string()));
        catalog_dict.insert("Pages".to_string(), Primitive::Reference(pages_id));
        self.objects.insert(
            catalog_id,
            IndirectObject::Primitive(Primitive::Dictionary(catalog_dict)),
        );

        write_pdf(&self.objects, catalog_id)
    }
}

fn pages_dict_page_count(dict: &Dict) -> i64 {
    match dict.get("Kids") {
        Some(Primitive::Array(kids)) => kids.len() as i64,
        _ => 0,
    }
}

fn object_as_dict_mut(object: &mut IndirectObject) -> Option<&mut Dict> {
    match object {
        IndirectObject::Primitive(Primitive::Dictionary(dict)) => Some(dict),
        IndirectObject::Stream { dict, .. } => Some(dict),
        _ => None,
    }
}

fn remap_indirect_object(object: &IndirectObject, id_map: &HashMap<ObjId, ObjId>) -> IndirectObject {
    match object {
        IndirectObject::Primitive(primitive) => {
            IndirectObject::Primitive(remap_primitive(primitive, id_map))
        }
        IndirectObject::Stream { dict, data } => IndirectObject::Stream {
            dict: remap_dict(dict, id_map),
            data: data.clone(),
        },
    }
}

fn remap_dict(dict: &Dict, id_map: &HashMap<ObjId, ObjId>) -> Dict {
    let mut out = Dict::new();
    for (key, value) in dict {
        out.insert(key.clone(), remap_primitive(value, id_map));
    }
    out
}

fn remap_primitive(primitive: &Primitive, id_map: &HashMap<ObjId, ObjId>) -> Primitive {
    match primitive {
        Primitive::Null => Primitive::Null,
        Primitive::Boolean(v) => Primitive::Boolean(*v),
        Primitive::Integer(v) => Primitive::Integer(*v),
        Primitive::Real(v) => Primitive::Real(*v),
        Primitive::Name(v) => Primitive::Name(v.clone()),
        Primitive::StringLiteral(v) => Primitive::StringLiteral(v.clone()),
        Primitive::HexString(v) => Primitive::HexString(v.clone()),
        Primitive::Array(items) => {
            Primitive::Array(items.iter().map(|item| remap_primitive(item, id_map)).collect())
        }
        Primitive::Dictionary(dict) => Primitive::Dictionary(remap_dict(dict, id_map)),
        Primitive::Reference(reference) => {
            if let Some(mapped) = id_map.get(reference) {
                Primitive::Reference(*mapped)
            } else {
                Primitive::Reference(*reference)
            }
        }
    }
}

impl ParsedPdf {
    fn parse(data: &[u8]) -> Result<Self, RenderError> {
        let xref_start = find_startxref(data)?;
        let (xref_entries, trailer) = parse_xref_chain(data, xref_start)?;

        let mut objects = BTreeMap::new();
        let mut offsets = xref_entries
            .values()
            .filter_map(|entry| match entry {
                XRefEntry::InUse { offset } => Some(*offset),
                XRefEntry::Compressed { .. } => None,
            })
            .collect::<Vec<_>>();
        offsets.sort_unstable();
        offsets.dedup();

        for offset in offsets {
            let (object_id, object) = parse_indirect_object(data, offset, &objects)?;
            objects.insert(object_id, object);
        }

        let mut compressed_targets: HashMap<ObjId, Vec<(ObjId, u32)>> = HashMap::new();
        for (object_id, entry) in &xref_entries {
            if let XRefEntry::Compressed {
                object_stream,
                index,
            } = entry
            {
                compressed_targets
                    .entry(*object_stream)
                    .or_default()
                    .push((*object_id, *index));
            }
        }

        for (object_stream, targets) in compressed_targets {
            let entries = parse_object_stream_entries(&objects, object_stream)?;

            for (target_id, stream_index) in targets {
                let index = stream_index as usize;
                let candidate = entries.get(index).map(|(_, primitive)| primitive.clone());
                let primitive = if let Some(primitive) = candidate {
                    primitive
                } else if let Some((_, primitive)) =
                    entries.iter().find(|(obj_num, _)| *obj_num == target_id.0)
                {
                    primitive.clone()
                } else {
                    return Err(RenderError(format!(
                        "Compressed object {:?} missing from object stream {:?}",
                        target_id, object_stream
                    )));
                };
                objects.insert(target_id, IndirectObject::Primitive(primitive));
            }
        }

        let root_id = trailer
            .get("Root")
            .and_then(as_reference)
            .ok_or_else(|| RenderError("PDF trailer is missing /Root reference".to_string()))?;
        let root_dict = object_dict(&objects, root_id).ok_or_else(|| {
            RenderError("PDF catalog object is missing or malformed".to_string())
        })?;
        let pages_root = root_dict
            .get("Pages")
            .and_then(as_reference)
            .ok_or_else(|| RenderError("PDF catalog is missing /Pages reference".to_string()))?;

        let mut pages = Vec::new();
        let mut visited = HashSet::new();
        collect_pages(&objects, pages_root, &mut visited, &mut pages)?;

        if pages.is_empty() {
            return Err(RenderError("PDF does not contain any pages".to_string()));
        }

        Ok(Self { objects, pages })
    }
}

fn parse_object_stream_entries(
    objects: &BTreeMap<ObjId, IndirectObject>,
    object_stream: ObjId,
) -> Result<Vec<(u32, Primitive)>, RenderError> {
    let stream_object = if let Some(object) = objects.get(&object_stream) {
        object
    } else if let Some((_, object)) = objects
        .iter()
        .find(|((object_id, _generation), _)| *object_id == object_stream.0)
    {
        object
    } else {
        return Err(RenderError(format!(
            "Compressed object stream {:?} was not found in PDF objects",
            object_stream
        )));
    };

    let (dict, raw_data) = match stream_object {
        IndirectObject::Stream { dict, data } => (dict, data),
        _ => {
            return Err(RenderError(format!(
                "Object {:?} is not a stream object",
                object_stream
            )))
        }
    };

    let stream_type = dict.get("Type").and_then(as_name).unwrap_or("");
    if stream_type != "ObjStm" {
        return Err(RenderError(format!(
            "Object {:?} is not an /ObjStm stream",
            object_stream
        )));
    }

    let n = resolve_dictionary_usize(dict, "N", objects)?;
    let first = resolve_dictionary_usize(dict, "First", objects)?;

    let decoded = decode_stream_data(dict, raw_data)?;
    if first > decoded.len() {
        return Err(RenderError(format!(
            "Object stream {:?} has invalid /First offset {}",
            object_stream, first
        )));
    }

    let mut header_parser = Parser::new(&decoded, 0)?;
    let mut header = Vec::with_capacity(n);
    for _ in 0..n {
        let object_number = header_parser.parse_u32_token()?;
        let relative_offset = header_parser.parse_usize_token()?;
        header.push((object_number, relative_offset));
    }

    let mut entries = Vec::with_capacity(header.len());
    for (index, (object_number, relative_offset)) in header.iter().enumerate() {
        let start = first + relative_offset;
        let end = if index + 1 < header.len() {
            first + header[index + 1].1
        } else {
            decoded.len()
        };
        if start > decoded.len() || end > decoded.len() || start > end {
            return Err(RenderError(format!(
                "Object stream {:?} has invalid object slice bounds",
                object_stream
            )));
        }

        let mut parser = Parser::new(&decoded[start..end], 0)?;
        parser.skip_ws_and_comments();
        let primitive = parser.parse_primitive()?;
        entries.push((*object_number, primitive));
    }

    Ok(entries)
}

fn collect_pages(
    objects: &BTreeMap<ObjId, IndirectObject>,
    node: ObjId,
    visited: &mut HashSet<ObjId>,
    out: &mut Vec<ObjId>,
) -> Result<(), RenderError> {
    if !visited.insert(node) {
        return Ok(());
    }

    let dict = object_dict(objects, node).ok_or_else(|| {
        RenderError(format!("Page tree node {:?} is missing or malformed", node))
    })?;

    let node_type = dict.get("Type").and_then(as_name).unwrap_or("");
    if node_type == "Page" {
        out.push(node);
        return Ok(());
    }

    let kids = dict.get("Kids").and_then(as_array).ok_or_else(|| {
        RenderError(format!("Page tree node {:?} is missing /Kids", node))
    })?;
    for kid in kids {
        let kid_ref = as_reference(kid).ok_or_else(|| {
            RenderError(format!("Page tree node {:?} has non-reference kid", node))
        })?;
        collect_pages(objects, kid_ref, visited, out)?;
    }

    Ok(())
}

fn object_dict<'a>(objects: &'a BTreeMap<ObjId, IndirectObject>, id: ObjId) -> Option<&'a Dict> {
    let object = objects.get(&id)?;
    match object {
        IndirectObject::Primitive(Primitive::Dictionary(dict)) => Some(dict),
        IndirectObject::Stream { dict, .. } => Some(dict),
        _ => None,
    }
}

fn as_reference(value: &Primitive) -> Option<ObjId> {
    match value {
        Primitive::Reference(reference) => Some(*reference),
        _ => None,
    }
}

fn as_name(value: &Primitive) -> Option<&str> {
    match value {
        Primitive::Name(name) => Some(name.as_str()),
        _ => None,
    }
}

fn as_array(value: &Primitive) -> Option<&[Primitive]> {
    match value {
        Primitive::Array(values) => Some(values.as_slice()),
        _ => None,
    }
}

fn as_integer(value: &Primitive) -> Option<i64> {
    match value {
        Primitive::Integer(number) => Some(*number),
        _ => None,
    }
}

fn parse_xref_chain(
    data: &[u8],
    latest_xref_offset: usize,
) -> Result<(HashMap<ObjId, XRefEntry>, Dict), RenderError> {
    let mut visited = HashSet::new();
    let mut sections = Vec::new();
    let mut current_offset = latest_xref_offset;

    loop {
        if !visited.insert(current_offset) {
            return Err(RenderError("Detected loop in xref /Prev chain".to_string()));
        }

        let section = parse_xref_section(data, current_offset)?;
        current_offset = match section.prev {
            Some(prev) => prev,
            None => {
                sections.push(section);
                break;
            }
        };
        sections.push(section);
    }

    sections.reverse();

    let mut all_entries: HashMap<ObjId, XRefEntry> = HashMap::new();
    let mut merged_trailer = Dict::new();
    for section in &sections {
        all_entries.extend(section.entries.iter().map(|(id, entry)| (*id, *entry)));
        merged_trailer.extend(section.trailer.clone());
    }

    Ok((all_entries, merged_trailer))
}

struct XrefSection {
    entries: HashMap<ObjId, XRefEntry>,
    trailer: Dict,
    prev: Option<usize>,
}

fn parse_xref_section(data: &[u8], offset: usize) -> Result<XrefSection, RenderError> {
    let mut parser = Parser::new(data, offset)?;
    parser.skip_ws_and_comments();

    if parser.consume_keyword("xref")? {
        return parse_classic_xref_section(&mut parser);
    }

    parse_xref_stream_section(data, offset)
}

fn parse_classic_xref_section(parser: &mut Parser<'_>) -> Result<XrefSection, RenderError> {
    let mut entries = HashMap::new();

    loop {
        parser.skip_ws_and_comments();
        if parser.consume_keyword("trailer")? {
            break;
        }

        let first_obj = parser.parse_u32_token()?;
        let count = parser.parse_u32_token()?;
        for index in 0..count {
            let object_offset = parser.parse_usize_token()?;
            let generation = parser.parse_u16_token()?;
            let entry_flag = parser.parse_token_string()?;
            if entry_flag == "n" {
                entries.insert((first_obj + index, generation), XRefEntry::InUse { offset: object_offset });
            }
        }
    }

    let trailer = match parser.parse_primitive()? {
        Primitive::Dictionary(dict) => dict,
        _ => {
            return Err(RenderError(
                "Malformed PDF trailer: expected dictionary".to_string(),
            ))
        }
    };
    let prev = trailer
        .get("Prev")
        .and_then(as_integer)
        .and_then(|value| if value >= 0 { Some(value as usize) } else { None });

    Ok(XrefSection {
        entries,
        trailer,
        prev,
    })
}

fn parse_xref_stream_section(data: &[u8], offset: usize) -> Result<XrefSection, RenderError> {
    let parsed_objects = BTreeMap::new();
    let (_, object) = parse_indirect_object(data, offset, &parsed_objects)?;
    let (dict, raw_data) = match object {
        IndirectObject::Stream { dict, data } => (dict, data),
        _ => {
            return Err(RenderError(
                "Malformed xref section: expected xref stream object".to_string(),
            ))
        }
    };

    if dict.get("Type").and_then(as_name) != Some("XRef") {
        return Err(RenderError(
            "Malformed xref section: stream at startxref is not /Type /XRef".to_string(),
        ));
    }

    let decoded = decode_stream_data(&dict, &raw_data)?;
    let entries = parse_xref_stream_entries(&dict, &decoded)?;

    let prev = dict
        .get("Prev")
        .and_then(as_integer)
        .and_then(|value| if value >= 0 { Some(value as usize) } else { None });

    Ok(XrefSection {
        entries,
        trailer: dict,
        prev,
    })
}

fn parse_xref_stream_entries(
    dict: &Dict,
    decoded: &[u8],
) -> Result<HashMap<ObjId, XRefEntry>, RenderError> {
    let widths = dict
        .get("W")
        .and_then(as_array)
        .ok_or_else(|| RenderError("XRef stream missing /W array".to_string()))?;
    if widths.len() != 3 {
        return Err(RenderError("XRef stream /W must have 3 integers".to_string()));
    }

    let w0 = primitive_to_usize(&widths[0])?;
    let w1 = primitive_to_usize(&widths[1])?;
    let w2 = primitive_to_usize(&widths[2])?;
    let entry_size = w0 + w1 + w2;
    if entry_size == 0 {
        return Err(RenderError("XRef stream has invalid /W entry widths".to_string()));
    }

    let mut index_pairs = Vec::<(u32, u32)>::new();
    if let Some(index_array) = dict.get("Index").and_then(as_array) {
        if index_array.len() % 2 != 0 {
            return Err(RenderError("XRef stream /Index must have even length".to_string()));
        }
        for pair in index_array.chunks(2) {
            let first = primitive_to_u32(&pair[0])?;
            let count = primitive_to_u32(&pair[1])?;
            index_pairs.push((first, count));
        }
    } else {
        let size = dict
            .get("Size")
            .map(primitive_to_u32)
            .transpose()?
            .ok_or_else(|| RenderError("XRef stream missing /Size".to_string()))?;
        index_pairs.push((0, size));
    }

    let mut position = 0usize;
    let mut entries = HashMap::new();

    for (first_obj, count) in index_pairs {
        for item_index in 0..count {
            if position + entry_size > decoded.len() {
                return Err(RenderError(
                    "XRef stream data shorter than expected".to_string(),
                ));
            }

            let field0 = read_be_u64(&decoded[position..position + w0])?;
            let field1 = read_be_u64(&decoded[position + w0..position + w0 + w1])?;
            let field2 = read_be_u64(&decoded[position + w0 + w1..position + entry_size])?;
            position += entry_size;

            let object_number = first_obj + item_index;
            let kind = if w0 == 0 { 1 } else { field0 as u32 };
            match kind {
                0 => {}
                1 => {
                    let generation = field2 as u16;
                    entries.insert(
                        (object_number, generation),
                        XRefEntry::InUse {
                            offset: field1 as usize,
                        },
                    );
                }
                2 => {
                    entries.insert(
                        (object_number, 0),
                        XRefEntry::Compressed {
                            object_stream: (field1 as u32, 0),
                            index: field2 as u32,
                        },
                    );
                }
                _ => {}
            }
        }
    }

    Ok(entries)
}

fn parse_indirect_object(
    data: &[u8],
    offset: usize,
    parsed_objects: &BTreeMap<ObjId, IndirectObject>,
) -> Result<(ObjId, IndirectObject), RenderError> {
    let mut parser = Parser::new(data, offset)?;
    parser.skip_ws_and_comments();

    let object_number = parser.parse_u32_token()?;
    let generation = parser.parse_u16_token()?;
    parser.expect_keyword("obj")?;

    let value = parser.parse_primitive()?;
    let object = match value {
        Primitive::Dictionary(dict) => {
            if parser.consume_keyword("stream")? {
                parser.consume_single_eol();

                let data = if let Some(length) = resolve_stream_length(&dict, parsed_objects) {
                    parser.read_stream_by_length(length)?
                } else {
                    parser.read_stream_until_endstream()?
                };
                parser.expect_keyword("endstream")?;
                IndirectObject::Stream { dict, data }
            } else {
                IndirectObject::Primitive(Primitive::Dictionary(dict))
            }
        }
        other => IndirectObject::Primitive(other),
    };

    parser.expect_keyword("endobj")?;
    Ok(((object_number, generation), object))
}

fn resolve_stream_length(
    dict: &Dict,
    parsed_objects: &BTreeMap<ObjId, IndirectObject>,
) -> Option<usize> {
    let length_primitive = dict.get("Length")?;
    match length_primitive {
        Primitive::Integer(length) if *length >= 0 => Some(*length as usize),
        Primitive::Reference(reference) => {
            let object = parsed_objects.get(reference)?;
            match object {
                IndirectObject::Primitive(Primitive::Integer(length)) if *length >= 0 => {
                    Some(*length as usize)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn resolve_dictionary_usize(
    dict: &Dict,
    key: &str,
    parsed_objects: &BTreeMap<ObjId, IndirectObject>,
) -> Result<usize, RenderError> {
    let value = dict
        .get(key)
        .ok_or_else(|| RenderError(format!("Stream dictionary missing /{}", key)))?;
    resolve_primitive_usize(value, parsed_objects).ok_or_else(|| {
        RenderError(format!(
            "Stream dictionary /{} must be a non-negative integer",
            key
        ))
    })
}

fn resolve_primitive_usize(
    primitive: &Primitive,
    parsed_objects: &BTreeMap<ObjId, IndirectObject>,
) -> Option<usize> {
    match primitive {
        Primitive::Integer(value) if *value >= 0 => Some(*value as usize),
        Primitive::Reference(reference) => {
            let object = parsed_objects.get(reference)?;
            match object {
                IndirectObject::Primitive(Primitive::Integer(value)) if *value >= 0 => {
                    Some(*value as usize)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn primitive_to_u32(value: &Primitive) -> Result<u32, RenderError> {
    match value {
        Primitive::Integer(number) if *number >= 0 => u32::try_from(*number)
            .map_err(|_| RenderError("XRef integer exceeds u32 range".to_string())),
        _ => Err(RenderError(
            "XRef array value must be a non-negative integer".to_string(),
        )),
    }
}

fn primitive_to_usize(value: &Primitive) -> Result<usize, RenderError> {
    match value {
        Primitive::Integer(number) if *number >= 0 => usize::try_from(*number)
            .map_err(|_| RenderError("XRef integer exceeds usize range".to_string())),
        _ => Err(RenderError(
            "XRef array value must be a non-negative integer".to_string(),
        )),
    }
}

fn read_be_u64(bytes: &[u8]) -> Result<u64, RenderError> {
    if bytes.len() > 8 {
        return Err(RenderError("XRef field width is too large".to_string()));
    }
    let mut value = 0u64;
    for byte in bytes {
        value = (value << 8) | u64::from(*byte);
    }
    Ok(value)
}

fn decode_stream_data(dict: &Dict, raw_data: &[u8]) -> Result<Vec<u8>, RenderError> {
    let filters = extract_stream_filters(dict)?;
    if filters.is_empty() {
        return Ok(raw_data.to_vec());
    }

    let decode_params = extract_decode_params(dict, filters.len())?;
    let mut data = raw_data.to_vec();
    for (index, filter) in filters.iter().enumerate() {
        let params = decode_params.get(index).and_then(|entry| entry.as_ref());
        data = match filter.as_str() {
            "FlateDecode" => decode_flate(&data, params)?,
            "ASCIIHexDecode" => decode_ascii_hex(&data)?,
            "ASCII85Decode" => {
                return Err(RenderError(
                    "Unsupported stream filter: ASCII85Decode".to_string(),
                ))
            }
            other => {
                return Err(RenderError(format!(
                    "Unsupported stream filter: {}",
                    other
                )))
            }
        };
    }

    Ok(data)
}

fn extract_decode_params(
    dict: &Dict,
    filter_count: usize,
) -> Result<Vec<Option<Dict>>, RenderError> {
    let mut params = vec![None; filter_count];
    let Some(decode_params) = dict.get("DecodeParms") else {
        return Ok(params);
    };

    match decode_params {
        Primitive::Null => Ok(params),
        Primitive::Dictionary(value) => {
            if !params.is_empty() {
                params[0] = Some(value.clone());
            }
            Ok(params)
        }
        Primitive::Array(values) => {
            if values.len() != filter_count {
                return Err(RenderError(format!(
                    "Stream /DecodeParms length ({}) does not match /Filter length ({})",
                    values.len(),
                    filter_count
                )));
            }

            for (index, value) in values.iter().enumerate() {
                match value {
                    Primitive::Null => {}
                    Primitive::Dictionary(value) => params[index] = Some(value.clone()),
                    _ => {
                        return Err(RenderError(
                            "Stream /DecodeParms array entries must be dictionaries or null"
                                .to_string(),
                        ))
                    }
                }
            }

            Ok(params)
        }
        _ => Err(RenderError(
            "Stream /DecodeParms must be a dictionary, array, or null".to_string(),
        )),
    }
}

fn extract_stream_filters(dict: &Dict) -> Result<Vec<String>, RenderError> {
    let Some(filter) = dict.get("Filter") else {
        return Ok(Vec::new());
    };

    match filter {
        Primitive::Name(name) => Ok(vec![name.clone()]),
        Primitive::Array(values) => {
            let mut filters = Vec::with_capacity(values.len());
            for value in values {
                match value {
                    Primitive::Name(name) => filters.push(name.clone()),
                    _ => {
                        return Err(RenderError(
                            "Stream /Filter array must contain PDF names".to_string(),
                        ))
                    }
                }
            }
            Ok(filters)
        }
        _ => Err(RenderError(
            "Stream /Filter must be a name or an array of names".to_string(),
        )),
    }
}

fn decode_flate(data: &[u8], params: Option<&Dict>) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    let mut zlib = ZlibDecoder::new(data);
    match zlib.read_to_end(&mut out) {
        Ok(_) => apply_flate_decode_params(&out, params),
        Err(_) => {
            let mut out = Vec::new();
            let mut deflate = DeflateDecoder::new(data);
            deflate
                .read_to_end(&mut out)
                .map_err(|e| RenderError(format!("FlateDecode failed: {}", e)))?;
            apply_flate_decode_params(&out, params)
        }
    }
}

fn apply_flate_decode_params(data: &[u8], params: Option<&Dict>) -> Result<Vec<u8>, RenderError> {
    let predictor = decode_param_usize(params, "Predictor", 1)?;
    if predictor <= 1 {
        return Ok(data.to_vec());
    }

    let colors = decode_param_usize(params, "Colors", 1)?;
    let bits_per_component = decode_param_usize(params, "BitsPerComponent", 8)?;
    let columns = decode_param_usize(params, "Columns", 1)?;

    if colors == 0 || bits_per_component == 0 || columns == 0 {
        return Err(RenderError(
            "Flate DecodeParms must use positive Colors, BitsPerComponent, and Columns values"
                .to_string(),
        ));
    }

    match predictor {
        2 => decode_tiff_predictor(data, colors, bits_per_component, columns),
        10..=15 => decode_png_predictor(data, predictor, colors, bits_per_component, columns),
        _ => Err(RenderError(format!(
            "Unsupported FlateDecode predictor {}",
            predictor
        ))),
    }
}

fn decode_param_usize(
    params: Option<&Dict>,
    key: &str,
    default: usize,
) -> Result<usize, RenderError> {
    let Some(params) = params else {
        return Ok(default);
    };
    let Some(value) = params.get(key) else {
        return Ok(default);
    };

    match value {
        Primitive::Integer(number) if *number >= 0 => usize::try_from(*number)
            .map_err(|_| RenderError(format!("DecodeParms /{} exceeds usize range", key))),
        Primitive::Integer(_) => Err(RenderError(format!(
            "DecodeParms /{} must be non-negative",
            key
        ))),
        _ => Err(RenderError(format!(
            "DecodeParms /{} must be an integer",
            key
        ))),
    }
}

fn decode_tiff_predictor(
    data: &[u8],
    colors: usize,
    bits_per_component: usize,
    columns: usize,
) -> Result<Vec<u8>, RenderError> {
    if bits_per_component != 8 {
        return Err(RenderError(format!(
            "Unsupported TIFF predictor BitsPerComponent {} (only 8 is supported)",
            bits_per_component
        )));
    }

    let bytes_per_pixel = colors;
    let row_len = columns
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| RenderError("TIFF predictor row length overflow".to_string()))?;
    if row_len == 0 {
        return Err(RenderError("TIFF predictor row length is zero".to_string()));
    }
    if data.len() % row_len != 0 {
        return Err(RenderError(format!(
            "TIFF predictor data length {} is not divisible by row length {}",
            data.len(),
            row_len
        )));
    }

    let mut decoded = data.to_vec();
    let mut row_start = 0usize;
    while row_start < decoded.len() {
        for byte_index in bytes_per_pixel..row_len {
            let current = row_start + byte_index;
            let left = current - bytes_per_pixel;
            decoded[current] = decoded[current].wrapping_add(decoded[left]);
        }
        row_start += row_len;
    }
    Ok(decoded)
}

fn decode_png_predictor(
    data: &[u8],
    predictor: usize,
    colors: usize,
    bits_per_component: usize,
    columns: usize,
) -> Result<Vec<u8>, RenderError> {
    if bits_per_component % 8 != 0 {
        return Err(RenderError(format!(
            "Unsupported PNG predictor BitsPerComponent {} (must be a multiple of 8)",
            bits_per_component
        )));
    }

    let bytes_per_component = bits_per_component / 8;
    let bytes_per_pixel = colors
        .checked_mul(bytes_per_component)
        .ok_or_else(|| RenderError("PNG predictor bytes-per-pixel overflow".to_string()))?;
    if bytes_per_pixel == 0 {
        return Err(RenderError(
            "PNG predictor bytes-per-pixel cannot be zero".to_string(),
        ));
    }
    let row_len = columns
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| RenderError("PNG predictor row length overflow".to_string()))?;
    if row_len == 0 {
        return Err(RenderError("PNG predictor row length is zero".to_string()));
    }

    if predictor == 15 {
        return decode_png_rows_with_prefix(data, row_len, bytes_per_pixel);
    }

    let fixed_filter = u8::try_from(predictor - 10)
        .map_err(|_| RenderError(format!("Unsupported PNG predictor {}", predictor)))?;

    decode_png_rows_with_prefix(data, row_len, bytes_per_pixel)
        .or_else(|_| decode_png_rows_without_prefix(data, row_len, bytes_per_pixel, fixed_filter))
}

fn decode_png_rows_with_prefix(
    data: &[u8],
    row_len: usize,
    bytes_per_pixel: usize,
) -> Result<Vec<u8>, RenderError> {
    let encoded_row_len = row_len
        .checked_add(1)
        .ok_or_else(|| RenderError("PNG predictor encoded row length overflow".to_string()))?;
    if data.len() % encoded_row_len != 0 {
        return Err(RenderError(format!(
            "PNG predictor data length {} is not divisible by encoded row length {}",
            data.len(),
            encoded_row_len
        )));
    }

    let row_count = data.len() / encoded_row_len;
    let mut output = Vec::with_capacity(row_count * row_len);
    let mut previous_row = vec![0u8; row_len];
    let mut cursor = 0usize;

    for _ in 0..row_count {
        let filter = data[cursor];
        cursor += 1;
        if filter > 4 {
            return Err(RenderError(format!(
                "PNG predictor row uses unsupported filter byte {}",
                filter
            )));
        }

        let row_data = &data[cursor..cursor + row_len];
        cursor += row_len;
        let decoded = decode_png_row(row_data, filter, &previous_row, bytes_per_pixel);
        previous_row = decoded.clone();
        output.extend_from_slice(&decoded);
    }

    Ok(output)
}

fn decode_png_rows_without_prefix(
    data: &[u8],
    row_len: usize,
    bytes_per_pixel: usize,
    filter: u8,
) -> Result<Vec<u8>, RenderError> {
    if filter > 4 {
        return Err(RenderError(format!(
            "PNG predictor uses unsupported fixed filter {}",
            filter
        )));
    }
    if data.len() % row_len != 0 {
        return Err(RenderError(format!(
            "PNG predictor data length {} is not divisible by row length {}",
            data.len(),
            row_len
        )));
    }

    let row_count = data.len() / row_len;
    let mut output = Vec::with_capacity(row_count * row_len);
    let mut previous_row = vec![0u8; row_len];
    let mut cursor = 0usize;

    for _ in 0..row_count {
        let row_data = &data[cursor..cursor + row_len];
        cursor += row_len;
        let decoded = decode_png_row(row_data, filter, &previous_row, bytes_per_pixel);
        previous_row = decoded.clone();
        output.extend_from_slice(&decoded);
    }

    Ok(output)
}

fn decode_png_row(row_data: &[u8], filter: u8, previous_row: &[u8], bytes_per_pixel: usize) -> Vec<u8> {
    let mut decoded = vec![0u8; row_data.len()];
    for index in 0..row_data.len() {
        let left = if index >= bytes_per_pixel {
            decoded[index - bytes_per_pixel]
        } else {
            0
        };
        let up = previous_row.get(index).copied().unwrap_or(0);
        let up_left = if index >= bytes_per_pixel {
            previous_row
                .get(index - bytes_per_pixel)
                .copied()
                .unwrap_or(0)
        } else {
            0
        };

        decoded[index] = match filter {
            0 => row_data[index],
            1 => row_data[index].wrapping_add(left),
            2 => row_data[index].wrapping_add(up),
            3 => row_data[index].wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
            4 => row_data[index].wrapping_add(paeth_predictor(left, up, up_left)),
            _ => row_data[index],
        };
    }
    decoded
}

fn paeth_predictor(left: u8, up: u8, up_left: u8) -> u8 {
    let left_i = i32::from(left);
    let up_i = i32::from(up);
    let up_left_i = i32::from(up_left);
    let prediction = left_i + up_i - up_left_i;

    let left_distance = (prediction - left_i).abs();
    let up_distance = (prediction - up_i).abs();
    let up_left_distance = (prediction - up_left_i).abs();

    if left_distance <= up_distance && left_distance <= up_left_distance {
        left
    } else if up_distance <= up_left_distance {
        up
    } else {
        up_left
    }
}

fn decode_ascii_hex(data: &[u8]) -> Result<Vec<u8>, RenderError> {
    let mut hex = Vec::new();
    for byte in data {
        if *byte == b'>' {
            break;
        }
        if is_whitespace(*byte) {
            continue;
        }
        hex.push(*byte);
    }

    if hex.is_empty() {
        return Ok(Vec::new());
    }

    if hex.len() % 2 == 1 {
        hex.push(b'0');
    }

    let mut out = Vec::with_capacity(hex.len() / 2);
    for pair in hex.chunks(2) {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        out.push((high << 4) | low);
    }
    Ok(out)
}

fn find_startxref(data: &[u8]) -> Result<usize, RenderError> {
    let marker = b"startxref";
    let index = find_subslice_from_end(data, marker).ok_or_else(|| {
        RenderError("Invalid PDF: startxref marker not found".to_string())
    })?;

    let mut parser = Parser::new(data, index + marker.len())?;
    parser.skip_ws_and_comments();
    parser.parse_usize_token()
}

fn find_subslice_from_end(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .rev()
        .find(|index| &haystack[*index..*index + needle.len()] == needle)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8], pos: usize) -> Result<Self, RenderError> {
        if pos > bytes.len() {
            return Err(RenderError("Parser offset out of bounds".to_string()));
        }
        Ok(Self { bytes, pos })
    }

    fn skip_ws_and_comments(&mut self) {
        while self.pos < self.bytes.len() {
            let byte = self.bytes[self.pos];
            if is_whitespace(byte) {
                self.pos += 1;
                continue;
            }
            if byte == b'%' {
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn parse_token_string(&mut self) -> Result<String, RenderError> {
        let token = self.parse_regular_token()?;
        String::from_utf8(token).map_err(|_| RenderError("Invalid UTF-8 token".to_string()))
    }

    fn parse_regular_token(&mut self) -> Result<Vec<u8>, RenderError> {
        self.skip_ws_and_comments();
        if self.pos >= self.bytes.len() {
            return Err(RenderError("Unexpected end of PDF while reading token".to_string()));
        }
        if is_delimiter(self.bytes[self.pos]) {
            return Err(RenderError("Unexpected delimiter while reading token".to_string()));
        }
        let start = self.pos;
        while self.pos < self.bytes.len()
            && !is_whitespace(self.bytes[self.pos])
            && !is_delimiter(self.bytes[self.pos])
        {
            self.pos += 1;
        }
        Ok(self.bytes[start..self.pos].to_vec())
    }

    fn consume_keyword(&mut self, keyword: &str) -> Result<bool, RenderError> {
        let saved = self.pos;
        let token = match self.parse_regular_token() {
            Ok(token) => token,
            Err(_) => {
                self.pos = saved;
                return Ok(false);
            }
        };
        if token == keyword.as_bytes() {
            Ok(true)
        } else {
            self.pos = saved;
            Ok(false)
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), RenderError> {
        if self.consume_keyword(keyword)? {
            Ok(())
        } else {
            Err(RenderError(format!("Expected keyword '{}'", keyword)))
        }
    }

    fn parse_u32_token(&mut self) -> Result<u32, RenderError> {
        let token = self.parse_token_string()?;
        token
            .parse::<u32>()
            .map_err(|_| RenderError(format!("Expected unsigned integer token, got '{}'", token)))
    }

    fn parse_u16_token(&mut self) -> Result<u16, RenderError> {
        let token = self.parse_token_string()?;
        token
            .parse::<u16>()
            .map_err(|_| RenderError(format!("Expected unsigned short token, got '{}'", token)))
    }

    fn parse_usize_token(&mut self) -> Result<usize, RenderError> {
        let token = self.parse_token_string()?;
        token
            .parse::<usize>()
            .map_err(|_| RenderError(format!("Expected usize token, got '{}'", token)))
    }

    fn parse_primitive(&mut self) -> Result<Primitive, RenderError> {
        self.skip_ws_and_comments();
        if self.pos >= self.bytes.len() {
            return Err(RenderError("Unexpected end of PDF while parsing object".to_string()));
        }

        match self.bytes[self.pos] {
            b'/' => self.parse_name().map(Primitive::Name),
            b'(' => self.parse_literal_string().map(Primitive::StringLiteral),
            b'[' => self.parse_array(),
            b'<' => {
                if self.peek_next() == Some(b'<') {
                    self.parse_dictionary().map(Primitive::Dictionary)
                } else {
                    self.parse_hex_string().map(Primitive::HexString)
                }
            }
            _ => self.parse_atomic(),
        }
    }

    fn parse_atomic(&mut self) -> Result<Primitive, RenderError> {
        let token = self.parse_token_string()?;
        match token.as_str() {
            "null" => Ok(Primitive::Null),
            "true" => Ok(Primitive::Boolean(true)),
            "false" => Ok(Primitive::Boolean(false)),
            _ => {
                if let Ok(int_value) = token.parse::<i64>() {
                    let saved = self.pos;
                    self.skip_ws_and_comments();
                    if let Ok(second_token) = self.parse_token_string() {
                        if let Ok(generation) = second_token.parse::<u16>() {
                            self.skip_ws_and_comments();
                            if self.consume_keyword("R")? {
                                if int_value < 0 {
                                    return Err(RenderError(
                                        "Invalid negative object reference".to_string(),
                                    ));
                                }
                                return Ok(Primitive::Reference((int_value as u32, generation)));
                            }
                        }
                    }
                    self.pos = saved;
                    Ok(Primitive::Integer(int_value))
                } else if let Ok(real_value) = token.parse::<f64>() {
                    Ok(Primitive::Real(real_value))
                } else {
                    Err(RenderError(format!("Unexpected PDF token '{}'", token)))
                }
            }
        }
    }

    fn parse_name(&mut self) -> Result<String, RenderError> {
        if self.bytes.get(self.pos) != Some(&b'/') {
            return Err(RenderError("Expected PDF name".to_string()));
        }
        self.pos += 1;
        let start = self.pos;
        while self.pos < self.bytes.len()
            && !is_whitespace(self.bytes[self.pos])
            && !is_delimiter(self.bytes[self.pos])
        {
            self.pos += 1;
        }
        let raw = &self.bytes[start..self.pos];
        decode_pdf_name(raw)
    }

    fn parse_literal_string(&mut self) -> Result<Vec<u8>, RenderError> {
        if self.bytes.get(self.pos) != Some(&b'(') {
            return Err(RenderError("Expected literal string".to_string()));
        }
        self.pos += 1;

        let mut depth = 1usize;
        let mut output = Vec::new();
        while self.pos < self.bytes.len() {
            let byte = self.bytes[self.pos];
            self.pos += 1;

            if byte == b'\\' {
                if self.pos >= self.bytes.len() {
                    break;
                }
                let escaped = self.bytes[self.pos];
                self.pos += 1;
                match escaped {
                    b'n' => output.push(b'\n'),
                    b'r' => output.push(b'\r'),
                    b't' => output.push(b'\t'),
                    b'b' => output.push(0x08),
                    b'f' => output.push(0x0C),
                    b'(' => output.push(b'('),
                    b')' => output.push(b')'),
                    b'\\' => output.push(b'\\'),
                    b'\n' => {}
                    b'\r' => {
                        if self.bytes.get(self.pos) == Some(&b'\n') {
                            self.pos += 1;
                        }
                    }
                    b'0'..=b'7' => {
                        let mut octal = (escaped - b'0') as u16;
                        for _ in 0..2 {
                            if let Some(next) = self.bytes.get(self.pos) {
                                if (b'0'..=b'7').contains(next) {
                                    octal = octal * 8 + (next - b'0') as u16;
                                    self.pos += 1;
                                } else {
                                    break;
                                }
                            }
                        }
                        output.push((octal & 0xFF) as u8);
                    }
                    other => output.push(other),
                }
                continue;
            }

            if byte == b'(' {
                depth += 1;
                output.push(byte);
                continue;
            }
            if byte == b')' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(output);
                }
                output.push(byte);
                continue;
            }

            output.push(byte);
        }

        Err(RenderError("Unterminated literal string in PDF".to_string()))
    }

    fn parse_hex_string(&mut self) -> Result<Vec<u8>, RenderError> {
        if self.bytes.get(self.pos) != Some(&b'<') {
            return Err(RenderError("Expected hex string".to_string()));
        }
        self.pos += 1;

        let mut hex = Vec::new();
        while self.pos < self.bytes.len() {
            let byte = self.bytes[self.pos];
            self.pos += 1;
            if byte == b'>' {
                break;
            }
            if is_whitespace(byte) {
                continue;
            }
            hex.push(byte);
        }

        if hex.is_empty() {
            return Ok(Vec::new());
        }
        if hex.len() % 2 == 1 {
            hex.push(b'0');
        }

        let mut output = Vec::with_capacity(hex.len() / 2);
        let mut index = 0usize;
        while index < hex.len() {
            let high = hex_nibble(hex[index])?;
            let low = hex_nibble(hex[index + 1])?;
            output.push((high << 4) | low);
            index += 2;
        }

        Ok(output)
    }

    fn parse_array(&mut self) -> Result<Primitive, RenderError> {
        if self.bytes.get(self.pos) != Some(&b'[') {
            return Err(RenderError("Expected array".to_string()));
        }
        self.pos += 1;

        let mut items = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.bytes.get(self.pos) == Some(&b']') {
                self.pos += 1;
                break;
            }
            items.push(self.parse_primitive()?);
        }

        Ok(Primitive::Array(items))
    }

    fn parse_dictionary(&mut self) -> Result<Dict, RenderError> {
        if !(self.bytes.get(self.pos) == Some(&b'<') && self.peek_next() == Some(b'<')) {
            return Err(RenderError("Expected dictionary".to_string()));
        }
        self.pos += 2;

        let mut dict = Dict::new();
        loop {
            self.skip_ws_and_comments();
            if self.bytes.get(self.pos) == Some(&b'>') && self.peek_next() == Some(b'>') {
                self.pos += 2;
                break;
            }

            let key = self.parse_name()?;
            let value = self.parse_primitive()?;
            dict.insert(key, value);
        }
        Ok(dict)
    }

    fn consume_single_eol(&mut self) {
        if self.bytes.get(self.pos) == Some(&b'\r') {
            self.pos += 1;
            if self.bytes.get(self.pos) == Some(&b'\n') {
                self.pos += 1;
            }
            return;
        }
        if self.bytes.get(self.pos) == Some(&b'\n') {
            self.pos += 1;
        }
    }

    fn read_stream_by_length(&mut self, length: usize) -> Result<Vec<u8>, RenderError> {
        if self.pos + length > self.bytes.len() {
            return Err(RenderError(
                "Stream length exceeds source PDF size".to_string(),
            ));
        }
        let data = self.bytes[self.pos..self.pos + length].to_vec();
        self.pos += length;
        self.skip_ws_and_comments();
        Ok(data)
    }

    fn read_stream_until_endstream(&mut self) -> Result<Vec<u8>, RenderError> {
        let marker = b"endstream";
        let mut marker_index = None;
        let mut index = self.pos;
        while index + marker.len() <= self.bytes.len() {
            if &self.bytes[index..index + marker.len()] == marker {
                marker_index = Some(index);
                break;
            }
            index += 1;
        }

        let marker_index = marker_index.ok_or_else(|| {
            RenderError("Could not find endstream marker".to_string())
        })?;

        let mut end = marker_index;
        if end >= 2 && self.bytes[end - 2] == b'\r' && self.bytes[end - 1] == b'\n' {
            end -= 2;
        } else if end >= 1 && (self.bytes[end - 1] == b'\n' || self.bytes[end - 1] == b'\r') {
            end -= 1;
        }

        let data = self.bytes[self.pos..end].to_vec();
        self.pos = marker_index;
        Ok(data)
    }

    fn peek_next(&self) -> Option<u8> {
        self.bytes.get(self.pos + 1).copied()
    }
}

fn decode_pdf_name(raw: &[u8]) -> Result<String, RenderError> {
    let mut out = Vec::with_capacity(raw.len());
    let mut index = 0usize;
    while index < raw.len() {
        if raw[index] == b'#' {
            if index + 2 >= raw.len() {
                return Err(RenderError("Invalid name hex escape".to_string()));
            }
            let high = hex_nibble(raw[index + 1])?;
            let low = hex_nibble(raw[index + 2])?;
            out.push((high << 4) | low);
            index += 3;
        } else {
            out.push(raw[index]);
            index += 1;
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

fn hex_nibble(byte: u8) -> Result<u8, RenderError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(RenderError(format!("Invalid hex digit '{}'", byte as char))),
    }
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b'\x00' | b'\x09' | b'\x0A' | b'\x0C' | b'\x0D' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn write_pdf(objects: &BTreeMap<ObjId, IndirectObject>, root: ObjId) -> Result<Vec<u8>, RenderError> {
    let mut output = Vec::new();
    output.extend_from_slice(b"%PDF-1.7\n");
    output.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

    let mut offsets = BTreeMap::<u32, usize>::new();
    let mut max_id = 0u32;

    for ((object_id, generation), object) in objects {
        if *generation != 0 {
            return Err(RenderError(
                "Only generation 0 objects are supported in merged output".to_string(),
            ));
        }
        max_id = max_id.max(*object_id);
        offsets.insert(*object_id, output.len());
        output.extend_from_slice(format!("{} 0 obj\n", object_id).as_bytes());
        write_indirect_object(&mut output, object);
        output.extend_from_slice(b"\nendobj\n");
    }

    let xref_offset = output.len();
    output.extend_from_slice(format!("xref\n0 {}\n", max_id + 1).as_bytes());
    output.extend_from_slice(b"0000000000 65535 f \n");
    for object_id in 1..=max_id {
        if let Some(offset) = offsets.get(&object_id) {
            output.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
        } else {
            output.extend_from_slice(b"0000000000 65535 f \n");
        }
    }

    output.extend_from_slice(b"trailer\n<<\n");
    output.extend_from_slice(format!("  /Size {}\n", max_id + 1).as_bytes());
    output.extend_from_slice(format!("  /Root {} 0 R\n", root.0).as_bytes());
    output.extend_from_slice(b">>\n");
    output.extend_from_slice(b"startxref\n");
    output.extend_from_slice(format!("{}\n", xref_offset).as_bytes());
    output.extend_from_slice(b"%%EOF\n");

    Ok(output)
}

fn write_indirect_object(output: &mut Vec<u8>, object: &IndirectObject) {
    match object {
        IndirectObject::Primitive(primitive) => write_primitive(output, primitive),
        IndirectObject::Stream { dict, data } => {
            let mut stream_dict = dict.clone();
            stream_dict.insert("Length".to_string(), Primitive::Integer(data.len() as i64));
            write_primitive(output, &Primitive::Dictionary(stream_dict));
            output.extend_from_slice(b"\nstream\n");
            output.extend_from_slice(data);
            output.extend_from_slice(b"\nendstream");
        }
    }
}

fn write_primitive(output: &mut Vec<u8>, primitive: &Primitive) {
    match primitive {
        Primitive::Null => output.extend_from_slice(b"null"),
        Primitive::Boolean(value) => {
            output.extend_from_slice(if *value { b"true" } else { b"false" });
        }
        Primitive::Integer(value) => output.extend_from_slice(value.to_string().as_bytes()),
        Primitive::Real(value) => output.extend_from_slice(format_real(*value).as_bytes()),
        Primitive::Name(name) => write_name(output, name),
        Primitive::StringLiteral(data) => write_literal_string(output, data),
        Primitive::HexString(data) => write_hex_string(output, data),
        Primitive::Array(items) => {
            output.push(b'[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(b' ');
                }
                write_primitive(output, item);
            }
            output.push(b']');
        }
        Primitive::Dictionary(dict) => {
            output.extend_from_slice(b"<<\n");
            for (key, value) in dict {
                output.extend_from_slice(b"  ");
                write_name(output, key);
                output.push(b' ');
                write_primitive(output, value);
                output.push(b'\n');
            }
            output.extend_from_slice(b">>");
        }
        Primitive::Reference((id, generation)) => {
            output.extend_from_slice(format!("{} {} R", id, generation).as_bytes());
        }
    }
}

fn write_name(output: &mut Vec<u8>, name: &str) {
    output.push(b'/');
    for byte in name.as_bytes() {
        if byte.is_ascii_whitespace()
            || matches!(
                *byte,
                b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
            )
            || !byte.is_ascii_graphic()
        {
            output.extend_from_slice(format!("#{:02X}", byte).as_bytes());
        } else {
            output.push(*byte);
        }
    }
}

fn write_literal_string(output: &mut Vec<u8>, data: &[u8]) {
    output.push(b'(');
    for byte in data {
        match *byte {
            b'(' => output.extend_from_slice(b"\\("),
            b')' => output.extend_from_slice(b"\\)"),
            b'\\' => output.extend_from_slice(b"\\\\"),
            other => output.push(other),
        }
    }
    output.push(b')');
}

fn write_hex_string(output: &mut Vec<u8>, data: &[u8]) {
    output.push(b'<');
    for byte in data {
        output.extend_from_slice(format!("{:02X}", byte).as_bytes());
    }
    output.push(b'>');
}

fn format_real(value: f64) -> String {
    if value.is_nan() || value.is_infinite() {
        return "0".to_string();
    }
    let mut s = format!("{:.6}", value);
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() || s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flate_decode_parms_apply_png_up_predictor_with_prefix() {
        let mut decode_parms = Dict::new();
        decode_parms.insert("Predictor".to_string(), Primitive::Integer(12));
        decode_parms.insert("Colors".to_string(), Primitive::Integer(1));
        decode_parms.insert("BitsPerComponent".to_string(), Primitive::Integer(8));
        decode_parms.insert("Columns".to_string(), Primitive::Integer(5));

        let encoded = vec![2, 1, 2, 3, 4, 5, 2, 5, 5, 5, 5, 5];
        let decoded = apply_flate_decode_params(&encoded, Some(&decode_parms)).unwrap();

        assert_eq!(decoded, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn png_predictor_falls_back_to_fixed_filter_without_prefix() {
        let encoded = vec![1, 2, 3, 4, 5, 5, 5, 5, 5, 5];
        let decoded = decode_png_predictor(&encoded, 12, 1, 8, 5).unwrap();

        assert_eq!(decoded, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }
}
