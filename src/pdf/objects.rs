//! PDF object types and dictionary helpers.

use std::io::Write;

use super::util::format_number;

/// PDF object reference (object number).
#[derive(Debug, Clone, Copy)]
pub struct ObjectRef(pub u32);

/// PDF object types.
#[derive(Debug, Clone)]
pub enum PdfObject {
    Null,
    Boolean(bool),
    Integer(i64),
    Real(f64),
    Name(String),
    String(String),
    Array(Vec<PdfObject>),
    Dictionary(Vec<(String, PdfObject)>),
    Reference(ObjectRef),
}

impl PdfObject {
    pub fn write(&self, buf: &mut Vec<u8>) {
        match self {
            PdfObject::Null => buf.extend_from_slice(b"null"),
            PdfObject::Boolean(b) => {
                buf.extend_from_slice(if *b { b"true" } else { b"false" })
            }
            PdfObject::Integer(i) => write!(buf, "{}", i).unwrap(),
            PdfObject::Real(r) => {
                let s = format_number(*r);
                buf.extend_from_slice(s.as_bytes());
            }
            PdfObject::Name(n) => {
                buf.push(b'/');
                // Escape special characters in name
                for ch in n.chars() {
                    match ch {
                        '#' => buf.extend_from_slice(b"#23"),
                        ' ' => buf.extend_from_slice(b"#20"),
                        '(' | ')' | '<' | '>' | '[' | ']' | '{' | '}' | '/' | '%' => {
                            write!(buf, "#{:02X}", ch as u8).unwrap();
                        }
                        c if c.is_ascii() && !c.is_ascii_control() => buf.push(c as u8),
                        c => write!(buf, "#{:02X}", c as u32).unwrap(),
                    }
                }
            }
            PdfObject::String(s) => {
                buf.push(b'(');
                for ch in s.bytes() {
                    match ch {
                        b'(' => buf.extend_from_slice(b"\\("),
                        b')' => buf.extend_from_slice(b"\\)"),
                        b'\\' => buf.extend_from_slice(b"\\\\"),
                        c => buf.push(c),
                    }
                }
                buf.push(b')');
            }
            PdfObject::Array(arr) => {
                buf.push(b'[');
                for (i, item) in arr.iter().enumerate() {
                    if i > 0 {
                        buf.push(b' ');
                    }
                    item.write(buf);
                }
                buf.push(b']');
            }
            PdfObject::Dictionary(dict) => {
                buf.extend_from_slice(b"<<\n");
                for (key, value) in dict {
                    buf.extend_from_slice(b"  /");
                    buf.extend_from_slice(key.as_bytes());
                    buf.push(b' ');
                    value.write(buf);
                    buf.push(b'\n');
                }
                buf.extend_from_slice(b">>");
            }
            PdfObject::Reference(r) => {
                write!(buf, "{} 0 R", r.0).unwrap();
            }
        }
    }
}

/// Helper to build dictionaries.
pub struct DictBuilder {
    entries: Vec<(String, PdfObject)>,
}

impl DictBuilder {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn entry(mut self, key: &str, value: PdfObject) -> Self {
        self.entries.push((key.to_string(), value));
        self
    }

    pub fn build(self) -> PdfObject {
        PdfObject::Dictionary(self.entries)
    }
}
