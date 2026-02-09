//! flexpdf CLI - render XML to PDF

use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let input_path = if args.len() > 1 {
        &args[1]
    } else {
        "test_page.xml"
    };

    let output_path = if args.len() > 2 {
        args[2].clone()
    } else {
        input_path.replace(".xml", ".pdf")
    };

    println!("Reading: {}", input_path);

    let xml = match fs::read_to_string(input_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };

    println!("Parsing XML...");

    let doc = match flexpdf::parse_xml(&xml) {
        Ok(doc) => doc,
        Err(e) => {
            eprintln!("Error parsing XML: {}", e);
            std::process::exit(1);
        }
    };

    println!(
        "Document: {:?}, {} page(s)",
        doc.title.as_deref().unwrap_or("Untitled"),
        doc.pages.len()
    );

    println!("Rendering PDF...");

    let pdf_bytes = match flexpdf::render_document(&doc) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Error rendering PDF: {}", e);
            std::process::exit(1);
        }
    };

    println!("Writing: {} ({} bytes)", output_path, pdf_bytes.len());

    if let Err(e) = fs::write(&output_path, &pdf_bytes) {
        eprintln!("Error writing file: {}", e);
        std::process::exit(1);
    }

    println!("Done!");
}
