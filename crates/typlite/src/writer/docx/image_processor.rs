//! Image processing functionality for DOCX conversion

use base64::Engine;
use docx_rs::*;
use std::io::Cursor;

use crate::Result;

/// Image processor for DOCX documents
pub struct DocxImageProcessor;

impl DocxImageProcessor {
    /// Create a new image processor
    pub fn new() -> Self {
        Self
    }

    /// Convert SVG data to PNG format
    pub fn convert_svg_to_png(&self, svg_data: &[u8]) -> Result<Vec<u8>> {
        // Check if data is valid SVG
        let svg_str = match std::str::from_utf8(svg_data) {
            Ok(s) => s,
            Err(_) => return Err("Unable to parse input data as UTF-8 string".into()),
        };

        let dpi = 300.0;
        let scale_factor = dpi / 96.0;

        let opt = resvg::usvg::Options {
            dpi,
            ..resvg::usvg::Options::default()
        };

        // Parse SVG
        let rtree = match resvg::usvg::Tree::from_str(svg_str, &opt) {
            Ok(tree) => tree,
            Err(e) => return Err(format!("SVG parsing error: {e:?}").into()),
        };

        let size = rtree.size().to_int_size();
        let width = (size.width() as f32 * scale_factor) as u32;
        let height = (size.height() as f32 * scale_factor) as u32;

        // Create pixel buffer
        let mut pixmap = match resvg::tiny_skia::Pixmap::new(width, height) {
            Some(pixmap) => pixmap,
            None => return Err("Unable to create pixel buffer".into()),
        };

        // Render SVG to pixel buffer
        resvg::render(
            &rtree,
            resvg::tiny_skia::Transform::from_scale(scale_factor, scale_factor),
            &mut pixmap.as_mut(),
        );

        // Encode as PNG
        pixmap
            .encode_png()
            .map_err(|e| format!("PNG encoding error: {e:?}").into())
    }

    /// Process image data and add to document
    pub fn process_image_data(
        &self,
        docx: Docx,
        data: &[u8],
        alt_text: Option<&str>,
        scale: Option<f32>,
    ) -> Docx {
        // Add image format validation
        match image::guess_format(data) {
            Ok(..) => {
                // Process image data

                // For other formats, try to convert to PNG
                let pic = match image::load_from_memory(data) {
                    Ok(img) => {
                        let (w, h) =
                            Self::image_dim(::image::GenericImageView::dimensions(&img), scale);
                        let mut buffer = Vec::new();
                        if img
                            .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)
                            .is_ok()
                        {
                            Pic::new_with_dimensions(buffer, w, h)
                        } else {
                            // If conversion fails, return original document (without image)
                            let err_para = Paragraph::new().add_run(Run::new().add_text(
                                        "[Image processing error: Unable to convert to supported format]".to_string(),
                                    ));
                            return docx.add_paragraph(err_para);
                        }
                    }
                    Err(_) => {
                        // If unable to load image, return original document (without image)
                        let err_para = Paragraph::new().add_run(Run::new().add_text(
                            "[Image processing error: Unable to load image]".to_string(),
                        ));
                        return docx.add_paragraph(err_para);
                    }
                };

                let img_para = Paragraph::new().add_run(Run::new().add_image(pic));
                let doc_with_img = docx.add_paragraph(img_para);

                if let Some(alt) = alt_text {
                    if !alt.is_empty() {
                        let caption_para = Paragraph::new()
                            .style("Caption")
                            .add_run(Run::new().add_text(alt));
                        doc_with_img.add_paragraph(caption_para)
                    } else {
                        doc_with_img
                    }
                } else {
                    doc_with_img
                }
            }
            Err(_) => {
                // If unable to determine image format, return original document (without image)
                let err_para =
                    Paragraph::new()
                        .add_run(Run::new().add_text(
                            "[Image processing error: Unknown image format]".to_string(),
                        ));
                docx.add_paragraph(err_para)
            }
        }
    }

    /// Process inline image and add to Run
    pub fn process_inline_image(&self, mut run: Run, data: &[u8]) -> Result<Run> {
        match image::guess_format(data) {
            Ok(..) => {
                // Try to convert to PNG
                let pic = match image::load_from_memory(data) {
                    Ok(img) => {
                        let (w, h) = ::image::GenericImageView::dimensions(&img);
                        let mut buffer = Vec::new();
                        if img
                            .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Png)
                            .is_ok()
                        {
                            Pic::new_with_dimensions(buffer, w, h)
                        } else {
                            run = run.add_text("[Image conversion error]");
                            return Ok(run);
                        }
                    }
                    Err(_) => {
                        run = run.add_text("[Image loading error]");
                        return Ok(run);
                    }
                };

                run = run.add_image(pic);
                Ok(run)
            }
            Err(_) => {
                run = run.add_text("[Unknown image format]");
                Ok(run)
            }
        }
    }

    /// Process data URL inline image
    pub fn process_data_url_image(&self, run: Run, src: &str, is_typst_block: bool) -> Result<Run> {
        if let Some(data_start) = src.find("base64,") {
            let base64_data = &src[data_start + 7..];
            if let Ok(img_data) = base64::engine::general_purpose::STANDARD.decode(base64_data) {
                // If it's a typst-block (SVG data), special handling is needed
                if is_typst_block {
                    // Use resvg to convert SVG to PNG
                    if let Ok(png_data) = self.convert_svg_to_png(&img_data) {
                        let mut new_run = run;
                        new_run = self.process_inline_image(new_run, &png_data)?;
                        return Ok(new_run);
                    } else {
                        return Ok(run.add_text("[SVG conversion failed]"));
                    }
                } else {
                    // Normal image processing
                    let mut new_run = run;
                    new_run = self.process_inline_image(new_run, &img_data)?;
                    return Ok(new_run);
                }
            }
        }
        Ok(run.add_text("[Invalid data URL]"))
    }

    /// Calculate image dimensions for DOCX
    pub fn image_dim((w, h): (u32, u32), scale_factor: Option<f32>) -> (u32, u32) {
        let actual_scale = scale_factor.unwrap_or(1.0);

        let max_width = 5486400;
        let scaled_w = (w as f32 * actual_scale) as u32;
        let scaled_h = (h as f32 * actual_scale) as u32;

        if scaled_w > max_width {
            let ratio = scaled_h as f32 / scaled_w as f32;
            let new_width = max_width;
            let new_height = (max_width as f32 * ratio) as u32;
            (new_width, new_height)
        } else {
            (scaled_w, scaled_h)
        }
    }
}
