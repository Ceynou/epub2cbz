use crate::epub::EpubArchive;
use crate::processing::image as imgproc;
use crate::settings::Settings;
use crate::types::*;
use std::path::Path;

pub fn integrate_chapters(book_full: &mut [EpubPage], chapters: &[EpubChapter]) {
    let chapter_map: std::collections::HashMap<&str, &str> = chapters
        .iter()
        .map(|c| (c.page.as_str(), c.title.as_str()))
        .collect();

    for (i, page) in book_full.iter_mut().enumerate() {
        let page_filename = Path::new(&page.page)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Some(&title) = chapter_map.get(page_filename.as_str()) {
            page.bookmark = html_decode(title);
        } else if i == 0 {
            page.bookmark = "Cover".to_string();
        }
    }
}

/// Returns true if a duplicate cover was removed.
/// BUG FIX: The original C# version crashed with IndexOutOfRange when
/// book_full had fewer than 2 elements. This version checks bounds first.
pub fn check_duplicate_cover(
    chapters: &[EpubChapter],
    book_full: &mut Vec<EpubPage>,
    archive: &mut EpubArchive,
    opf_content: &str,
    settings: &Settings,
) -> bool {
    // Guard: need at least 2 pages
    if book_full.len() < 2 {
        return false;
    }

    let page1_filename = Path::new(&book_full[1].page)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    // Check if first chapter points to second page and is a cover
    if !chapters.is_empty() && chapters[0].page == page1_filename {
        let title = &chapters[0].title;
        if title.contains("Cover") || title == "カバー" || title == "表紙" {
            remove_duplicate_cover(book_full, settings);
            return true;
        }
    }

    // Check if cover image and second page image are identical paths
    let img0_filename = Path::new(&book_full[0].image)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();
    let img1_filename = Path::new(&book_full[1].image)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_default();

    if !img0_filename.is_empty() && img0_filename == img1_filename {
        remove_duplicate_cover(book_full, settings);
        return true;
    }

    // Compare images by perceptual hash
    if !book_full[0].image.is_empty() && !book_full[1].image.is_empty() {
        let data0 = archive.read_entry(&book_full[0].image).ok();
        let data1 = archive.read_entry(&book_full[1].image).ok();

        if let (Some(d0), Some(d1)) = (data0, data1) {
            if imgproc::compare_images(&d0, &d1) {
                remove_duplicate_cover(book_full, settings);
                return true;
            }
        }
    }

    // Check OPF guide for cover reference
    if let Ok(doc) = roxmltree::Document::parse(opf_content) {
        for node in doc.descendants() {
            if node.tag_name().name() == "reference"
                && node.attribute("type") == Some("cover")
            {
                if let Some(href) = node.attribute("href") {
                    let cover_filename = Path::new(href.split('#').next().unwrap_or(""))
                        .file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default();

                    if !cover_filename.is_empty() && cover_filename == page1_filename {
                        remove_duplicate_cover(book_full, settings);
                        return true;
                    }
                }
            }
        }
    }

    false
}

fn remove_duplicate_cover(book_full: &mut Vec<EpubPage>, settings: &Settings) {
    if book_full.len() < 2 {
        return;
    }

    if settings.higher_resolution_cover && book_full[0].height > 0 && book_full[1].height > 0 {
        if book_full[0].height >= book_full[1].height {
            if settings.page_spread {
                book_full[1].blank = true;
            } else {
                book_full.remove(1);
            }
        } else {
            book_full.remove(0);
            if settings.page_spread && book_full.len() >= 2 {
                book_full.insert(
                    1,
                    EpubPage {
                        page: "blank".to_string(),
                        blank: true,
                        ..Default::default()
                    },
                );
            }
        }
    } else if settings.page_spread {
        book_full[1].blank = true;
    } else {
        book_full.remove(1);
    }
}

pub fn fix_page_alignment_post(book_full: &mut Vec<EpubPage>, direction: &ReadingDirection) {
    if book_full.is_empty() {
        return;
    }

    let is_rtl = direction.is_rtl();
    let mut expected_left = !is_rtl;

    let mut aligned: Vec<EpubPage> = Vec::with_capacity(book_full.len());
    aligned.push(book_full[0].clone());

    for i in 1..book_full.len() {
        let current = &book_full[i];
        let spread_lower = current.spread.to_lowercase();

        let req_left =
            spread_lower.contains("left") || (current.doublepage && !is_rtl);
        let req_right =
            spread_lower.contains("right") || (current.doublepage && is_rtl);

        let need_blank =
            (req_left && !expected_left) || (req_right && expected_left);

        if need_blank {
            aligned.push(EpubPage {
                page: "blank".to_string(),
                spread: if expected_left {
                    "page-spread-left".to_string()
                } else {
                    "page-spread-right".to_string()
                },
                ..Default::default()
            });
            expected_left = !expected_left;
        }

        aligned.push(current.clone());

        if current.doublepage {
            expected_left = !is_rtl;
        } else {
            expected_left = !expected_left;
        }
    }

    if aligned.len() > book_full.len() {
        *book_full = aligned;
    }
}

pub fn blank_page_behavior(
    book_full: &mut Vec<EpubPage>,
    archive: &mut EpubArchive,
) {
    if book_full.len() < 3 {
        return;
    }

    let is_blank = if !book_full[2].image.is_empty() && !book_full[2].blank {
        archive
            .read_entry(&book_full[2].image)
            .ok()
            .map(|data| imgproc::is_image_blank_white(&data))
            .unwrap_or(false)
    } else {
        book_full[2].image.is_empty() || book_full[2].blank
    };

    if is_blank {
        book_full.remove(2);
        book_full.remove(1);
    }
}
