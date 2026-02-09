#[derive(Debug)]
pub enum ParseError {
    XmlError(quick_xml::Error),
    Utf8Error(std::str::Utf8Error),
    AttrError(String),
    MissingDocument,
    UnexpectedEof,
}

impl From<quick_xml::Error> for ParseError {
    fn from(e: quick_xml::Error) -> Self {
        ParseError::XmlError(e)
    }
}

impl From<std::str::Utf8Error> for ParseError {
    fn from(e: std::str::Utf8Error) -> Self {
        ParseError::Utf8Error(e)
    }
}

impl From<quick_xml::events::attributes::AttrError> for ParseError {
    fn from(e: quick_xml::events::attributes::AttrError) -> Self {
        ParseError::AttrError(format!("{:?}", e))
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::XmlError(e) => write!(f, "XML error: {}", e),
            ParseError::Utf8Error(e) => write!(f, "UTF-8 error: {}", e),
            ParseError::AttrError(e) => write!(f, "Attribute error: {}", e),
            ParseError::MissingDocument => write!(f, "Missing Document element"),
            ParseError::UnexpectedEof => write!(f, "Unexpected end of file"),
        }
    }
}

impl std::error::Error for ParseError {}
