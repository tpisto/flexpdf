//! Core document model types for flexpdf.
//! These structs are public so you can build PDFs without XML.

use crate::style::Style;

/// Where to load a font from.
#[derive(Debug, Clone)]
pub enum FontSource {
    /// Google Fonts - name or full URL
    Google(String),
    /// Local file path
    Local(String),
}

/// A font family entry.
///
/// The `family` name is the one you use in styles.
#[derive(Debug, Clone)]
pub struct FontDefinition {
    /// The family name to use in styles (e.g., "Roboto", "MyFont")
    pub family: String,
    /// Where to load the font from
    pub source: FontSource,
}

/// A selection of pages to import from an existing PDF file.
#[derive(Debug, Clone, Default)]
pub struct ImportedPdfPages {
    /// Source PDF path
    pub src: String,
    /// Source PDF bytes for in-memory imports
    pub bytes: Option<Vec<u8>>,
    /// 1-based page numbers to include, in output order
    pub pages: Vec<u32>,
}

impl ImportedPdfPages {
    pub fn new(src: impl Into<String>, pages: impl IntoIterator<Item = u32>) -> Self {
        Self {
            src: src.into(),
            bytes: None,
            pages: pages.into_iter().collect(),
        }
    }

    pub fn from_bytes(bytes: impl Into<Vec<u8>>, pages: impl IntoIterator<Item = u32>) -> Self {
        Self {
            src: String::new(),
            bytes: Some(bytes.into()),
            pages: pages.into_iter().collect(),
        }
    }
}

/// A document section in output order.
#[derive(Debug, Clone)]
pub enum DocumentSection {
    Page(Page),
    ImportPdf(ImportedPdfPages),
}

/// Root document container.
///
/// Add pages to `pages` and optionally set metadata like `title`.
#[derive(Debug, Clone, Default)]
pub struct Document {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub page_mode: Option<String>,
    pub fonts: Vec<FontDefinition>,
    /// Ordered output flow (pages and imported PDF sections)
    pub sections: Vec<DocumentSection>,
    /// Backwards-compatible collection of generated pages
    pub pages: Vec<Page>,
}

impl Document {
    /// Create an empty document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a page to the document.
    pub fn push_page(&mut self, page: Page) {
        self.sections.push(DocumentSection::Page(page.clone()));
        self.pages.push(page);
    }

    /// Append a page and return the document for chaining.
    pub fn with_page(mut self, page: Page) -> Self {
        self.push_page(page);
        self
    }

    /// Append an imported PDF section at the current position.
    pub fn push_import_pdf(&mut self, import: ImportedPdfPages) {
        self.sections.push(DocumentSection::ImportPdf(import));
    }

    /// Append an in-memory imported PDF section at the current position.
    pub fn push_import_pdf_bytes(
        &mut self,
        bytes: impl Into<Vec<u8>>,
        pages: impl IntoIterator<Item = u32>,
    ) {
        self.sections
            .push(DocumentSection::ImportPdf(ImportedPdfPages::from_bytes(
                bytes, pages,
            )));
    }
}

/// Page sizes in points (1 point = 1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    A4,
    Letter,
    Legal,
    Custom(f32, f32),
}

impl PageSize {
    pub fn dimensions(&self) -> (f32, f32) {
        match self {
            PageSize::A4 => (595.28, 841.89),      // 210mm x 297mm
            PageSize::Letter => (612.0, 792.0),    // 8.5" x 11"
            PageSize::Legal => (612.0, 1008.0),    // 8.5" x 14"
            PageSize::Custom(w, h) => (*w, *h),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "A4" => PageSize::A4,
            "LETTER" => PageSize::Letter,
            "LEGAL" => PageSize::Legal,
            _ => PageSize::A4,
        }
    }
}

impl Default for PageSize {
    fn default() -> Self {
        PageSize::A4
    }
}

/// Page orientation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

impl Orientation {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "landscape" => Orientation::Landscape,
            _ => Orientation::Portrait,
        }
    }
}

/// Hyphenation language for text layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HyphenationLang {
    English,
    Finnish,
}

impl HyphenationLang {
    pub fn from_str(value: &str) -> Option<Self> {
        match value.trim().to_lowercase().as_str() {
            "en" | "en-us" | "en_us" | "english" => Some(HyphenationLang::English),
            "fi" | "fi-fi" | "fi_fi" | "finnish" => Some(HyphenationLang::Finnish),
            _ => None,
        }
    }

