# flexpdf

flexpdf is a Rust library that renders a small, React-PDF-like XML syntax (or a Rust document model) into a PDF byte buffer. It aims for compatibility with [react-pdf](https://github.com/diegomura/react-pdf) layouts and styling. It is built on a flexbox layout engine and is designed to be easy to embed into other Rust programs.

## Features

- Render PDF from XML strings.
- Build documents directly in Rust without XML.
- Flexbox-based layout (via [Taffy](https://github.com/DioxusLabs/taffy)).
- Aiming for react-pdf compatibility.
- Embedded fonts (Google Fonts, local files, and PDF standard fonts).
- Images, links, annotations, and bookmarks.

## Installation

```toml
[dependencies]
flexpdf = { path = "./flexpdf" }
```

## Powered by

- [Taffy](https://github.com/DioxusLabs/taffy) for flexbox layout.
- [Parley](https://github.com/linebender/parley) for text layout.
- [skrifa](https://github.com/linebender/skrifa) and [read-fonts](https://github.com/linebender/read-fonts) for font parsing and metrics.
- [quick-xml](https://github.com/tafia/quick-xml) for XML parsing.
- [reqwest](https://github.com/seanmonstar/reqwest) for font downloads.

## Usage (XML)

```rust
use flexpdf::render_xml;

let xml = r#"
<Document title="Quarterly Brief" author="Acme Studio">
  <Fonts>
    <Font family="Manrope" google="Manrope" />
  </Fonts>
  <Page size="A4">
    <View style="width: 100%; height: 100%; padding: 32; gap: 20; backgroundColor: #f8fafc; fontFamily: Manrope; color: #0f172a;">
      <View style="padding: 16; borderRadius: 12; backgroundColor: #2563eb; flexDirection: row; alignItems: center; justifyContent: space-between;">
        <Text style="color: #ffffff; fontSize: 18; fontWeight: 600;">Launch Brief</Text>
        <Text style="color: #ffffff; fontSize: 12;">Q2 2025</Text>
      </View>
      <View style="gap: 16; flexGrow: 1;">
        <Text style="fontSize: 16; fontWeight: 600;">Highlights</Text>
        <View style="flexDirection: row; gap: 12;">
          <View style="flexGrow: 1; padding: 14; borderRadius: 10; backgroundColor: #dbeafe;">
            <Text style="fontSize: 10; color: #64748b;">Signups</Text>
            <Text style="fontSize: 18; fontWeight: 700;">3,482</Text>
          </View>
          <View style="flexGrow: 1; padding: 14; borderRadius: 10; backgroundColor: #dcfce7;">
            <Text style="fontSize: 10; color: #64748b;">Retention</Text>
            <Text style="fontSize: 18; fontWeight: 700;">68%</Text>
          </View>
          <View style="flexGrow: 1; padding: 14; borderRadius: 10; backgroundColor: #fef3c7;">
            <Text style="fontSize: 10; color: #64748b;">NPS</Text>
            <Text style="fontSize: 18; fontWeight: 700;">54</Text>
          </View>
        </View>
        <View style="padding: 16; borderRadius: 10; backgroundColor: #e2e8f0;">
          <Text style="fontWeight: 600;">Next steps</Text>
          <Text style="color: #64748b;">Finalize onboarding and ship the analytics refresh.</Text>
        </View>
      </View>
      <View style="flexDirection: row; justifyContent: space-between; alignItems: center;">
        <Text style="color: #64748b; fontSize: 10;">Acme Studio • Internal</Text>
        <Text style="color: #64748b; fontSize: 10;">Page {pageNumber} of {totalPages}</Text>
      </View>
    </View>
  </Page>
</Document>
"#;

let pdf_bytes = render_xml(xml)?;
std::fs::write("styled.pdf", pdf_bytes)?;
# Ok::<(), flexpdf::Error>(())
```

## Usage (Rust model)

```rust
use flexpdf::builder::{document, text, view};
use flexpdf::{render_document, PageSize, Style};

let doc = document()
    .title("Hello")
    .page_with(PageSize::A4, |page| {
        page.child(
            view()
                .style(Style {
                    padding: Some(24.0),
                    ..Style::default()
                })
                .children([text("Hello from Rust"), text("Second line")]),
        )
    })
    .build();

let pdf_bytes = render_document(&doc)?;
std::fs::write("hello.pdf", pdf_bytes)?;
# Ok::<(), flexpdf::Error>(())
```

## Import existing PDF pages

You can include imported pages anywhere in the document flow:

```rust
use flexpdf::builder::{document, text, view};
use flexpdf::{render_document, PageSize};

let input_bytes = std::fs::read("input.pdf")?;

let doc = document()
    .page_with(PageSize::A4, |page| page.child(view().child(text("Dynamic A"))))
    .import_pdf_bytes_pages(input_bytes, [1, 2])
    .page_with(PageSize::A4, |page| page.child(view().child(text("Dynamic B"))))
    .import_pdf_pages("appendix.pdf", [1])
    .page_with(PageSize::A4, |page| page.child(view().child(text("Dynamic C"))))
    .build();

let pdf_bytes = render_document(&doc)?;
std::fs::write("output.pdf", pdf_bytes)?;
# Ok::<(), flexpdf::Error>(())
```

XML supports the same flow with `<ImportPdf />` as a sibling of `<Page>`:

```xml
<Document>
  <Page size="A4"><Text>Dynamic A</Text></Page>
  <ImportPdf src="input.pdf" pages="1,2" />
  <Page size="A4"><Text>Dynamic B</Text></Page>
  <ImportPdf src="appendix.pdf" pages="1-2,4" />
  <Page size="A4"><Text>Dynamic C</Text></Page>
</Document>
```

`pages` accepts comma-separated page numbers and ranges (`1,3-5`).
Use `import_pdf_pages(path, pages)` for file paths and `import_pdf_bytes_pages(bytes, pages)` for in-memory Rust `Vec<u8>` data.
Native import supports both classic xref-table PDFs and xref-stream/object-stream PDFs.

## Library entry points

- `render_xml(xml: &str) -> Result<Vec<u8>, Error>`
- `parse_xml(xml: &str) -> Result<Document, Error>`
- `render_document(doc: &Document) -> Result<Vec<u8>, Error>`

## CLI (optional)

A small CLI binary is included for development:

```bash
cargo run --manifest-path flexpdf/Cargo.toml --bin flexpdf -- path/to/input.xml path/to/output.pdf
```

## License

This project is dual-licensed under either of:

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT License (`LICENSE-MIT`)

The AFM font metric data in `assets/pdfkit/afm` is derived from PDFKit and is licensed under the MIT license in `assets/pdfkit/LICENSE`.
