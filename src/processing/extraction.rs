use crate::epub::EpubArchive;
use crate::processing::image as imgproc;
use crate::settings::Settings;
use crate::types::*;
use anyhow::Result;
use image::GenericImageView;
use rayon::prelude::*;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const WIDE_IMAGE_RATIO: f64 = 1.125;

struct PageTask {
    name: String,
    data: PageData,
}

enum PageData {
    Passthrough(Vec<u8>),
    Blank(u32, u32),
    Process(Vec<u8>, bool, bool),
    Split(Vec<u8>, bool),
}

struct PageResult {
    chunks: Vec<(String, Vec<u8>)>,
}

pub fn extract_to_cbz(
    archive: &mut EpubArchive,
    book_full: &[EpubPage],
    _chapters: &[EpubChapter],
    metadata: &EpubMetadata,
    comic_info_xml: &str,
    settings: &Settings,
    output_path: &std::path::Path,
    log: &dyn Fn(&str),
) -> Result<()> {
    let extension = settings.image_extension();
    let needs_processing = settings.crop_images || settings.resize_images;
    let (single_w, single_h) = if settings.resize_images {
        imgproc::get_single_page_resolution(book_full, settings)
    } else {
        (0, 0)
    };

    // Phase 1: read all page data from ZIP and build task list
    let mut tasks: Vec<PageTask> = Vec::with_capacity(book_full.len());
    let mut page_number = 0u32;
    let mut current_chapter = String::new();

    for (i, page) in book_full.iter().enumerate() {
        if page.blank && page.image.is_empty() {
            if settings.page_spread {
                page_number += 1;
                let name = format_page_name(page_number, &current_chapter, ".png", settings);
                let (bw, bh) = if single_w > 0 && single_h > 0 {
                    (single_w, single_h)
                } else {
                    get_neighbor_dimensions(book_full, i)
                };
                tasks.push(PageTask {
                    name,
                    data: PageData::Blank(bw.max(1), bh.max(1)),
                });
            }
            continue;
        }

        if page.image.is_empty() {
            continue;
        }

        if !page.bookmark.is_empty() {
            current_chapter = sanitize_filename(&page.bookmark);
        }

        let data = match archive.read_entry(&page.image) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let is_double = page.doublepage
            || (page.width as f64 >= page.height as f64 * WIDE_IMAGE_RATIO
                && page.width > 0
                && page.height > 0);

        if is_double && settings.split_page_spread && i > 0 {
            page_number += 1;
            let name1 = format_page_name(page_number, &current_chapter, &extension, settings);
            page_number += 1;
            let name2 = format_page_name(page_number, &current_chapter, &extension, settings);
            tasks.push(PageTask {
                name: format!("{}\0{}", name1, name2),
                data: PageData::Split(data, metadata.reading_direction.is_rtl()),
            });
        } else if needs_processing || !format_matches(&page.image, &extension) {
            let is_double_resize = is_double && single_w > 0;
            page_number += 1;
            let name = format_page_name(page_number, &current_chapter, &extension, settings);
            tasks.push(PageTask {
                name,
                data: PageData::Process(data, is_double_resize, needs_processing),
            });
        } else {
            // Fast path: no processing needed, format matches → passthrough
            page_number += 1;
            let src_ext = std::path::Path::new(&page.image)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_else(|| extension.clone());
            let name = format_page_name(page_number, &current_chapter, &src_ext, settings);
            tasks.push(PageTask {
                name,
                data: PageData::Passthrough(data),
            });
        }
    }

    let total = tasks.len();
    log(&format!("  Extracting {total} pages..."));

    // Phase 2: process images in parallel
    let ext = extension.clone();
    let results: Vec<PageResult> = tasks
        .into_par_iter()
        .map(|task| {
            let chunks = match task.data {
                PageData::Passthrough(data) => {
                    vec![(task.name.clone(), data)]
                }
                PageData::Blank(w, h) => {
                    vec![(task.name.clone(), imgproc::create_blank_image_png(w, h))]
                }
                PageData::Process(data, is_double_resize, do_process) => {
                    match image::load_from_memory(&data) {
                        Ok(mut img) => {
                            if do_process {
                                let (max_w, max_h) = if is_double_resize {
                                    (single_w * 2, single_h)
                                } else {
                                    (single_w, single_h)
                                };
                                if settings.crop_images {
                                    imgproc::apply_cropping(&mut img, settings);
                                }
                                if settings.resize_images && max_w > 0 && max_h > 0 {
                                    imgproc::apply_resizing(&mut img, max_w, max_h);
                                }
                            }
                            vec![(task.name.clone(), imgproc::encode_image(&img, &ext))]
                        }
                        Err(_) => vec![(task.name.clone(), data)],
                    }
                }
                PageData::Split(data, is_rtl) => {
                    let names: Vec<&str> = task.name.split('\0').collect();
                    match image::load_from_memory(&data) {
                        Ok(img) => {
                            let (w, h) = img.dimensions();
                            let half = w / 2;
                            let (first_x, second_x) = if is_rtl {
                                (half, 0)
                            } else {
                                (0, half)
                            };
                            let mut left = img.crop_imm(first_x, 0, half, h);
                            let mut right = img.crop_imm(second_x, 0, w - half, h);
                            if settings.crop_images {
                                imgproc::apply_cropping(&mut left, settings);
                                imgproc::apply_cropping(&mut right, settings);
                            }
                            if settings.resize_images && single_w > 0 && single_h > 0 {
                                imgproc::apply_resizing(&mut left, single_w, single_h);
                                imgproc::apply_resizing(&mut right, single_w, single_h);
                            }
                            vec![
                                (names[0].to_string(), imgproc::encode_image(&left, &ext)),
                                (names[1].to_string(), imgproc::encode_image(&right, &ext)),
                            ]
                        }
                        Err(_) => vec![(names[0].to_string(), data)],
                    }
                }
            };
            PageResult { chunks }
        })
        .collect();

    // Phase 3: write to ZIP sequentially
    let file = std::fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(settings.zip_compression());

    for result in &results {
        for (name, data) in &result.chunks {
            zip.start_file(name, options)?;
            zip.write_all(data)?;
        }
    }

    if settings.comic_info && !comic_info_xml.is_empty() {
        zip.start_file("ComicInfo.xml", options)?;
        zip.write_all(comic_info_xml.as_bytes())?;
    }

    zip.finish()?;
    log(&format!("  Done: {} pages written", results.len()));
    Ok(())
}