    pub fn pdf_lang(self) -> &'static str {
        match self {
            HyphenationLang::English => "en-US",
            HyphenationLang::Finnish => "fi-FI",
        }
    }
}

/// Manual page break type.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BreakType {
    #[default]
    None,
    /// Force a page break before this element
    Page,
}

impl BreakType {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "page" | "true" | "1" => BreakType::Page,
            _ => BreakType::None,
        }
    }
}

/// A single page in the document.
#[derive(Debug, Clone)]
pub struct Page {
    pub size: PageSize,
    pub orientation: Orientation,
    pub style: Style,
    pub children: Vec<Component>,
    /// Whether content can wrap to additional pages
    pub wrap: bool,
    /// Optional hyphenation language for this page
    pub hyphenation: Option<HyphenationLang>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            size: PageSize::default(),
            orientation: Orientation::default(),
            style: Style::default(),
            children: Vec::new(),
            // Match react-pdf default: pages wrap unless explicitly disabled.
            wrap: true,
            hyphenation: None,
        }
    }
}

impl Page {
    /// Create a page with a specific size.
    pub fn new(size: PageSize) -> Self {
        let mut page = Self::default();
        page.size = size;
        page
    }

    pub fn dimensions(&self) -> (f32, f32) {
        let (w, h) = self.size.dimensions();
        match self.orientation {
            Orientation::Portrait => (w, h),
            Orientation::Landscape => (h, w),
        }
    }

    /// Append a child component to the page.
    pub fn push(&mut self, child: impl Into<Component>) {
        self.children.push(child.into());
    }

    /// Append a child and return the page for chaining.
    pub fn with_child(mut self, child: impl Into<Component>) -> Self {
        self.children.push(child.into());
        self
    }
}

/// Flexbox container.
#[derive(Debug, Clone, Default)]
pub struct View {
    pub style: Style,
    pub children: Vec<Component>,
    /// Whether to break before this element
    pub break_before: BreakType,
    pub id: Option<String>,
    pub fixed: bool,
    /// Whether this element can be split between pages (default: true)
    pub wrap: Option<bool>,
    /// Desired minimum remaining presence (in points) after this element.
    pub min_presence_ahead: f32,
}

impl View {
    /// Create an empty view.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the style for the view.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Append a child component to the view.
    pub fn push(&mut self, child: impl Into<Component>) {
        self.children.push(child.into());
    }

    /// Append a child and return the view for chaining.
    pub fn with_child(mut self, child: impl Into<Component>) -> Self {
        self.children.push(child.into());
        self
    }
}

/// A styled text segment inside a [`Text`] node.
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub content: String,
    pub style: Style,
}

impl Default for TextSpan {
    fn default() -> Self {
        Self {
            content: String::new(),
            style: Style::default(),
        }
    }
}

/// Text content.
///
/// Use `content` for plain text or `spans` for mixed styling.
#[derive(Debug, Clone)]
pub struct Text {
    /// Plain text content (used when no spans)
    pub content: String,
    /// Inline spans for mixed-style text
    pub spans: Vec<TextSpan>,
    /// Style for the text container (and default for spans)
    pub style: Style,
    /// Bookmark title (used for outlines)
    pub bookmark: Option<String>,
    /// Whether to break before this element
    pub break_before: BreakType,
    /// Whether this element is rendered on all pages (react-pdf `fixed`)
    pub fixed: bool,
    /// Whether this element can be split between pages (default: true)
    pub wrap: Option<bool>,
    /// Desired minimum remaining presence (in points) after this element.
    pub min_presence_ahead: f32,
    /// Minimum number of lines kept at the start of a split text node.
    pub orphans: usize,
    /// Minimum number of lines kept at the end of a split text node.
    pub widows: usize,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            content: String::new(),
            spans: Vec::new(),
            style: Style::default(),
            bookmark: None,
            break_before: BreakType::None,
            fixed: false,
            wrap: None,
            min_presence_ahead: 0.0,
            // Match react-pdf defaults.
            orphans: 2,
            widows: 2,
        }
    }
}

