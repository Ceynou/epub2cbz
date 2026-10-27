pub mod css;
pub mod epub;
pub mod processing;
pub mod settings;
pub mod types;

use crate::epub::barnes;
use crate::epub::detection;
use crate::epub::metadata;
use crate::epub::parsing;
use crate::epub::toc;
use crate::epub::EpubArchive;
use crate::processing::alignment;
use crate::processing::comic_info;
use crate::processing::extraction;
use crate::processing::image as imgproc;
use crate::processing::simple;
use crate::settings::Settings;
use crate::types::*;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const WIDE_IMAGE_RATIO: f64 = 1.125;

pub fn process_epub(
    epub_path: &Path,
    output_dir: &Path,
    settings: &Settings,
    log: &dyn Fn(&str),
) -> Result<ProcessingResult> {
    let filename = epub_path
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = epub_path
        .file_stem()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    log(&format!("Processing: {filename}"));

    if !detection::check_epub(epub_path) {
        anyhow::bail!("Not a valid EPUB file (missing PK header)");
    }

    let mut archive = EpubArchive::open(epub_path)
        .with_context(|| format!("Failed to open: {filename}"))?;

    if settings.drm_protection {
        if let Err(e) = detection::check_drm_protection(&mut archive) {
            return Ok(ProcessingResult {
                filename: filename.clone(),
                success: false,
                message: format!("DRM protected: {e}"),
            });
        }
    }

    let opf_path = parsing::get_opf_file(&mut archive)?;
    let opf_content = parsing::parse_opf_document(&mut archive, &opf_path)?;

    let epub_metadata = metadata::parse_metadata_xml(&opf_content, settings)?;
    let direction = epub_metadata.reading_direction;

    let is_bn = barnes::is_barnes_and_noble_book(&opf_content);
    let is_omf = parsing::is_open_manga_format(&opf_content);

    let (mut book_full, chapters) = if is_omf {
        log("  OpenMangaFormat detected");
        let bf = parsing::parse_omf_content_js(&mut archive, &opf_path)?;
        let ch = toc::parse_epub_toc(&mut archive, &opf_content, &opf_path)?;
        (bf, ch)
    } else if is_bn {
        log("  Barnes & Noble format detected");
        let (replica_content, replica_path) =
            barnes::get_replica_map(&mut archive, &opf_content, &opf_path)?;
        let spine = barnes::parse_replica_map_pages(&replica_content, &replica_path)?;
        let mut bf = barnes::parse_replica_map_pages_xml(&mut archive, &spine)?;
        barnes::parse_cover(&mut archive, &opf_content, &mut bf, &opf_path)?;
        let ch = barnes::parse_toc(&replica_content)?;
        (bf, ch)
    } else {
        let spine = parsing::parse_spine_xml(&opf_content, &opf_path)?;
        let mut bf =
            parsing::parse_opf_pages_xml(&mut archive, &spine, &opf_path, &opf_content)?;

        // Fill in dimensions for pages that don't have them yet
        for page in bf.iter_mut() {
            if !page.image.is_empty() && page.width == 0 {
                if let Ok(data) = archive.read_entry(&page.image) {
                    let (w, h) = imgproc::get_image_dimensions(&data);
                    page.width = w;
                    page.height = h;
                    page.size = data.len() as u64;
                    page.doublepage =
                        w as f64 >= h as f64 * WIDE_IMAGE_RATIO && w > 0 && h > 0;
                }
            }
        }

        // Alternative cover
        if settings.add_alternative_cover {
            let _ = parsing::parse_alternative_cover(
                &mut archive,
                &opf_content,
                &mut bf,
                &opf_path,
            );
        }

        let ch = toc::parse_epub_toc(&mut archive, &opf_content, &opf_path)?;
        (bf, ch)
    };

    if book_full.is_empty() {
        return Ok(ProcessingResult {
            filename,
            success: false,
            message: "No pages found in EPUB".to_string(),
        });
    }

    let is_fxl = detection::is_fixed_layout_epub(&opf_content, &mut archive);
    log(&format!(
        "  Pages: {}, FXL: {is_fxl}, Direction: {:?}",
        book_full.len(),
        direction
    ));

    // Integrate chapter bookmarks
    alignment::integrate_chapters(&mut book_full, &chapters);

    // Duplicate cover detection
    if settings.duplicate_cover {
        if alignment::check_duplicate_cover(
            &chapters,
            &mut book_full,
            &mut archive,
            &opf_content,
            settings,
        ) {
            log("  Duplicate cover removed");
        }
    }

    // Blank page behavior
    if settings.blank_image {
        alignment::blank_page_behavior(&mut book_full, &mut archive);
    }

    // Page spread alignment
    if settings.page_spread {
        alignment::fix_page_alignment_post(&mut book_full, &direction);
    }

    // Generate ComicInfo.xml
    let comic_info_xml = if settings.comic_info {
        comic_info::generate_comic_info(&book_full, &epub_metadata, &filename, settings)
    } else {
        String::new()
    };

    // Build output path
    let ext = settings.archive_extension();
    let output_filename = format!("{stem}{ext}");
    let output_path = output_dir.join(&output_filename);

    // Extract
    if settings.simple_extraction {
        simple::simple_extract_to_cbz(
            &mut archive,
            &book_full,
            &comic_info_xml,
            settings,
            &output_path,
            log,
        )?;
    } else {
        extraction::extract_to_cbz(
            &mut archive,
            &book_full,
            &chapters,
            &epub_metadata,
            &comic_info_xml,
            settings,
            &output_path,
            log,
        )?;
    }

    log(&format!("  Output: {}", output_path.display()));

    Ok(ProcessingResult {
        filename,
        success: true,
        message: format!("Converted to {output_filename}"),
    })
}

pub fn find_epub_files(path: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if path.is_file() {
        if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("epub"))
            .unwrap_or(false)
        {
            files.push(path.to_path_buf());
        }
        return files;
    }

    if path.is_dir() {
        if recursive {
            for entry in walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file()
                    && entry
                        .path()
                        .extension()
                        .map(|e| e.eq_ignore_ascii_case("epub"))
                        .unwrap_or(false)
                {
                    files.push(entry.into_path());
                }
            }
        } else if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let p = entry.path();
                if p.is_file()
                    && p.extension()
                        .map(|e| e.eq_ignore_ascii_case("epub"))
                        .unwrap_or(false)
                {
                    files.push(p);
                }
            }
        }
    }

    files.sort();
    files
}
