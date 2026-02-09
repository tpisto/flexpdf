//! TrueType font embedding for PDF.
//! Embeds fonts with proper metrics.

mod cid;
mod standard;
mod truetype;
mod unicode;

pub use cid::embed_cid_font;
pub use standard::{embed_noto_sans, embed_standard_font};
pub use truetype::embed_font;
