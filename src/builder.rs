//! Fluent builders for creating documents without XML.

use crate::components::{
    BreakType, Component, Document, FontDefinition, HyphenationLang, Image, ImportedPdfPages, Link,
    Note, ObjectFit, Orientation, Page, PageSize, Text, TextSpan, View,
};
use crate::style::Style;

/// Start a new document builder.
pub fn document() -> DocumentBuilder {
    DocumentBuilder::new()
}

/// Start a new page builder.
pub fn page(size: PageSize) -> PageBuilder {
    PageBuilder::new(size)
}

/// Start a new view builder.
pub fn view() -> ViewBuilder {
    ViewBuilder::new()
}

/// Start a new text builder.
pub fn text(content: impl Into<String>) -> TextBuilder {
    TextBuilder::new(content)
}

/// Start a new image builder.
pub fn image(src: impl Into<String>) -> ImageBuilder {
    ImageBuilder::new(src)
}

/// Start a new link builder.
pub fn link(src: impl Into<String>) -> LinkBuilder {
    LinkBuilder::new(src)
}

/// Start a new note builder.
pub fn note(content: impl Into<String>) -> NoteBuilder {
    NoteBuilder::new(content)
}

#[derive(Debug, Default)]
pub struct DocumentBuilder {
    doc: Document,
}

