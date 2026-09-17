//! PDF sources through Pdfium: how many pages a file has, how much text
//! and how many pictures are on them, and a picture cut out of a page so
//! a generated card can carry it. The library is not linked into the
//! binary; [`load`] binds it at boot from the path `FLASH_PDFIUM_PATH`
//! names, so a server without it still runs with the `pdf` input off.
//!
//! Pdfium is one process-wide library. [`load`] therefore works once per
//! process (a second call reports that the bindings are already in
//! place), and every call into it is serialized by the crate's
//! `thread_safe` bindings, which is also what makes [`Pdf`] `Send`. Each
//! method parses its bytes from scratch and holds the document only for
//! the duration of the call: nothing here is fast, so callers run it
//! inside `spawn_blocking`.
#![cfg(feature = "pdf")]

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageFormat};
use pdfium_render::prelude::*;

/// Pages past this many are not opened by [`Pdf::stats`]; the count is
/// then a floor, which is all a preflight needs to refuse a book.
const STATS_PAGE_BOUND: u32 = 200;

/// A figure is cut from the page rendered at this resolution.
const FIGURE_DPI: u32 = 150;

/// When the PNG at [`FIGURE_DPI`] is over the size limit, one retry at
/// this resolution before giving up.
const FIGURE_FALLBACK_DPI: u32 = 100;

/// A crop narrower or shorter than this is a stray mark, not a figure.
const MIN_FIGURE_PX: u32 = 64;

/// The largest PNG a figure may become; it is stored as ordinary media.
const MAX_FIGURE_PNG_BYTES: usize = 2 * 1024 * 1024;

/// The longest edge a page render may have at any requested resolution,
/// so a poster-sized page does not turn into a gigabyte of bitmap.
const MAX_RENDER_PX: u32 = 4096;

/// The Pdfium library, bound once at boot.
pub struct Pdf {
    pdfium: Pdfium,
}

/// What a preflight tells the user about a PDF before any model sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfStats {
    pub pages: u32,
    /// Whitespace-separated runs of text over the first
    /// [`STATS_PAGE_BOUND`] pages.
    pub words: u32,
    /// Image objects over the same pages: pictures, not vector drawings.
    pub images: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum PdfError {
    #[error("the PDF is password-protected")]
    Encrypted,
    #[error("the file is not a readable PDF")]
    Malformed,
    #[error("page {0} is not in the document")]
    NoSuchPage(u32),
    #[error("the figure is too small to use ({0} by {1} pixels)")]
    TooSmall(u32, u32),
    #[error("the figure is too large to store")]
    TooLarge,
    /// Pdfium's own account of what went wrong; for the log, not the user.
    #[error("pdfium: {0}")]
    Render(String),
}

impl PdfError {
    /// The sentence the user sees. Pdfium's internals stay in the log.
    pub fn user_message(&self) -> String {
        match self {
            PdfError::Encrypted => {
                "This PDF is password-protected. Remove the password and upload it again."
                    .to_string()
            }
            PdfError::Malformed => "This file could not be read as a PDF.".to_string(),
            PdfError::NoSuchPage(page) => format!("The PDF has no page {page}."),
            PdfError::TooSmall(w, h) => {
                format!("That figure is too small to use ({w} by {h} pixels).")
            }
            PdfError::TooLarge => "That figure is too large to store as an image.".to_string(),
            PdfError::Render(_) => "This PDF could not be rendered.".to_string(),
        }
    }
}

impl From<PdfiumError> for PdfError {
    fn from(err: PdfiumError) -> Self {
        match err {
            PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::PasswordError | PdfiumInternalError::SecurityError,
            ) => PdfError::Encrypted,
            PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::FormatError | PdfiumInternalError::FileError,
            ) => PdfError::Malformed,
            other => PdfError::Render(other.to_string()),
        }
    }
}