impl Text {
    /// Create a text node with content.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Self::default()
        }
    }

    /// Set the style for the text node.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Check if this text has inline spans
    pub fn has_spans(&self) -> bool {
        !self.spans.is_empty()
    }

    /// Get the full plain text content (concatenates all spans if present)
    pub fn full_text(&self) -> String {
        if self.spans.is_empty() {
            self.content.clone()
        } else {
            self.spans.iter().map(|s| s.content.as_str()).collect()
        }
    }
}

/// How an image should fit within its container.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ObjectFit {
    /// Scale to fill the container, may crop
    Cover,
    /// Scale to fit inside the container, may have letterboxing
    #[default]
    Contain,
    /// Stretch to fill the container exactly
    Fill,
}

impl ObjectFit {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cover" => ObjectFit::Cover,
            "contain" => ObjectFit::Contain,
            "fill" => ObjectFit::Fill,
            _ => ObjectFit::Contain,
        }
    }
}

/// Raster image node.
#[derive(Debug, Clone, Default)]
pub struct Image {
    /// Source URL or file path
    pub src: String,
    /// Object fit mode
    pub object_fit: ObjectFit,
    /// Style (width, height, borders, etc.)
    pub style: Style,
    /// Whether to break before this element
    pub break_before: BreakType,
    /// Whether this element is rendered on all pages (react-pdf `fixed`)
    pub fixed: bool,
    /// Whether this element can be split between pages (ignored for images; always false)
    pub wrap: Option<bool>,
    /// Desired minimum remaining presence (in points) after this element.
    pub min_presence_ahead: f32,
}

impl Image {
    /// Create an image node with the given source.
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            ..Self::default()
        }
    }

    /// Set the style for the image node.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

/// Link node.
#[derive(Debug, Clone, Default)]
pub struct Link {
    pub src: String,
    pub style: Style,
    pub children: Vec<Component>,
    /// Whether to break before this element
    pub break_before: BreakType,
    /// Whether this element is rendered on all pages (react-pdf `fixed`)
    pub fixed: bool,
    /// Whether this element can be split between pages (default: true)
    pub wrap: Option<bool>,
    /// Desired minimum remaining presence (in points) after this element.
    pub min_presence_ahead: f32,
}

impl Link {
    /// Create a link node with the given target.
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            ..Self::default()
        }
    }

    /// Set the style for the link node.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Append a child component to the link.
    pub fn push(&mut self, child: impl Into<Component>) {
        self.children.push(child.into());
    }

    /// Append a child and return the link for chaining.
    pub fn with_child(mut self, child: impl Into<Component>) -> Self {
        self.children.push(child.into());
        self
    }
}

/// Note annotation.
#[derive(Debug, Clone, Default)]
pub struct Note {
    pub content: String,
    pub style: Style,
    /// Whether to break before this element
    pub break_before: BreakType,
    /// Whether this element is rendered on all pages (react-pdf `fixed`)
    pub fixed: bool,
    /// Whether this element can be split between pages (ignored for notes; always false)
    pub wrap: Option<bool>,
    /// Desired minimum remaining presence (in points) after this element.
    pub min_presence_ahead: f32,
}

impl Note {
    /// Create a note annotation.
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            ..Self::default()
        }
    }

    /// Set the style for the note.
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }
}

/// Any renderable node.
#[derive(Debug, Clone)]
pub enum Component {
    View(View),
    Text(Text),
    Image(Image),
    Link(Link),
    Note(Note),
}

impl Component {
    pub fn style(&self) -> &Style {
        match self {
            Component::View(v) => &v.style,
            Component::Text(t) => &t.style,
            Component::Image(i) => &i.style,
            Component::Link(l) => &l.style,
            Component::Note(n) => &n.style,
        }
    }
}

impl From<View> for Component {
    fn from(value: View) -> Self {
        Component::View(value)
    }
}

impl From<Text> for Component {
    fn from(value: Text) -> Self {
        Component::Text(value)
    }
}

impl From<Image> for Component {
    fn from(value: Image) -> Self {
        Component::Image(value)
    }
}

impl From<Link> for Component {
    fn from(value: Link) -> Self {
        Component::Link(value)
    }
}

impl From<Note> for Component {
    fn from(value: Note) -> Self {
        Component::Note(value)
    }
}

impl From<&str> for Component {
    fn from(value: &str) -> Self {
        Component::Text(Text::new(value))
    }
}

impl From<String> for Component {
    fn from(value: String) -> Self {
        Component::Text(Text::new(value))
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Text::new(value)
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        Text::new(value)
    }
}
