//! XML parser for the flexpdf document format.
//! Parses Document, Page, View, Text, Fonts elements.

mod components;
mod document;
mod error;
mod text;
mod util;

pub use document::parse_xml;
pub use error::ParseError;