/// Binds Pdfium from `path`: the library file itself, or the directory
/// that holds it under its platform name (`libpdfium.so`,
/// `libpdfium.dylib`, `pdfium.dll`). The error is a sentence naming the
/// path, ready for a boot message.
pub fn load(path: &Path) -> Result<Pdf, String> {
    let file = if path.is_dir() {
        Pdfium::pdfium_platform_library_name_at_path(path)
    } else {
        path.to_path_buf()
    };
    let bindings = Pdfium::bind_to_library(&file).map_err(|err| {
        format!(
            "the Pdfium library could not be loaded from {} ({err}); fetch it with \
             tools/fetch-pdfium and point FLASH_PDFIUM_PATH at it, or drop pdf from \
             FLASH_AI_INPUTS",
            file.display()
        )
    })?;
    Ok(Pdf {
        pdfium: Pdfium::new(bindings),
    })
}

impl Pdf {
    /// How many pages the file has. Encrypted and broken files are errors.
    pub fn page_count(&self, bytes: &[u8]) -> Result<u32, PdfError> {
        let doc = self.open(bytes)?;
        Ok(page_count(&doc))
    }

    /// Pages, words and pictures, the words and pictures counted over the
    /// first [`STATS_PAGE_BOUND`] pages only.
    pub fn stats(&self, bytes: &[u8]) -> Result<PdfStats, PdfError> {
        let doc = self.open(bytes)?;
        let pages = page_count(&doc);
        let mut words: u32 = 0;
        let mut images: u32 = 0;
        for index in 0..pages.min(STATS_PAGE_BOUND) {
            let page = doc.pages().get(page_index(index))?;
            let text = page.text()?.all();
            words = words.saturating_add(count_u32(text.split_whitespace().count()));
            let pictures = page
                .objects()
                .iter()
                .filter(|object| matches!(object.object_type(), PdfPageObjectType::Image))
                .count();
            images = images.saturating_add(count_u32(pictures));
        }
        Ok(PdfStats {
            pages,
            words,
            images,
        })
    }

    /// The whole of page `page` (1-based, the number a reader sees) as a
    /// PNG at `dpi`, the longest edge capped at [`MAX_RENDER_PX`].
    pub fn render_page_png(&self, bytes: &[u8], page: u32, dpi: u32) -> Result<Vec<u8>, PdfError> {
        let doc = self.open(bytes)?;
        let image = render(&doc, page, dpi)?;
        encode_png(&image)
    }

    /// The part of page `page` (1-based) inside `bbox`, given as
    /// `[x0, y0, x1, y1]` fractions of the page with the origin at the
    /// top left, as a PNG. Rendered at [`FIGURE_DPI`]; a crop under
    /// [`MIN_FIGURE_PX`] on either side is [`PdfError::TooSmall`], and one
    /// whose PNG is still over [`MAX_FIGURE_PNG_BYTES`] after a retry at
    /// [`FIGURE_FALLBACK_DPI`] is [`PdfError::TooLarge`].
    pub fn crop_figure_png(
        &self,
        bytes: &[u8],
        page: u32,
        bbox: [f32; 4],
    ) -> Result<Vec<u8>, PdfError> {
        let doc = self.open(bytes)?;
        let bbox = clamp_bbox(bbox);

        let full = render(&doc, page, FIGURE_DPI)?;
        let crop = crop_box(full.width(), full.height(), bbox);
        if crop.width < MIN_FIGURE_PX || crop.height < MIN_FIGURE_PX {
            return Err(PdfError::TooSmall(crop.width, crop.height));
        }
        let png = encode_png(&full.crop_imm(crop.x, crop.y, crop.width, crop.height))?;
        if png.len() <= MAX_FIGURE_PNG_BYTES {
            return Ok(png);
        }

        // Too heavy at 150 dpi: a photograph or a dense scan. Once more at
        // a resolution that still reads on a card.
        let full = render(&doc, page, FIGURE_FALLBACK_DPI)?;
        let crop = crop_box(full.width(), full.height(), bbox);
        let png = encode_png(&full.crop_imm(crop.x, crop.y, crop.width, crop.height))?;
        if png.len() <= MAX_FIGURE_PNG_BYTES {
            Ok(png)
        } else {
            Err(PdfError::TooLarge)
        }
    }

    fn open<'a>(&'a self, bytes: &'a [u8]) -> Result<PdfDocument<'a>, PdfError> {
        Ok(self.pdfium.load_pdf_from_byte_slice(bytes, None)?)
    }
}