impl DocumentBuilder {
    pub fn new() -> Self {
        Self {
            doc: Document::default(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.doc.title = Some(title.into());
        self
    }

    pub fn author(mut self, author: impl Into<String>) -> Self {
        self.doc.author = Some(author.into());
        self
    }

    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.doc.subject = Some(subject.into());
        self
    }

    pub fn keywords(mut self, keywords: impl Into<String>) -> Self {
        self.doc.keywords = Some(keywords.into());
        self
    }

    pub fn page_mode(mut self, page_mode: impl Into<String>) -> Self {
        self.doc.page_mode = Some(page_mode.into());
        self
    }

    pub fn hyphenation(mut self, lang: HyphenationLang) -> Self {
        self.doc.hyphenation = Some(lang);
        self
    }

    pub fn font(mut self, font: FontDefinition) -> Self {
        self.doc.fonts.push(font);
        self
    }

    pub fn import_pdf_pages(
        mut self,
        src: impl Into<String>,
        pages: impl IntoIterator<Item = u32>,
    ) -> Self {
        self.doc.push_import_pdf(ImportedPdfPages::new(src, pages));
        self
    }

    pub fn import_pdf_bytes_pages(
        mut self,
        bytes: impl Into<Vec<u8>>,
        pages: impl IntoIterator<Item = u32>,
    ) -> Self {
        self.doc.push_import_pdf(ImportedPdfPages::from_bytes(bytes, pages));
        self
    }

    pub fn push_font(&mut self, font: FontDefinition) -> &mut Self {
        self.doc.fonts.push(font);
        self
    }

    pub fn page(mut self, page: impl Into<Page>) -> Self {
        self.doc.push_page(page.into());
        self
    }

    pub fn page_with<F>(mut self, size: PageSize, build: F) -> Self
    where
        F: FnOnce(PageBuilder) -> PageBuilder,
    {
        let page = build(PageBuilder::new(size)).build();
        self.doc.push_page(page);
        self
    }

    pub fn pages<I, P>(mut self, pages: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<Page>,
    {
        for page in pages {
            self.doc.push_page(page.into());
        }
        self
    }

    pub fn push_page(&mut self, page: impl Into<Page>) -> &mut Self {
        self.doc.push_page(page.into());
        self
    }

    pub fn build(self) -> Document {
        self.doc
    }
}

#[derive(Debug)]
pub struct PageBuilder {
    page: Page,
}

impl PageBuilder {
    pub fn new(size: PageSize) -> Self {
        let mut page = Page::default();
        page.size = size;
        Self { page }
    }

    pub fn orientation(mut self, orientation: Orientation) -> Self {
        self.page.orientation = orientation;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.page.style = style;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.page.wrap = wrap;
        self
    }

    pub fn hyphenation(mut self, lang: HyphenationLang) -> Self {
        self.page.hyphenation_config = crate::components::HyphenationConfig::Language(lang);
        self
    }

    pub fn no_hyphenation(mut self) -> Self {
        self.page.hyphenation_config = crate::components::HyphenationConfig::Disabled;
        self
    }

    pub fn child(mut self, child: impl Into<Component>) -> Self {
        self.page.children.push(child.into());
        self
    }

    pub fn children<I, C>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<Component>,
    {
        for child in children {
            self.page.children.push(child.into());
        }
        self
    }

    pub fn push_child(&mut self, child: impl Into<Component>) -> &mut Self {
        self.page.children.push(child.into());
        self
    }

    pub fn build(self) -> Page {
        self.page
    }
}

#[derive(Debug)]
pub struct ViewBuilder {
    view: View,
}

impl ViewBuilder {
    pub fn new() -> Self {
        Self {
            view: View::default(),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.view.style = style;
        self
    }

    pub fn break_before(mut self, break_before: BreakType) -> Self {
        self.view.break_before = break_before;
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.view.id = Some(id.into());
        self
    }

    pub fn fixed(mut self, fixed: bool) -> Self {
        self.view.fixed = fixed;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.view.wrap = Some(wrap);
        self
    }

    pub fn min_presence_ahead(mut self, value: f32) -> Self {
        self.view.min_presence_ahead = value;
        self
    }

    pub fn child(mut self, child: impl Into<Component>) -> Self {
        self.view.children.push(child.into());
        self
    }

    pub fn children<I, C>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<Component>,
    {
        for child in children {
            self.view.children.push(child.into());
        }
        self
    }

    pub fn push_child(&mut self, child: impl Into<Component>) -> &mut Self {
        self.view.children.push(child.into());
        self
    }

    pub fn build(self) -> View {
        self.view
    }
}

#[derive(Debug)]
pub struct TextBuilder {
    text: Text,
}

impl TextBuilder {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            text: Text::new(content),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.text.style = style;
        self
    }

    pub fn bookmark(mut self, bookmark: impl Into<String>) -> Self {
        self.text.bookmark = Some(bookmark.into());
        self
    }

    pub fn break_before(mut self, break_before: BreakType) -> Self {
        self.text.break_before = break_before;
        self
    }

    pub fn fixed(mut self, fixed: bool) -> Self {
        self.text.fixed = fixed;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.text.wrap = Some(wrap);
        self
    }

    pub fn min_presence_ahead(mut self, value: f32) -> Self {
        self.text.min_presence_ahead = value;
        self
    }

    pub fn orphans(mut self, value: usize) -> Self {
        self.text.orphans = value;
        self
    }

    pub fn widows(mut self, value: usize) -> Self {
        self.text.widows = value;
        self
    }

    pub fn span(mut self, content: impl Into<String>, style: Style) -> Self {
        self.ensure_spans();
        self.text.spans.push(TextSpan {
            content: content.into(),
            style,
        });
        self
    }

    pub fn push_span(&mut self, content: impl Into<String>, style: Style) -> &mut Self {
        self.ensure_spans();
        self.text.spans.push(TextSpan {
            content: content.into(),
            style,
        });
        self
    }

    pub fn build(self) -> Text {
        self.text
    }

    fn ensure_spans(&mut self) {
        if self.text.spans.is_empty() && !self.text.content.is_empty() {
            let base_style = self.text.style.clone();
            self.text.spans.push(TextSpan {
                content: std::mem::take(&mut self.text.content),
                style: base_style,
            });
        }
    }
}

#[derive(Debug)]
pub struct ImageBuilder {
    image: Image,
}

impl ImageBuilder {
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            image: Image::new(src),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.image.style = style;
        self
    }

    pub fn object_fit(mut self, fit: ObjectFit) -> Self {
        self.image.object_fit = fit;
        self
    }

    pub fn break_before(mut self, break_before: BreakType) -> Self {
        self.image.break_before = break_before;
        self
    }

    pub fn fixed(mut self, fixed: bool) -> Self {
        self.image.fixed = fixed;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.image.wrap = Some(wrap);
        self
    }

    pub fn min_presence_ahead(mut self, value: f32) -> Self {
        self.image.min_presence_ahead = value;
        self
    }

    pub fn build(self) -> Image {
        self.image
    }
}

#[derive(Debug)]
pub struct LinkBuilder {
    link: Link,
}

impl LinkBuilder {
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            link: Link::new(src),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.link.style = style;
        self
    }

