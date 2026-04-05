//! File output module — generate TXT, Markdown, and PDF files from agent content.
//!
//! Supported formats:
//!   "txt"  — plain UTF-8 text file
//!   "md"   — Markdown file (same as txt, different extension)
//!   "pdf"  — single-page PDF via printpdf (long content is paginated)

use std::path::{Path, PathBuf};
use crate::error::{AppError, AppResult};

/// Generate a file from `content` and return the absolute path where it was saved.
///
/// `format` must be "txt", "md", or "pdf".
/// `output_dir` is the directory to write into (created if it doesn't exist).
pub fn generate(
    content: &str,
    filename: &str,
    format: &str,
    output_dir: &Path,
) -> AppResult<PathBuf> {
    std::fs::create_dir_all(output_dir).map_err(AppError::Io)?;

    // Sanitise filename — strip path separators
    let safe_name: String = filename
        .chars()
        .map(|c| if c == '/' || c == '\\' || c == ':' { '_' } else { c })
        .collect();

    let ext = match format {
        "txt"  => "txt",
        "md"   => "md",
        "pdf"  => "pdf",
        other  => return Err(AppError::InvalidInput(
            format!("Unsupported file format: '{other}'. Use txt, md, or pdf.")
        )),
    };

    let path = output_dir.join(format!("{safe_name}.{ext}"));

    match format {
        "pdf" => generate_pdf(content, &safe_name, &path)?,
        _     => std::fs::write(&path, content.as_bytes()).map_err(AppError::Io)?,
    }

    Ok(path)
}

// ── PDF generation ─────────────────────────────────────────────────────────

fn generate_pdf(content: &str, title: &str, path: &Path) -> AppResult<()> {
    use printpdf::*;
    use std::fs::File;
    use std::io::BufWriter;

    // A4 page dimensions
    let page_w = Mm(210.0);
    let page_h = Mm(297.0);
    let margin_left = Mm(20.0);
    let margin_top  = 277.0_f64; // mm from bottom (page_h - 20 mm top margin)
    let font_size   = 10.0_f64;
    let line_h      = 5.5_f64;  // mm per line
    let usable_h    = margin_top - 20.0_f64; // stop at 20 mm from bottom

    let (doc, first_page, first_layer) =
        PdfDocument::new(title, page_w, page_h, "Content");

    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| AppError::Connector(e.to_string()))?;

    let mut current_layer = doc.get_page(first_page).get_layer(first_layer);
    let mut y = margin_top;
    let mut page_num = 0usize;

    // Wrap text to fit the page width (~170 mm at 10pt ≈ 95 chars)
    let wrapped = wrap_text(content, 95);

    for line in &wrapped {
        if y < 20.0 {
            // Add a new page
            page_num += 1;
            let (new_page, new_layer) = doc.add_page(page_w, page_h, format!("Page {}", page_num + 1));
            current_layer = doc.get_page(new_page).get_layer(new_layer);
            y = usable_h;
        }
        current_layer.use_text(line.as_str(), font_size, margin_left, Mm(y), &font);
        y -= line_h;
    }

    let file = File::create(path).map_err(AppError::Io)?;
    doc.save(&mut BufWriter::new(file))
        .map_err(|e| AppError::Connector(e.to_string()))?;

    Ok(())
}

// ── Text utilities ─────────────────────────────────────────────────────────

/// Word-wrap `text` to at most `max_chars` per line.
/// Preserves existing newlines; splits long words at max_chars.
fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    let mut result = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            result.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if line.is_empty() {
                // Word may itself be longer than max_chars
                if word.len() > max_chars {
                    let mut w = word;
                    while w.len() > max_chars {
                        result.push(w[..max_chars].to_string());
                        w = &w[max_chars..];
                    }
                    line = w.to_string();
                } else {
                    line = word.to_string();
                }
            } else if line.len() + 1 + word.len() <= max_chars {
                line.push(' ');
                line.push_str(word);
            } else {
                result.push(line.clone());
                line = word.to_string();
            }
        }
        if !line.is_empty() {
            result.push(line);
        }
    }
    result
}