fn page_count(doc: &PdfDocument<'_>) -> u32 {
    u32::try_from(doc.pages().len()).unwrap_or(0)
}

/// A zero-based page index in Pdfium's own integer type. The caller has
/// already checked it against the page count.
fn page_index(index: u32) -> PdfPageIndex {
    PdfPageIndex::try_from(index).unwrap_or(PdfPageIndex::MAX)
}

fn count_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Page `page` (1-based) rasterized at `dpi`, aspect ratio kept, longest
/// edge capped at [`MAX_RENDER_PX`], as an opaque RGB image.
fn render(doc: &PdfDocument<'_>, page: u32, dpi: u32) -> Result<DynamicImage, PdfError> {
    let count = page_count(doc);
    if page == 0 || page > count {
        return Err(PdfError::NoSuchPage(page));
    }
    let page = doc.pages().get(page_index(page - 1))?;
    let width_pt = page.width().value.max(1.0);
    let height_pt = page.height().value.max(1.0);
    let mut scale = dpi.max(1) as f32 / 72.0;
    let longest = width_pt.max(height_pt) * scale;
    if longest > MAX_RENDER_PX as f32 {
        scale *= MAX_RENDER_PX as f32 / longest;
    }
    let target_width = (width_pt * scale).round().max(1.0) as i32;
    let config = PdfRenderConfig::new().set_target_width(target_width);
    let bitmap = page.render_with_config(&config)?;
    // Pdfium hands back BGRA; the PNG wants no alpha channel and the
    // page background is already painted white.
    Ok(DynamicImage::ImageRgb8(bitmap.as_image()?.to_rgb8()))
}

fn encode_png(image: &DynamicImage) -> Result<Vec<u8>, PdfError> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|err| PdfError::Render(format!("png encode: {err}")))?;
    Ok(out.into_inner())
}

/// Each edge inside `[0, 1]`, NaN read as the page edge, and the corners
/// in order so `x0 <= x1` and `y0 <= y1`.
fn clamp_bbox([x0, y0, x1, y1]: [f32; 4]) -> [f32; 4] {
    let unit = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
    let (x0, x1) = (unit(x0), unit(x1));
    let (y0, y1) = (unit(y0), unit(y1));
    [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)]
}

struct CropBox {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

/// The pixel rectangle a normalized box covers on a `width` by `height`
/// render, grown outward to whole pixels and kept inside the image.
fn crop_box(width: u32, height: u32, [x0, y0, x1, y1]: [f32; 4]) -> CropBox {
    let w = width as f32;
    let h = height as f32;
    let x = ((x0 * w).floor().max(0.0) as u32).min(width);
    let y = ((y0 * h).floor().max(0.0) as u32).min(height);
    let right = ((x1 * w).ceil().max(0.0) as u32).min(width);
    let bottom = ((y1 * h).ceil().max(0.0) as u32).min(height);
    CropBox {
        x,
        y,
        width: right.saturating_sub(x),
        height: bottom.saturating_sub(y),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbox_is_clamped_and_ordered() {
        assert_eq!(clamp_bbox([1.5, -0.2, 0.25, 0.75]), [0.25, 0.0, 1.0, 0.75]);
        assert_eq!(clamp_bbox([f32::NAN, 0.5, 0.5, 0.5]), [0.0, 0.5, 0.5, 0.5]);
    }

    #[test]
    fn crop_box_grows_to_whole_pixels_inside_the_image() {
        let c = crop_box(1000, 500, [0.1004, 0.2, 0.5, 1.0]);
        assert_eq!((c.x, c.y, c.width, c.height), (100, 100, 400, 400));
        let full = crop_box(1000, 500, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!((full.x, full.y, full.width, full.height), (0, 0, 1000, 500));
        let empty = crop_box(1000, 500, [0.5, 0.5, 0.5, 0.5]);
        assert_eq!((empty.width, empty.height), (0, 0));
    }

    #[test]
    fn user_messages_name_no_internals() {
        let msg = PdfError::Render("FPDF_LoadMemDocument64 returned null".into()).user_message();
        assert!(!msg.contains("FPDF"));
        assert!(PdfError::NoSuchPage(7).user_message().contains('7'));
    }
}
