//! PDF writer that serializes objects and streams.

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

use super::{ObjectRef, PdfObject};

/// PDF writer handles serialization.
pub struct PdfWriter {
    buffer: Vec<u8>,
    objects: Vec<(ObjectRef, usize)>, // (ref, byte offset)
    next_id: u32,
}

impl PdfWriter {
    pub fn new() -> Self {
        let mut writer = Self {
            buffer: Vec::with_capacity(65536),
            objects: Vec::new(),
            next_id: 1,
        };

        // Write PDF header
        writer.buffer.extend_from_slice(b"%PDF-1.7\n");
        // Binary marker (high bytes to indicate binary content)
        writer.buffer.extend_from_slice(b"%\xE2\xE3\xCF\xD3\n");

        writer
    }

    /// Reserve an object ID without writing.
    pub fn reserve_id(&mut self) -> ObjectRef {
        let id = ObjectRef(self.next_id);
        self.next_id += 1;
        id
    }

    /// Write an object and return its reference.
    pub fn write_object(&mut self, obj: &PdfObject) -> ObjectRef {
        let id = self.reserve_id();
        let offset = self.buffer.len();
        self.objects.push((id, offset));

        write!(self.buffer, "{} 0 obj\n", id.0).unwrap();
        obj.write(&mut self.buffer);
        self.buffer.extend_from_slice(b"\nendobj\n");

        id
    }

    /// Write an object at a reserved ID.
    pub fn write_object_at(&mut self, id: ObjectRef, obj: &PdfObject) {
        let offset = self.buffer.len();
        self.objects.push((id, offset));

        write!(self.buffer, "{} 0 obj\n", id.0).unwrap();
        obj.write(&mut self.buffer);
        self.buffer.extend_from_slice(b"\nendobj\n");
    }

    /// Write a stream object.
    pub fn write_stream(&mut self, data: &[u8], compress: bool) -> ObjectRef {
        let id = self.reserve_id();
        let offset = self.buffer.len();
        self.objects.push((id, offset));

        write!(self.buffer, "{} 0 obj\n", id.0).unwrap();

        if compress {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(data).unwrap();
            let compressed = encoder.finish().unwrap();

            write!(
                self.buffer,
                "<< /Length {} /Filter /FlateDecode >>\n",
                compressed.len()
            )
            .unwrap();
            self.buffer.extend_from_slice(b"stream\n");
            self.buffer.extend_from_slice(&compressed);
            self.buffer.extend_from_slice(b"\nendstream\n");
        } else {
            write!(self.buffer, "<< /Length {} >>\n", data.len()).unwrap();
            self.buffer.extend_from_slice(b"stream\n");
            self.buffer.extend_from_slice(data);
            self.buffer.extend_from_slice(b"\nendstream\n");
        }

        self.buffer.extend_from_slice(b"endobj\n");

        id
    }

    /// Write an image stream object with custom dictionary (for XObjects).
    pub fn write_image_stream(&mut self, data: &[u8], dict: PdfObject) -> ObjectRef {
        let id = self.reserve_id();
        let offset = self.buffer.len();
        self.objects.push((id, offset));

        write!(self.buffer, "{} 0 obj\n", id.0).unwrap();

        // Compress the image data
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        let compressed = encoder.finish().unwrap();

        // Write the dictionary with Length and Filter added
        if let PdfObject::Dictionary(entries) = dict {
            self.buffer.extend_from_slice(b"<<\n");
            for (key, value) in &entries {
                self.buffer.extend_from_slice(b"  /");
                self.buffer.extend_from_slice(key.as_bytes());
                self.buffer.push(b' ');
                value.write(&mut self.buffer);
                self.buffer.push(b'\n');
            }
            // Add Length and Filter
            write!(self.buffer, "  /Length {}\n", compressed.len()).unwrap();
            self.buffer.extend_from_slice(b"  /Filter /FlateDecode\n");
            self.buffer.extend_from_slice(b">>\n");
        }

        self.buffer.extend_from_slice(b"stream\n");
        self.buffer.extend_from_slice(&compressed);
        self.buffer.extend_from_slice(b"\nendstream\n");
        self.buffer.extend_from_slice(b"endobj\n");

        id
    }

    /// Finalize PDF with catalog and cross-reference table.
    pub fn finish(mut self, catalog_ref: ObjectRef, info_ref: Option<ObjectRef>) -> Vec<u8> {
        // Write cross-reference table
        let xref_offset = self.buffer.len();

        self.buffer.extend_from_slice(b"xref\n");
        write!(self.buffer, "0 {}\n", self.next_id).unwrap();

        // Object 0 is always free
        self.buffer
            .extend_from_slice(b"0000000000 65535 f \n");

        // Sort objects by ID
        self.objects.sort_by_key(|(id, _)| id.0);

        // Write entries
        let mut expected_id = 1;
        for (id, offset) in &self.objects {
            while expected_id < id.0 {
                self.buffer
                    .extend_from_slice(b"0000000000 65535 f \n");
                expected_id += 1;
            }
            write!(self.buffer, "{:010} {:05} n \n", offset, 0).unwrap();
            expected_id += 1;
        }

        // Write trailer
        self.buffer.extend_from_slice(b"trailer\n");
        self.buffer.extend_from_slice(b"<<\n");
        write!(self.buffer, "  /Size {}\n", self.next_id).unwrap();
        write!(self.buffer, "  /Root {} 0 R\n", catalog_ref.0).unwrap();
        if let Some(info) = info_ref {
            write!(self.buffer, "  /Info {} 0 R\n", info.0).unwrap();
        }
        self.buffer.extend_from_slice(b">>\n");
        self.buffer.extend_from_slice(b"startxref\n");
        write!(self.buffer, "{}\n", xref_offset).unwrap();
        self.buffer.extend_from_slice(b"%%EOF\n");

        self.buffer
    }
}
