use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::components::{
    BreakType,
    Component,
    Image,
    Link,
    Note,
    ObjectFit,
    View,
};
use crate::style::parse_style;

use super::error::ParseError;
use super::text::{normalize_whitespace, parse_text};
use super::util::skip_element;

fn parse_bool_attr(value: &str) -> Result<bool, ParseError> {
    match value.trim().to_lowercase().as_str() {
        "true" | "1" => Ok(true),
        "false" | "0" | "" => Ok(false),
        other => Err(ParseError::AttrError(format!(
            "Invalid boolean value '{}'",
            other
        ))),
    }
}

fn parse_f32_attr(key: &str, value: &str) -> Result<f32, ParseError> {
    value.trim().parse::<f32>().map_err(|_| {
        ParseError::AttrError(format!("Invalid {} value '{}'", key, value))
    })
}

pub(super) fn parse_component(
    reader: &mut Reader<&[u8]>,
    start: &BytesStart,
) -> Result<Option<Component>, ParseError> {
    let name = start.name();
    let tag = std::str::from_utf8(name.as_ref())?;

    match tag {
        "View" => Ok(Some(Component::View(parse_view(reader, start)?))),
        "Text" => Ok(Some(Component::Text(parse_text(reader, start)?))),
        "Link" => Ok(Some(Component::Link(parse_link(reader, start)?))),
        "Note" => Ok(Some(Component::Note(parse_note(reader, start)?))),
        "Image" => {
            // Image with content (skip to closing tag)
            let image = parse_image(start)?;
            skip_element(reader, tag)?;
            Ok(Some(Component::Image(image)))
        }
        _ => {
            // Skip unknown elements
            skip_element(reader, tag)?;
            Ok(None)
        }
    }
}

pub(super) fn parse_empty_component(
    start: &BytesStart,
) -> Result<Option<Component>, ParseError> {
    let name = start.name();
    let tag = std::str::from_utf8(name.as_ref())?;

    match tag {
        "Image" => Ok(Some(Component::Image(parse_image(start)?))),
        "View" => Ok(Some(Component::View(parse_empty_view(start)?))),
        "Note" => Ok(Some(Component::Note(parse_note_empty(start)?))),
        _ => Ok(None),
    }
}

/// Parse a self-closing View element (e.g., <View break="page" />).
fn parse_empty_view(start: &BytesStart) -> Result<View, ParseError> {
    let mut view = View::default();

    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "style" => view.style = parse_style(&value),
            "break" => view.break_before = BreakType::from_str(&value),
            "id" => view.id = Some(value),
            "fixed" => view.fixed = parse_bool_attr(&value)?,
            "wrap" => view.wrap = Some(parse_bool_attr(&value)?),
            "minPresenceAhead" => {
                view.min_presence_ahead = parse_f32_attr(key, &value)?.max(0.0);
            }
            _ => {}
        }
    }

    Ok(view)
}

/// Parse an Image element.
fn parse_image(start: &BytesStart) -> Result<Image, ParseError> {
    let mut image = Image::default();

    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "src" => image.src = value,
            "style" => {
                image.style = parse_style(&value);
                // Check for objectFit in style
                if let Some(obj_fit) = image.style.object_fit.take() {
                    image.object_fit = obj_fit;
                }
            }
            "objectFit" => image.object_fit = ObjectFit::from_str(&value),
            "break" => image.break_before = BreakType::from_str(&value),
            "fixed" => image.fixed = parse_bool_attr(&value)?,
            "wrap" => image.wrap = Some(parse_bool_attr(&value)?),
            "minPresenceAhead" => {
                image.min_presence_ahead = parse_f32_attr(key, &value)?.max(0.0);
            }
            _ => {}
        }
    }

    Ok(image)
}

fn parse_view(reader: &mut Reader<&[u8]>, start: &BytesStart) -> Result<View, ParseError> {
    let mut view = View::default();

    // Parse attributes
    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "style" => view.style = parse_style(&value),
            "break" => view.break_before = BreakType::from_str(&value),
            "id" => view.id = Some(value),
            "fixed" => view.fixed = parse_bool_attr(&value)?,
            "wrap" => view.wrap = Some(parse_bool_attr(&value)?),
            "minPresenceAhead" => {
                view.min_presence_ahead = parse_f32_attr(key, &value)?.max(0.0);
            }
            _ => {}
        }
    }

    // Parse children
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let Some(comp) = parse_component(reader, e)? {
                    view.children.push(comp);
                }
            }
            Ok(Event::Empty(ref e)) => {
                if let Some(comp) = parse_empty_component(e)? {
                    view.children.push(comp);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"View" => {
                break;
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(view)
}

fn parse_link(reader: &mut Reader<&[u8]>, start: &BytesStart) -> Result<Link, ParseError> {
    let mut link = Link::default();

    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "src" => link.src = value,
            "style" => link.style = parse_style(&value),
            "break" => link.break_before = BreakType::from_str(&value),
            "fixed" => link.fixed = parse_bool_attr(&value)?,
            "wrap" => link.wrap = Some(parse_bool_attr(&value)?),
            "minPresenceAhead" => {
                link.min_presence_ahead = parse_f32_attr(key, &value)?.max(0.0);
            }
            _ => {}
        }
    }

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if let Some(comp) = parse_component(reader, e)? {
                    link.children.push(comp);
                }
            }
            Ok(Event::Empty(ref e)) => {
                if let Some(comp) = parse_empty_component(e)? {
                    link.children.push(comp);
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"Link" => {
                break;
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(link)
}

fn parse_note(reader: &mut Reader<&[u8]>, start: &BytesStart) -> Result<Note, ParseError> {
    let mut note = Note::default();

    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "style" => note.style = parse_style(&value),
            "break" => note.break_before = BreakType::from_str(&value),
            "fixed" => note.fixed = parse_bool_attr(&value)?,
            "wrap" => note.wrap = Some(parse_bool_attr(&value)?),
            "minPresenceAhead" => {
                note.min_presence_ahead = parse_f32_attr(key, &value)?.max(0.0);
            }
            _ => {}
        }
    }

    let mut content = String::new();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref e)) => {
                content.push_str(&e.unescape().map_err(|e| ParseError::XmlError(e))?.into_owned());
            }
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let tag = std::str::from_utf8(name.as_ref())?;
                skip_element(reader, tag)?;
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"Note" => {
                break;
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    note.content = normalize_whitespace(&content);
    Ok(note)
}

fn parse_note_empty(start: &BytesStart) -> Result<Note, ParseError> {
    let mut note = Note::default();

    for attr in start.attributes() {
        let attr = attr?;
        let key = std::str::from_utf8(attr.key.as_ref())?;
        let value = attr
            .unescape_value()
            .map_err(|e| ParseError::XmlError(e))?
            .into_owned();

        match key {
            "style" => note.style = parse_style(&value),
            "break" => note.break_before = BreakType::from_str(&value),
            "fixed" => note.fixed = parse_bool_attr(&value)?,
            "wrap" => note.wrap = Some(parse_bool_attr(&value)?),
            "minPresenceAhead" => {
                note.min_presence_ahead = parse_f32_attr(key, &value)?.max(0.0);
            }
            _ => {}
        }
    }

    Ok(note)
}
