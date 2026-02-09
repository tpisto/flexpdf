use quick_xml::events::Event;
use quick_xml::Reader;

use super::error::ParseError;

pub(super) fn skip_element(
    reader: &mut Reader<&[u8]>,
    tag_name: &str,
) -> Result<(), ParseError> {
    let mut buf = Vec::new();
    let mut depth = 1;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(ref e)) => {
                depth -= 1;
                if depth == 0 && std::str::from_utf8(e.name().as_ref())? == tag_name {
                    break;
                }
            }
            Ok(Event::Eof) => return Err(ParseError::UnexpectedEof),
            Err(e) => return Err(ParseError::XmlError(e)),
            _ => {}
        }
        buf.clear();
    }

    Ok(())
}
