use crate::epub::EpubArchive;
use crate::settings::Settings;
use crate::types::*;
use anyhow::Result;
use std::io::Write;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn simple_extract_to_cbz(
    archive: &mut EpubArchive,
    book_full: &[EpubPage],
    comic_info_xml: &str,
    settings: &Settings,
    output_path: &std::path::Path,
    log: &dyn Fn(&str),
) -> Result<()> {
    let file = std::fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(settings.zip_compression());

    let extension = settings.image_extension();
    let mut page_number = 0u32;

    for page in book_full {
        if page.image.is_empty() || page.blank {
            continue;
        }

        let data = match archive.read_entry(&page.image) {
            Ok(d) => d,
            Err(e) => {
                log(&format!("  Warning: Could not read {}: {e}", page.image));
                continue;
            }
        };

        page_number += 1;
        let img_ext = std::path::Path::new(&page.image)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_else(|| extension.clone());
        let name = format!("{:04}{img_ext}", page_number);

        zip.start_file(&name, options)?;
        zip.write_all(&data)?;
        log(&format!("  {name}"));
    }

    if settings.comic_info && !comic_info_xml.is_empty() {
        zip.start_file("ComicInfo.xml", options)?;
        zip.write_all(comic_info_xml.as_bytes())?;
    }

    zip.finish()?;
    Ok(())
}