    pub fn break_before(mut self, break_before: BreakType) -> Self {
        self.link.break_before = break_before;
        self
    }

    pub fn fixed(mut self, fixed: bool) -> Self {
        self.link.fixed = fixed;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.link.wrap = Some(wrap);
        self
    }

    pub fn min_presence_ahead(mut self, value: f32) -> Self {
        self.link.min_presence_ahead = value;
        self
    }

    pub fn child(mut self, child: impl Into<Component>) -> Self {
        self.link.children.push(child.into());
        self
    }

    pub fn children<I, C>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<Component>,
    {
        for child in children {
            self.link.children.push(child.into());
        }
        self
    }

    pub fn push_child(&mut self, child: impl Into<Component>) -> &mut Self {
        self.link.children.push(child.into());
        self
    }

    pub fn build(self) -> Link {
        self.link
    }
}

#[derive(Debug)]
pub struct NoteBuilder {
    note: Note,
}

impl NoteBuilder {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            note: Note::new(content),
        }
    }

    pub fn style(mut self, style: Style) -> Self {
        self.note.style = style;
        self
    }

    pub fn break_before(mut self, break_before: BreakType) -> Self {
        self.note.break_before = break_before;
        self
    }

    pub fn fixed(mut self, fixed: bool) -> Self {
        self.note.fixed = fixed;
        self
    }

    pub fn wrap(mut self, wrap: bool) -> Self {
        self.note.wrap = Some(wrap);
        self
    }

    pub fn min_presence_ahead(mut self, value: f32) -> Self {
        self.note.min_presence_ahead = value;
        self
    }

    pub fn build(self) -> Note {
        self.note
    }
}

impl From<PageBuilder> for Page {
    fn from(value: PageBuilder) -> Self {
        value.build()
    }
}

impl From<ViewBuilder> for View {
    fn from(value: ViewBuilder) -> Self {
        value.build()
    }
}

impl From<TextBuilder> for Text {
    fn from(value: TextBuilder) -> Self {
        value.build()
    }
}

impl From<ImageBuilder> for Image {
    fn from(value: ImageBuilder) -> Self {
        value.build()
    }
}

impl From<LinkBuilder> for Link {
    fn from(value: LinkBuilder) -> Self {
        value.build()
    }
}

impl From<NoteBuilder> for Note {
    fn from(value: NoteBuilder) -> Self {
        value.build()
    }
}

impl From<ViewBuilder> for Component {
    fn from(value: ViewBuilder) -> Self {
        Component::View(value.build())
    }
}

impl From<TextBuilder> for Component {
    fn from(value: TextBuilder) -> Self {
        Component::Text(value.build())
    }
}

impl From<ImageBuilder> for Component {
    fn from(value: ImageBuilder) -> Self {
        Component::Image(value.build())
    }
}

impl From<LinkBuilder> for Component {
    fn from(value: LinkBuilder) -> Self {
        Component::Link(value.build())
    }
}

impl From<NoteBuilder> for Component {
    fn from(value: NoteBuilder) -> Self {
        Component::Note(value.build())
    }
}
