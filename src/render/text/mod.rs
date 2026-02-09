//! Text rendering utilities.

mod font;
mod justify;
mod lines;
mod render;
mod segments;

pub(super) use render::{render_text, render_text_with_spans, resolve_placeholders};
