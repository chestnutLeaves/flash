//! The Pdfium bridge against a real library. Pdfium is not part of the
//! build, so these tests run only where `FLASH_PDFIUM_PATH` names it
//! (fetch one with tools/fetch-pdfium); elsewhere they say so and pass.
//! One process binds the library once, hence one test.
#![cfg(feature = "pdf")]

use std::path::PathBuf;

use flash_server::ai::pdf::{Pdf, PdfError};

/// A two-page PDF with a line of Helvetica on each page, assembled with a
/// correct cross-reference table so nothing depends on Pdfium's repair.
fn tiny_pdf() -> Vec<u8> {
    let page = |contents: u32| {
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents {contents} 0 R \
             /Resources << /Font << /F1 7 0 R >> >> >>"
        )
    };
    let stream = |text: &str| {
        let body = format!("BT /F1 24 Tf 20 100 Td ({text}) Tj ET");
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len())
    };
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_string(),
        page(4),
        stream("Hello flash cards"),
        page(6),
        stream("Second page has more words"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];

    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{body}\nendobj\n", i + 1));
    }
    let xref_at = out.len();
    out.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        out.push_str(&format!("{offset:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
        objects.len() + 1
    ));
    out.into_bytes()
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[test]
fn counts_words_and_crops_a_figure_from_a_real_library() {
    let Some(path) = std::env::var_os("FLASH_PDFIUM_PATH") else {
        println!("FLASH_PDFIUM_PATH unset; skipping the Pdfium tests");
        return;
    };
    let pdf: Pdf = flash_server::ai::pdf::load(&PathBuf::from(path)).expect("Pdfium loads");
    let bytes = tiny_pdf();

    assert_eq!(pdf.page_count(&bytes).unwrap(), 2);

    let stats = pdf.stats(&bytes).unwrap();
    assert_eq!(stats.pages, 2);
    assert_eq!(stats.words, 8, "three words on page one, five on page two");
    assert_eq!(stats.images, 0);

    let png = pdf
        .crop_figure_png(&bytes, 1, [0.0, 0.0, 1.0, 1.0])
        .unwrap();
    assert_eq!(&png[..8], &PNG_SIGNATURE);
    // 300 by 200 points at 150 dpi is 625 by 417 pixels; the PNG of a
    // mostly white page is a few kilobytes.
    assert!(
        png.len() > 100 && png.len() < 200_000,
        "png is {} bytes",
        png.len()
    );

    let whole_page = pdf.render_page_png(&bytes, 2, 72).unwrap();
    assert_eq!(&whole_page[..8], &PNG_SIGNATURE);

    // A sliver of the page is not a figure.
    assert!(matches!(
        pdf.crop_figure_png(&bytes, 1, [0.5, 0.5, 0.52, 0.52]),
        Err(PdfError::TooSmall(_, _))
    ));
    assert!(matches!(
        pdf.crop_figure_png(&bytes, 3, [0.0, 0.0, 1.0, 1.0]),
        Err(PdfError::NoSuchPage(3))
    ));
    assert!(matches!(
        pdf.page_count(b"%PDF-1.4 not really"),
        Err(PdfError::Malformed)
    ));

    let inset = pdf
        .crop_figure_png(&bytes, 1, [0.1, 0.1, 0.9, 0.9])
        .unwrap();
    assert_eq!(&inset[..8], &PNG_SIGNATURE);
}
