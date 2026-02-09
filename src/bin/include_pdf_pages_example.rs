use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use flexpdf::builder::{document, text, view};
use flexpdf::{render_document, Color, PageSize, Style};

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {}", error);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Usage: cargo run --bin include_pdf_pages_example -- <source.pdf> <output.pdf> [customer]"
        );
        std::process::exit(1);
    }

    let source_pdf = args[1].clone();
    let output_pdf = args[2].clone();
    let customer = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "Acme Corporation".to_string());

    let run_id = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let source_pdf_bytes = std::fs::read(&source_pdf)?;
    let metrics = [
        ("New users", 3421),
        ("Active users", 8150),
        ("Conversion rate", 24),
    ];
    let total_score: i32 = metrics.iter().map(|(_, value)| *value).sum();

    let mut metric_lines = Vec::new();
    for (label, value) in metrics {
        metric_lines.push(
            text(format!("{}: {}", label, value)).style(Style {
                font_size: Some(12.0),
                color: Some(color("#334155")),
                ..Style::default()
            }),
        );
    }

    let doc = document()
        .title("Imported + Dynamic Example")
        .author("flexpdf example")
        .import_pdf_bytes_pages(source_pdf_bytes, [1, 2])
        .page_with(PageSize::A4, |page| {
            page.child(
                view()
                    .style(Style {
                        padding: Some(36.0),
                        gap: Some(12.0),
                        ..Style::default()
                    })
                    .child(text("Dynamic Summary").style(Style {
                        font_size: Some(24.0),
                        font_weight: Some(700),
                        color: Some(color("#0f172a")),
                        ..Style::default()
                    }))
                    .child(text(format!("Customer: {}", customer)).style(Style {
                        font_size: Some(14.0),
                        color: Some(color("#334155")),
                        ..Style::default()
                    }))
                    .child(text(format!("Run ID: {}", run_id)).style(Style {
                        font_size: Some(12.0),
                        color: Some(color("#64748b")),
                        ..Style::default()
                    }))
                    .children(metric_lines),
            )
        })
        .page_with(PageSize::A4, |page| {
            page.child(
                view()
                    .style(Style {
                        padding: Some(36.0),
                        gap: Some(10.0),
                        ..Style::default()
                    })
                    .child(text("Dynamic Details").style(Style {
                        font_size: Some(22.0),
                        font_weight: Some(700),
                        color: Some(color("#0f172a")),
                        ..Style::default()
                    }))
                    .child(text("This page is generated at runtime from Rust data.").style(
                        Style {
                            font_size: Some(12.0),
                            color: Some(color("#334155")),
                            ..Style::default()
                        },
                    ))
                    .child(text(format!("Aggregate score: {}", total_score)).style(Style {
                        font_size: Some(14.0),
                        font_weight: Some(600),
                        color: Some(color("#1d4ed8")),
                        ..Style::default()
                    })),
            )
        })
        .page_with(PageSize::A4, |page| {
            page.child(
                view()
                    .style(Style {
                        padding: Some(36.0),
                        gap: Some(10.0),
                        ..Style::default()
                    })
                    .child(text("New Page 1").style(Style {
                        font_size: Some(24.0),
                        font_weight: Some(700),
                        color: Some(color("#0f172a")),
                        ..Style::default()
                    }))
                    .child(text("Appended page after the dynamic section.").style(Style {
                        font_size: Some(12.0),
                        color: Some(color("#334155")),
                        ..Style::default()
                    })),
            )
        })
        .page_with(PageSize::A4, |page| {
            page.child(
                view()
                    .style(Style {
                        padding: Some(36.0),
                        gap: Some(10.0),
                        ..Style::default()
                    })
                    .child(text("New Page 2").style(Style {
                        font_size: Some(24.0),
                        font_weight: Some(700),
                        color: Some(color("#0f172a")),
                        ..Style::default()
                    }))
                    .child(text("Final page in the output PDF.").style(Style {
                        font_size: Some(12.0),
                        color: Some(color("#334155")),
                        ..Style::default()
                    })),
            )
        })
        .build();

    let pdf_bytes = render_document(&doc)?;
    std::fs::write(&output_pdf, pdf_bytes)?;
    println!("Wrote {}", output_pdf);

    Ok(())
}

fn color(hex: &str) -> Color {
    Color::from_hex(hex).unwrap_or_else(Color::black)
}
