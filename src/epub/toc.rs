use super::{resolve_root_path, EpubArchive};
use crate::types::*;
use anyhow::Result;
use std::path::Path;

pub fn parse_epub_toc(
    archive: &mut EpubArchive,
    opf_content: &str,
    opf_path: &str,
) -> Result<Vec<EpubChapter>> {
    let doc = roxmltree::Document::parse(opf_content)?;
    let nav_paths = get_nav_files(&doc, opf_path);

    if nav_paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut xhtml_chapters = Vec::new();
    let mut ncx_chapters = Vec::new();

    for nav_path in &nav_paths {
        let content = match archive.read_entry_string(nav_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Fix bare ampersands (Rust regex doesn't support lookahead, so do a simple replace-and-restore)
        let content = content
            .replace("&amp;", "\x00AMP\x00")
            .replace("&lt;", "\x00LT\x00")
            .replace("&gt;", "\x00GT\x00")
            .replace("&quot;", "\x00QUOT\x00")
            .replace("&apos;", "\x00APOS\x00")
            .replace('&', "&amp;")
            .replace("\x00AMP\x00", "&amp;")
            .replace("\x00LT\x00", "&lt;")
            .replace("\x00GT\x00", "&gt;")
            .replace("\x00QUOT\x00", "&quot;")
            .replace("\x00APOS\x00", "&apos;");

        let toc_doc = match roxmltree::Document::parse(&content) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let lower_path = nav_path.to_lowercase();
        if lower_path.ends_with(".xhtml") {
            parse_xhtml_toc(&toc_doc, nav_path, &mut xhtml_chapters);
        } else if lower_path.ends_with(".ncx") {
            parse_ncx_toc(&toc_doc, nav_path, &mut ncx_chapters);
        }
    }

    let mut chapters = if xhtml_chapters.len() >= ncx_chapters.len() {
        xhtml_chapters
    } else {
        ncx_chapters
    };

    // Merge chapters on the same page
    let mut i = chapters.len().wrapping_sub(2);
    while i < chapters.len() {
        if chapters[i].page == chapters[i + 1].page {
            let merged_title =
                format!("{} - {}", chapters[i].title, chapters[i + 1].title);
            chapters[i].title = merged_title;
            chapters.remove(i + 1);
        }
        i = i.wrapping_sub(1);
    }

    Ok(chapters)
}

fn get_nav_files(doc: &roxmltree::Document, opf_path: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for node in doc.descendants() {
        if node.tag_name().name() == "item" {
            let media_type = node.attribute("media-type").unwrap_or("");
            let properties = node.attribute("properties").unwrap_or("");

            if media_type == "application/x-dtbncx+xml" || properties == "nav" {
                if let Some(href) = node.attribute("href") {
                    let resolved = resolve_root_path(opf_path, href);
                    if seen.insert(resolved.clone()) {
                        paths.push(resolved);
                    }
                }
            }
        }
    }
    paths
}

fn parse_xhtml_toc(doc: &roxmltree::Document, nav_path: &str, chapters: &mut Vec<EpubChapter>) {
    let ops_ns = "http://www.idpf.org/2007/ops";

    // Find <nav epub:type="toc">
    let nav = doc.descendants().find(|n| {
        n.tag_name().name() == "nav" && n.attribute((ops_ns, "type")) == Some("toc")
    });

    let nav = match nav {
        Some(n) => n,
        None => return,
    };

    // Collect all <a> elements with href and text
    let mut href_titles: Vec<(String, Vec<String>)> = Vec::new();

    for a_node in nav.descendants().filter(|n| n.tag_name().name() == "a") {
        let href = match a_node.attribute("href") {
            Some(h) => h.to_string(),
            None => continue,
        };
        let text = collect_text(&a_node);
        if text.is_empty() {
            continue;
        }

        // Group by href (same page can appear multiple times)
        if let Some(existing) = href_titles.iter_mut().find(|(h, _)| *h == href) {
            existing.1.push(text);
        } else {
            href_titles.push((href, vec![text]));
        }
    }

    for (href, titles) in href_titles {
        let title = titles.join(" - ");
        let page_part = href.split('#').next().unwrap_or("");
        let resolved = resolve_root_path(nav_path, page_part);
        let page_filename = Path::new(&resolved)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        chapters.push(EpubChapter {
            title,
            page: page_filename,
        });
    }
}

fn parse_ncx_toc(doc: &roxmltree::Document, nav_path: &str, chapters: &mut Vec<EpubChapter>) {
    let ncx_ns = "http://www.daisy.org/z3986/2005/ncx/";

    for nav_point in doc.descendants().filter(|n| {
        n.tag_name().name() == "navPoint"
            && n.tag_name()
                .namespace()
                .map_or(true, |ns| ns == ncx_ns)
    }) {
        let title = nav_point
            .descendants()
            .find(|n| n.tag_name().name() == "text")
            .and_then(|n| n.text())
            .unwrap_or("")
            .trim()
            .to_string();

        let src = nav_point
            .descendants()
            .find(|n| n.tag_name().name() == "content")
            .and_then(|n| n.attribute("src"))
            .unwrap_or("");

        let page_part = src.split('#').next().unwrap_or("");
        let resolved = resolve_root_path(nav_path, page_part);
        let page_filename = Path::new(&resolved)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        chapters.push(EpubChapter {
            title,
            page: page_filename,
        });
    }
}

pub fn parse_alternative_toc(
    archive: &mut EpubArchive,
    opf_content: &str,
    chapters: Vec<EpubChapter>,
    book_full: &[EpubPage],
    opf_path: &str,
) -> Vec<EpubChapter> {
    let doc = match roxmltree::Document::parse(opf_content) {
        Ok(d) => d,
        Err(_) => return chapters,
    };

    let mut alt_toc_file = String::new();

    for node in doc.descendants() {
        if node.tag_name().name() == "reference"
            && node.attribute("type") == Some("toc")
        {
            if let Some(href) = node.attribute("href") {
                alt_toc_file = href.to_string();
                break;
            }
        }
    }

    if alt_toc_file.is_empty() {
        return chapters;
    }

    let alt_toc_file = resolve_root_path(opf_path, alt_toc_file.split('#').next().unwrap_or(""));

    for (index, page) in book_full.iter().enumerate() {
        if page.page == alt_toc_file {
            return get_toc_file(archive, &chapters, book_full, index);
        }
    }

    chapters
}

fn get_toc_file(
    archive: &mut EpubArchive,
    existing_chapters: &[EpubChapter],
    book_full: &[EpubPage],
    start_index: usize,
) -> Vec<EpubChapter> {
    let mut new_chapters: Vec<EpubChapter> = existing_chapters.to_vec();

    // Check up to 6 pages starting from the TOC page
    for i in 0..6 {
        let idx = start_index + i;
        if idx >= book_full.len() {
            break;
        }

        let alt_toc_path = &book_full[idx].page;
        let content = match archive.read_entry_string(alt_toc_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let doc = match roxmltree::Document::parse(&content) {
            Ok(d) => d,
            Err(_) => break,
        };

        let body = match doc.descendants().find(|n| n.tag_name().name() == "body") {
            Some(b) => b,
            None => break,
        };

        let hrefs: Vec<String> = body
            .descendants()
            .filter(|n| n.tag_name().name() == "a")
            .filter_map(|n| n.attribute("href").map(|h| h.to_string()))
            .collect();

        for href in &hrefs {
            let href_filename = Path::new(href.split('#').next().unwrap_or(""))
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();

            for (book_idx, book_page) in book_full.iter().enumerate() {
                let page_filename = Path::new(&book_page.page)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();

                if page_filename == href_filename {
                    new_chapters.push(EpubChapter {
                        title: format!("Page {}", book_idx + 1),
                        page: href_filename.clone(),
                    });
                    break;
                }
            }
        }
    }

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    new_chapters.retain(|ch| seen.insert(ch.page.clone()));
    new_chapters
}

fn collect_text(node: &roxmltree::Node) -> String {
    let mut text = String::new();
    for child in node.children() {
        if child.is_text() {
            text.push_str(child.text().unwrap_or(""));
        } else {
            text.push_str(&collect_text(&child));
        }
    }
    text.trim().to_string()
}
