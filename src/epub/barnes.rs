use super::{resolve_root_path, EpubArchive};
use crate::processing::image as imgproc;
use crate::types::*;
use anyhow::Result;

pub fn is_barnes_and_noble_book(opf_content: &str) -> bool {
    // Check for BNContentKind meta tag (without namespace prefix)
    opf_content.contains("BNContentKind")
}

pub fn get_replica_map(
    archive: &mut EpubArchive,
    opf_content: &str,
    opf_path: &str,
) -> Result<(String, String)> {
    let doc = roxmltree::Document::parse(opf_content)?;

    let mut replica_href = String::new();
    for node in doc.descendants() {
        if node.tag_name().name() == "item" && node.attribute("id") == Some("replicaMap") {
            if let Some(href) = node.attribute("href") {
                replica_href = href.to_string();
                break;
            }
        }
    }

    anyhow::ensure!(!replica_href.is_empty(), "replicaMap not found in manifest");

    let replica_path = resolve_root_path(opf_path, &replica_href);
    let content = archive.read_entry_string(&replica_path)?;

    Ok((content, replica_path))
}

pub fn parse_replica_map_pages(
    replica_content: &str,
    replica_path: &str,
) -> Result<Vec<EpubSpineEntry>> {
    let doc = roxmltree::Document::parse(replica_content)?;
    let mut entries = Vec::new();

    for node in doc.descendants() {
        if node.tag_name().name() == "Page" {
            let page_num = node.attribute("pageNum").unwrap_or("").to_string();
            let file_attr = node.attribute("file").unwrap_or("");

            let page_file = if !file_attr.is_empty() {
                resolve_root_path(replica_path, file_attr)
            } else {
                String::new()
            };

            entries.push(EpubSpineEntry {
                pages: page_file,
                ids: page_num,
                spread: String::new(),
            });
        }
    }

    Ok(entries)
}

pub fn parse_replica_map_pages_xml(
    archive: &mut EpubArchive,
    spine: &[EpubSpineEntry],
) -> Result<Vec<EpubPage>> {
    let mut book_full = Vec::new();
    const WIDE_IMAGE_RATIO: f64 = 1.125;

    for entry in spine {
        let image_path = &entry.pages;
        if !image_path.is_empty() && archive.entry_exists(image_path) {
            let data = archive.read_entry(image_path)?;
            let (width, height) = imgproc::get_image_dimensions(&data);
            let size = data.len() as u64;
            let is_double = width as f64 >= (height as f64 * WIDE_IMAGE_RATIO);

            book_full.push(EpubPage {
                page: entry.ids.clone(),
                image: image_path.clone(),
                doublepage: is_double,
                height,
                width,
                size,
                ..Default::default()
            });
        }
    }

    Ok(book_full)
}

pub fn parse_cover(
    archive: &mut EpubArchive,
    opf_content: &str,
    book_full: &mut Vec<EpubPage>,
    opf_path: &str,
) -> Result<()> {
    let doc = roxmltree::Document::parse(opf_content)?;

    let mut cover_href = String::new();
    for node in doc.descendants() {
        if node.tag_name().name() == "item" && node.attribute("id") == Some("cover") {
            if let Some(href) = node.attribute("href") {
                cover_href = href.to_string();
                break;
            }
        }
    }

    if cover_href.is_empty() {
        return Ok(());
    }

    let filename = resolve_root_path(opf_path, &cover_href);
    if !archive.entry_exists(&filename) {
        return Ok(());
    }

    let data = archive.read_entry(&filename)?;
    let (width, height) = imgproc::get_image_dimensions(&data);
    let size = data.len() as u64;

    book_full.insert(
        0,
        EpubPage {
            page: "Cover".to_string(),
            image: filename,
            height,
            width,
            size,
            ..Default::default()
        },
    );

    Ok(())
}

pub fn parse_toc(replica_content: &str) -> Result<Vec<EpubChapter>> {
    let doc = roxmltree::Document::parse(replica_content)?;
    let mut chapters = Vec::new();

    for node in doc.descendants() {
        if node.tag_name().name() == "TocEntry" {
            let title = node.attribute("title").unwrap_or("").to_string();
            let page_num = node
                .children()
                .find(|n| n.tag_name().name() == "Page")
                .and_then(|n| n.attribute("pagenum"))
                .unwrap_or("")
                .to_string();

            chapters.push(EpubChapter {
                title,
                page: page_num,
            });
        }
    }

    Ok(chapters)
}
