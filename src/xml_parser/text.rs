use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::components::{Text, TextSpan};
use crate::style::parse_style;

use super::error::ParseError;
use super::util::skip_element;

pub(super) fn parse_text(
    reader: &mut Reader<&[u8]>,
    start: &BytesStart,
) -> Result<Text, ParseError> {
    let mut text = Text::default();

    // Parse attributes
    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        if key == "style" {
            text.style = parse_style(&value);
        } else if key == "bookmark" {
            text.bookmark = Some(value);
        }
    }

    // Parse text content with potential inline spans
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut current_text = String::new();
    let mut buf = Vec::new();
    let mut is_first_text = true; // Track if this is the first text segment

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref e)) => {
                let text_content =
                    e.unescape().map_err(|e| ParseError::XmlError(e))?.into_owned();
                current_text.push_str(&text_content);
            }
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"Text" {
                    // Save any accumulated plain text as a span (with empty/default style)
                    // - First segment: trim leading, keep trailing
                    // - After a span: keep leading (space after </Text>), keep trailing
                    let normalized =
                        normalize_whitespace_for_span(&current_text, is_first_text, false);
                    if !normalized.is_empty() {
                        spans.push(TextSpan {
                            content: normalized,
                            style: crate::style::Style::default(),
                        });
                    }
                    current_text.clear();
                    is_first_text = false; // After first span, don't trim leading

                    // Parse the nested Text element as a span
                    let span = parse_text_span(reader, e)?;
                    spans.push(span);
                } else {
                    // Skip unknown nested elements
                    let name = e.name();
                    let tag = std::str::from_utf8(name.as_ref())?;
                    skip_element(reader, tag)?;
                }
            }
            Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"Text" {
                    // Self-closing nested <Text /> - parse style but no content
                    let span = parse_text_span_empty(e)?;
                    if !span.content.is_empty() {
                        spans.push(span);
                    }
                    is_first_text = false;
                }
                // Other empty elements are ignored
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"Text" {
                    break;
                }
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    // Handle remaining text after last span
    // - If no spans: trim both leading and trailing (normal behavior)
    // - If there are spans: keep leading (space after last </Text>), trim trailing
    if !current_text.is_empty() {
        if spans.is_empty() {
            // No nested spans - use simple content with normal normalization
            let normalized = normalize_whitespace(&current_text);
            if !normalized.is_empty() {
                text.content = normalized;
            }
        } else {
            // Add as final span - keep leading space, trim trailing
            let normalized = normalize_whitespace_for_span(&current_text, false, true);
            if !normalized.is_empty() {
                spans.push(TextSpan {
                    content: normalized,
                    style: crate::style::Style::default(),
                });
            }
        }
    }

    // If we have spans, store them
    if !spans.is_empty() {
        text.spans = spans;
    }

    Ok(text)
}

/// Parse a nested Text element as a TextSpan.
fn parse_text_span(reader: &mut Reader<&[u8]>, start: &BytesStart) -> Result<TextSpan, ParseError> {
    let mut span = TextSpan::default();

    // Parse attributes (style)
    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        if key == "style" {
            span.style = parse_style(&value);
        }
    }

    // Parse text content (no further nesting supported for now)
    let mut content = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref e)) => {
                content.push_str(&e.unescape().map_err(|e| ParseError::XmlError(e))?.into_owned());
            }
            Ok(Event::Start(ref e)) => {
                // Skip nested elements within spans (no deep nesting)
                let name = e.name();
                let tag = std::str::from_utf8(name.as_ref())?;
                skip_element(reader, tag)?;
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"Text" {
                    break;
                }
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    span.content = normalize_whitespace(&content);
    Ok(span)
}

/// Parse a self-closing nested Text element.
fn parse_text_span_empty(start: &BytesStart) -> Result<TextSpan, ParseError> {
    let mut span = TextSpan::default();

    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        if key == "style" {
            span.style = parse_style(&value);
        }
    }

    Ok(span)
}

pub(super) fn normalize_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut last_was_whitespace = true; // Start true to trim leading

    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_was_whitespace {
                result.push(' ');
                last_was_whitespace = true;
            }
        } else {
            result.push(ch);
            last_was_whitespace = false;
        }
    }

    // Trim trailing
    result.trim_end().to_string()
}

/// Normalize whitespace for inline spans with control over leading/trailing.
/// trim_leading: whether to trim leading whitespace
/// trim_trailing: whether to trim trailing whitespace
pub(super) fn normalize_whitespace_for_span(
    s: &str,
    trim_leading: bool,
    trim_trailing: bool,
) -> String {
    let mut result = String::with_capacity(s.len());
    let mut last_was_whitespace = trim_leading; // Start true to trim leading

    for ch in s.chars() {
        if ch.is_whitespace() {
            if !last_was_whitespace {
                result.push(' ');
                last_was_whitespace = true;
            }
        } else {
            result.push(ch);
            last_was_whitespace = false;
        }
    }

    if trim_trailing {
        result.trim_end().to_string()
    } else {
        result
    }
}
