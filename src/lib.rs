//! flexpdf turns a small XML syntax (React-PDF style) or a Rust document model
//! into a PDF byte buffer.
//!
//! The library is split into two entry points:
//! - Use [`render_xml`] when you already have XML.
//! - Use [`render_document`] when you want to build a [`Document`] in Rust.
//!
//! # XML example (styled)
//! ```no_run
//! use flexpdf::render_xml;
//!
//! let xml = r#"
//! <Document title="Quarterly Brief" author="Acme Studio">
//!   <Fonts>
//!     <Font family="Manrope" google="Manrope" />
//!   </Fonts>
//!   <Page size="A4">
//!     <View style="width: 100%; height: 100%; padding: 32; gap: 20; backgroundColor: #f8fafc; fontFamily: Manrope; color: #0f172a;">
//!       <View style="padding: 16; borderRadius: 12; backgroundColor: #2563eb; flexDirection: row; alignItems: center; justifyContent: space-between;">
//!         <Text style="color: #ffffff; fontSize: 18; fontWeight: 600;">Launch Brief</Text>
//!         <Text style="color: #ffffff; fontSize: 12;">Q2 2025</Text>
//!       </View>
//!       <View style="gap: 16; flexGrow: 1;">
//!         <Text style="fontSize: 16; fontWeight: 600;">Highlights</Text>
//!         <View style="flexDirection: row; gap: 12;">
//!           <View style="flexGrow: 1; padding: 14; borderRadius: 10; backgroundColor: #dbeafe;">
//!             <Text style="fontSize: 10; color: #64748b;">Signups</Text>
//!             <Text style="fontSize: 18; fontWeight: 700;">3,482</Text>
//!           </View>
//!           <View style="flexGrow: 1; padding: 14; borderRadius: 10; backgroundColor: #dcfce7;">
//!             <Text style="fontSize: 10; color: #64748b;">Retention</Text>
//!             <Text style="fontSize: 18; fontWeight: 700;">68%</Text>
//!           </View>
//!           <View style="flexGrow: 1; padding: 14; borderRadius: 10; backgroundColor: #fef3c7;">
//!             <Text style="fontSize: 10; color: #64748b;">NPS</Text>
//!             <Text style="fontSize: 18; fontWeight: 700;">54</Text>
//!           </View>
//!         </View>
//!         <View style="padding: 16; borderRadius: 10; backgroundColor: #e2e8f0;">
//!           <Text style="fontWeight: 600;">Next steps</Text>
//!           <Text style="color: #64748b;">Finalize onboarding and ship the analytics refresh.</Text>
//!         </View>
//!       </View>
//!       <View style="flexDirection: row; justifyContent: space-between; alignItems: center;">
//!         <Text style="color: #64748b; fontSize: 10;">Acme Studio • Internal</Text>
//!         <Text style="color: #64748b; fontSize: 10;">Page {pageNumber} of {totalPages}</Text>
//!       </View>
//!     </View>
//!   </Page>
//! </Document>
//! "#;
//!
//! let pdf_bytes = render_xml(xml)?;
//! std::fs::write("hello.pdf", pdf_bytes)?;
//! # Ok::<(), flexpdf::Error>(())
//! ```
//!
//! # Rust quick start (no XML)
//! ```no_run
//! use flexpdf::builder::{document, text, view};
//! use flexpdf::{render_document, PageSize, Style};
//!
//! let doc = document()
//!     .title("Hello")
//!     .page_with(PageSize::A4, |page| {
//!         page.child(
//!             view().style(Style {
//!                 padding: Some(24.0),
//!                 ..Style::default()
//!             })
//!             .children([text("Hello from Rust"), text("Second line")]),
//!         )
//!     })
//!     .build();
//!
//! let pdf_bytes = render_document(&doc)?;
//! std::fs::write("hello.pdf", pdf_bytes)?;
//! # Ok::<(), flexpdf::Error>(())
//! ```

pub mod components;
pub mod builder;
pub mod fonts;
pub mod image;
pub mod layout;
pub mod pdf;
pub mod render;
pub mod standard_fonts;
pub mod style;
pub mod xml_parser;

pub use crate::fonts::embed as font_embed;
pub use crate::fonts::google as google_fonts;

pub use builder::{
    DocumentBuilder, ImageBuilder, LinkBuilder, NoteBuilder, PageBuilder, TextBuilder, ViewBuilder,
};
pub use components::{
    BreakType, Component, Document, FontDefinition, FontSource, HyphenationLang, Image, Link, Note,
    ObjectFit, Orientation, Page, PageSize, Text, TextSpan, View,
};
pub use render::RenderError;
pub use style::{Color, Style};
pub use xml_parser::ParseError;

/// Library error type.
#[derive(Debug)]
pub enum Error {
    /// XML parsing failed.
    Parse(ParseError),
    /// PDF rendering failed.
    Render(RenderError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(err) => write!(f, "XML parse error: {}", err),
            Error::Render(err) => write!(f, "PDF render error: {}", err),
        }
    }
}

impl std::error::Error for Error {}

impl From<ParseError> for Error {
    fn from(err: ParseError) -> Self {
        Error::Parse(err)
    }
}

impl From<RenderError> for Error {
    fn from(err: RenderError) -> Self {
        Error::Render(err)
    }
}

/// Parse XML into a [`Document`].
pub fn parse_xml(xml: &str) -> Result<Document, Error> {
    xml_parser::parse_xml(xml).map_err(Error::Parse)
}

/// Render a [`Document`] into PDF bytes.
pub fn render_document(doc: &Document) -> Result<Vec<u8>, Error> {
    render::render_document(doc).map_err(Error::Render)
}

/// Parse XML and render it into PDF bytes.
pub fn render_xml(xml: &str) -> Result<Vec<u8>, Error> {
    let doc = parse_xml(xml)?;
    render_document(&doc)
}
