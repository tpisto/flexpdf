//! Style types and CSS-like parsing helpers.
//! Most fields are optional and map to common flexbox and text properties.

mod parse;
mod types;

pub use parse::parse_style;
pub use types::*;