fn format_matches(image_path: &str, target_ext: &str) -> bool {
    let src = image_path.to_lowercase();
    let tgt = target_ext.to_lowercase();
    match tgt.as_str() {
        ".jpg" | ".jpeg" => src.ends_with(".jpg") || src.ends_with(".jpeg"),
        ".png" => src.ends_with(".png"),
        ".webp" => src.ends_with(".webp"),
        _ => false,
    }
}

fn format_page_name(
    page_number: u32,
    chapter: &str,
    extension: &str,
    settings: &Settings,
) -> String {
    let num = format!("{:04}", page_number);
    if settings.chapter_folders && !chapter.is_empty() {
        format!("{chapter}/{num}{extension}")
    } else {
        format!("{num}{extension}")
    }
}

fn get_neighbor_dimensions(book_full: &[EpubPage], index: usize) -> (u32, u32) {
    for i in (index + 1)..book_full.len() {
        if book_full[i].width > 0 && book_full[i].height > 0 && !book_full[i].doublepage {
            return (book_full[i].width, book_full[i].height);
        }
    }
    for i in (0..index).rev() {
        if book_full[i].width > 0 && book_full[i].height > 0 && !book_full[i].doublepage {
            return (book_full[i].width, book_full[i].height);
        }
    }
    (800, 1200)
}
