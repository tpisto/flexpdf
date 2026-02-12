use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::components::{
    Document,
    FontDefinition,
    FontSource,
    HyphenationConfig,
    HyphenationLang,
    ImportedPdfPages,
    Orientation,
    Page,
    PageSize,
};
use crate::style::parse_style;

use super::components::{parse_component, parse_empty_component};
use super::error::ParseError;
use super::util::skip_element;

pub fn parse_xml(xml: &str) -> Result<Document, ParseError> {
    let mut reader = Reader::from_str(xml);
    // Don't trim text - we need whitespace for inline spans.
    // We handle whitespace normalization ourselves in normalize_whitespace functions.
    reader.trim_text(false);

    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"Document" {
                    return parse_document(&mut reader, e);
                }
            }
            Ok(Event::Eof) => {
                return Err(ParseError::MissingDocument);
            }
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }
}

fn parse_document(reader: &mut Reader<&[u8]>, start: &BytesStart) -> Result<Document, ParseError> {
    let mut doc = Document::default();

    // Parse attributes
    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "title" => doc.title = Some(value),
            "author" => doc.author = Some(value),
            "subject" => doc.subject = Some(value),
            "keywords" => doc.keywords = Some(value),
            "pageMode" => doc.page_mode = Some(value),
            "hyphenation" => doc.hyphenation = HyphenationLang::from_str(&value),
            _ => {}
        }
    }

    // Parse children
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag_name = e.name();
                if tag_name.as_ref() == b"Page" {
                    let page = parse_page(reader, e)?;
                    doc.push_page(page);
                } else if tag_name.as_ref() == b"Fonts" {
                    let fonts = parse_fonts(reader)?;
                    doc.fonts = fonts;
                } else if tag_name.as_ref() == b"ImportPdf" {
                    let import = parse_import_pdf(e)?;
                    doc.push_import_pdf(import);
                    skip_element(reader, "ImportPdf")?;
                }
            }
            Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"ImportPdf" {
                    let import = parse_import_pdf(e)?;
                    doc.push_import_pdf(import);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"Document" => {
                break;
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(doc)
}

fn parse_import_pdf(start: &BytesStart) -> Result<ImportedPdfPages, ParseError> {
    let mut src: Option<String> = None;
    let mut pages: Option<Vec<u32>> = None;

    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(ParseError::XmlError)?
            .into_owned();

        match key {
            "src" => src = Some(value),
            "pages" => pages = Some(parse_import_pages(&value)?),
            _ => {}
        }
    }

    let src = src.ok_or_else(|| ParseError::AttrError("ImportPdf requires src".to_string()))?;
    let pages = pages.ok_or_else(|| ParseError::AttrError("ImportPdf requires pages".to_string()))?;

    Ok(ImportedPdfPages::new(src, pages))
}

fn parse_import_pages(value: &str) -> Result<Vec<u32>, ParseError> {
    let mut pages = Vec::new();

    for token in value.split(',').flat_map(|part| part.split_whitespace()) {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        if let Some((start, end)) = token.split_once('-') {
            let start = start.trim().parse::<u32>().map_err(|_| {
                ParseError::AttrError(format!("Invalid page number '{}'", start.trim()))
            })?;
            let end = end.trim().parse::<u32>().map_err(|_| {
                ParseError::AttrError(format!("Invalid page number '{}'", end.trim()))
            })?;
            if start == 0 || end == 0 || start > end {
                return Err(ParseError::AttrError(format!(
                    "Invalid page range '{}'",
                    token
                )));
            }
            for page in start..=end {
                pages.push(page);
            }
        } else {
            let page = token
                .parse::<u32>()
                .map_err(|_| ParseError::AttrError(format!("Invalid page number '{}'", token)))?;
            if page == 0 {
                return Err(ParseError::AttrError(
                    "Page numbers must be 1-based".to_string(),
                ));
            }
            pages.push(page);
        }
    }

    if pages.is_empty() {
        return Err(ParseError::AttrError(
            "ImportPdf pages list cannot be empty".to_string(),
        ));
    }

    Ok(pages)
}

/// Parse <Fonts> element containing font definitions.
fn parse_fonts(reader: &mut Reader<&[u8]>) -> Result<Vec<FontDefinition>, ParseError> {
    let mut fonts = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) => {
                // Self-closing <Font ... />
                if e.name().as_ref() == b"Font" {
                    if let Some(font_def) = parse_font_definition(e)? {
                        fonts.push(font_def);
                    }
                }
            }
            Ok(Event::Start(ref e)) => {
                // <Font ...> with content - skip to </Font>
                if e.name().as_ref() == b"Font" {
                    if let Some(font_def) = parse_font_definition(e)? {
                        fonts.push(font_def);
                    }
                    skip_element(reader, "Font")?;
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"Fonts" => {
                break;
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(fonts)
}

/// Parse a single <Font> element.
fn parse_font_definition(start: &BytesStart) -> Result<Option<FontDefinition>, ParseError> {
    let mut family: Option<String> = None;
    let mut google: Option<String> = None;
    let mut src: Option<String> = None;

    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "family" => family = Some(value),
            "google" => google = Some(value),
            "src" => src = Some(value),
            _ => {}
        }
    }

    // Must have a family name
    let family = match family {
        Some(f) => f,
        None => return Ok(None),
    };

    // Determine the source (google takes precedence over src)
    let source = if let Some(g) = google {
        FontSource::Google(g)
    } else if let Some(s) = src {
        FontSource::Local(s)
    } else {
        // No source specified, skip this font
        return Ok(None);
    };

    Ok(Some(FontDefinition { family, source }))
}

fn parse_page(reader: &mut Reader<&[u8]>, start: &BytesStart) -> Result<Page, ParseError> {
    let mut page = Page::default();

    // Parse attributes
    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "size" => page.size = PageSize::from_str(&value),
            "orientation" => page.orientation = Orientation::from_str(&value),
            "style" => page.style = parse_style(&value),
            "wrap" => {
                page.wrap = matches!(value.trim().to_lowercase().as_str(), "true" | "1");
            }
            "hyphenation" => page.hyphenation_config = HyphenationConfig::from_str(&value),
            _ => {}
        }
    }

    // Parse children
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let Some(comp) = parse_component(reader, e)? {
                    page.children.push(comp);
                }
            }
            Ok(Event::Empty(ref e)) => {
                if let Some(comp) = parse_empty_component(e)? {
                    page.children.push(comp);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"Page" => {
                break;
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(page)
}
