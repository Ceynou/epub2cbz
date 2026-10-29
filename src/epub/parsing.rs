use super::{resolve_root_path, EpubArchive};
use crate::css::SimpleStylesheet;
use crate::processing::image as imgproc;
use crate::types::*;
use anyhow::{Context, Result};


const OPF_NS: &str = "http://www.idpf.org/2007/opf";

pub fn get_opf_file(archive: &mut EpubArchive) -> Result<String> {
    let content = archive
        .read_entry_string("META-INF/container.xml")
        .context("container.xml not found")?;
    let doc = crate::epub::parse_xml(&content)?;

    for node in doc.descendants() {
        if node.tag_name().name() == "rootfile" {
            if let Some(path) = node.attribute("full-path") {
                return Ok(path.to_string());
            }
        }
    }
    anyhow::bail!("OPF file not found in container.xml")
}

pub fn parse_opf_document(archive: &mut EpubArchive, opf_path: &str) -> Result<String> {
    let mut content = archive.read_entry_string(opf_path)?;
    // Sanitize null characters
    content = content.replace('\0', "").replace('\x01', "");
    // Fix bare ampersands
    content = content.replace("&amp;", "&").replace('&', "&amp;");
    Ok(content)
}

pub fn parse_spine_xml(
    opf_content: &str,
    opf_path: &str,
) -> Result<Vec<EpubSpineEntry>> {
    let doc = crate::epub::parse_xml(opf_content)?;
    let mut id_props: Vec<(String, String)> = Vec::new();

    for node in doc.descendants() {
        if node.tag_name().name() == "itemref"
            && node
                .tag_name()
                .namespace()
                .map_or(true, |ns| ns == OPF_NS)
        {
            let idref = node.attribute("idref").unwrap_or("").to_string();
            let props = node.attribute("properties").unwrap_or("").to_string();
            id_props.push((idref, props));
        }
    }

    let mut entries = Vec::new();
    for (idref, props) in &id_props {
        for node in doc.descendants() {
            if node.tag_name().name() == "item"
                && node
                    .tag_name()
                    .namespace()
                    .map_or(true, |ns| ns == OPF_NS)
                && node.attribute("id") == Some(idref)
            {
                if let Some(href) = node.attribute("href") {
                    let resolved = resolve_root_path(opf_path, href);
                    entries.push(EpubSpineEntry {
                        pages: resolved,
                        ids: idref.clone(),
                        spread: props.clone(),
                    });
                }
                break;
            }
        }
    }
    Ok(entries)
}

pub fn parse_opf_pages_xml(
    archive: &mut EpubArchive,
    spine: &[EpubSpineEntry],
    opf_path: &str,
    opf_content: &str,
) -> Result<Vec<EpubPage>> {
    let doc = crate::epub::parse_xml(opf_content)?;
    let (css_path, stylesheet) = get_css_file(opf_path, &doc, archive);
    const WIDE_IMAGE_RATIO: f64 = 1.125;

    let mut book_full = Vec::new();

    for entry in spine {
        let page_path = entry.pages.split('#').next().unwrap_or("");
        if let Some(image_path) = find_image_path_in_file(archive, page_path) {
            if !image_path.is_empty() && archive.entry_exists(&image_path) {
                let data = archive.read_entry(&image_path)?;
                let (width, height) = imgproc::get_image_dimensions(&data);
                let size = data.len() as u64;
                let is_double = width as f64 >= (height as f64 * WIDE_IMAGE_RATIO);

                book_full.push(EpubPage {
                    page: page_path.to_string(),
                    image: image_path,
                    spread: entry.spread.clone(),
                    doublepage: is_double,
                    height,
                    width,
                    size,
                    ..Default::default()
                });
                continue;
            }
        }

        // Try CSS path
        if !css_path.is_empty() {
            if let Some(ref ss) = stylesheet {
                let css_image = find_image_path_in_css(archive, ss, page_path);
                if !css_image.is_empty() {
                    let resolved = resolve_root_path(&css_path, &css_image);
                    if archive.entry_exists(&resolved) {
                        let data = archive.read_entry(&resolved)?;
                        let (width, height) = imgproc::get_image_dimensions(&data);
                        let size = data.len() as u64;
                        let is_double =
                            width as f64 >= (height as f64 * WIDE_IMAGE_RATIO);

                        book_full.push(EpubPage {
                            page: page_path.to_string(),
                            image: resolved,
                            spread: entry.spread.clone(),
                            doublepage: is_double,
                            height,
                            width,
                            size,
                            ..Default::default()
                        });
                        continue;
                    }
                }
            }
        }

        // Blank page if no image found
        if !book_full.iter().any(|b| b.page == entry.pages) {
            book_full.push(EpubPage {
                page: page_path.to_string(),
                spread: entry.spread.clone(),
                blank: true,
                ..Default::default()
            });
        }
    }

    Ok(book_full)
}

fn get_css_file(
    opf_path: &str,
    doc: &roxmltree::Document,
    archive: &mut EpubArchive,
) -> (String, Option<SimpleStylesheet>) {
    for node in doc.descendants() {
        if node.tag_name().name() == "item"
            && node.attribute("media-type") == Some("text/css")
        {
            if let Some(href) = node.attribute("href") {
                let css_path = resolve_root_path(opf_path, href);
                if let Ok(content) = archive.read_entry_string(&css_path) {
                    return (css_path, Some(SimpleStylesheet::parse(&content)));
                }
            }
        }
    }
    (String::new(), None)
}

fn find_image_path_in_css(
    archive: &mut EpubArchive,
    stylesheet: &SimpleStylesheet,
    xhtml_page: &str,
) -> String {
    if xhtml_page.is_empty() {
        return String::new();
    }
    let lower = xhtml_page.to_lowercase();
    if !lower.ends_with(".xhtml") && !lower.ends_with(".html") && !lower.ends_with(".xml") {
        return String::new();
    }
    if !archive.entry_exists(xhtml_page) {
        return String::new();
    }

    let content = match archive.read_entry_string(xhtml_page) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let doc = match crate::epub::parse_xml(&content) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };

    let body = doc.descendants().find(|n| n.tag_name().name() == "body");
    let div = body.and_then(|b| b.children().find(|n| n.tag_name().name() == "div"));

    if let Some(div) = div {
        if let Some(id) = div.attribute("id") {
            if !id.is_empty() {
                let selector = format!("#{id}");
                if let Some(url) = stylesheet.get_background_image(&selector) {
                    return url;
                }
            }
        }

        if let Some(class) = div.attribute("class") {
            for class_name in class.split_whitespace() {
                let selector = format!("div.{class_name}");
                if let Some(url) = stylesheet.get_background_image(&selector) {
                    return url;
                }
                let selector = format!(".{class_name}");
                if let Some(url) = stylesheet.get_background_image(&selector) {
                    return url;
                }
            }
        }
    }

    String::new()
}

pub fn find_image_path_in_file(
    archive: &mut EpubArchive,
    filename: &str,
) -> Option<String> {
    if filename.is_empty() {
        return Some(String::new());
    }

    let lower = filename.to_lowercase();
    if lower.ends_with(".xhtml") || lower.ends_with(".html") || lower.ends_with(".xml") {
        if !archive.entry_exists(filename) {
            return None;
        }

        let content = archive.read_entry_string(filename).ok()?;

        if content.starts_with("<HTML>") {
            return None;
        }

        let doc = crate::epub::parse_xml(&content).ok()?;

        let svg_ns = "http://www.w3.org/2000/svg";
        let xlink_ns = "http://www.w3.org/1999/xlink";

        // Find <img src="..."> in body
        let body = doc.descendants().find(|n| n.tag_name().name() == "body");
        if let Some(body) = body {
            let img_srcs: Vec<String> = body
                .descendants()
                .filter(|n| n.tag_name().name() == "img")
                .filter_map(|n| n.attribute("src").map(|s| s.to_string()))
                .collect();

            if img_srcs.len() > 1 {
                // Check if all point to the same image
                if img_srcs.iter().all(|s| s == &img_srcs[0]) {
                    return Some(resolve_root_path(filename, &img_srcs[0]));
                }
                // Pick the largest by file size
                let largest = img_srcs
                    .iter()
                    .filter_map(|src| {
                        let resolved = resolve_root_path(filename, src);
                        let size = archive.entry_size(&resolved)?;
                        Some((resolved, size))
                    })
                    .max_by_key(|(_, size)| *size);
                return largest.map(|(path, _)| path);
            } else if img_srcs.len() == 1 {
                return Some(resolve_root_path(filename, &img_srcs[0]));
            }

            // Try SVG image with xlink:href
            for node in body.descendants() {
                if node.tag_name().name() == "image"
                    && node
                        .tag_name()
                        .namespace()
                        .map_or(false, |ns| ns == svg_ns)
                {
                    if let Some(href) = node.attribute((xlink_ns, "href")) {
                        return Some(resolve_root_path(filename, href));
                    }
                }
            }
        }

        Some(String::new())
    } else if is_image_extension(filename) {
        Some(filename.to_string())
    } else {
        Some(String::new())
    }
}

pub fn parse_alternative_cover(
    archive: &mut EpubArchive,
    opf_content: &str,
    book_full: &mut Vec<EpubPage>,
    opf_path: &str,
) -> Result<()> {
    let doc = crate::epub::parse_xml(opf_content)?;

    // Find cover id from metadata
    let mut cover_id = String::new();
    for node in doc.descendants() {
        if node.tag_name().name() == "meta" && node.attribute("name") == Some("cover") {
            if let Some(content) = node.attribute("content") {
                cover_id = content.to_string();
                break;
            }
        }
    }

    if cover_id.is_empty() {
        return Ok(());
    }

    // Find cover href from manifest
    let mut cover_href = String::new();
    for node in doc.descendants() {
        if node.tag_name().name() == "item" && node.attribute("id") == Some(&cover_id) {
            if let Some(href) = node.attribute("href") {
                cover_href = href.to_string();
                break;
            }
        }
    }

    if cover_href.is_empty() || !is_image_extension(&cover_href) {
        return Ok(());
    }

    let resolved = resolve_root_path(opf_path, &cover_href);
    if !archive.entry_exists(&resolved) {
        return Ok(());
    }

    let first_image = book_full.first().map(|p| p.image.as_str()).unwrap_or("");
    if resolved == first_image {
        return Ok(());
    }

    let data = archive.read_entry(&resolved)?;
    let (width, height) = imgproc::get_image_dimensions(&data);
    let size = data.len() as u64;

    book_full.insert(
        0,
        EpubPage {
            page: "Cover".to_string(),
            image: resolved,
            height,
            width,
            size,
            ..Default::default()
        },
    );

    Ok(())
}

pub fn is_open_manga_format(opf_content: &str) -> bool {
    let doc = match crate::epub::parse_xml(opf_content) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let root = doc.root_element();
    root.tag_name().name() == "package"
        && root
            .attribute("prefix")
            .map(|p| p.to_lowercase().contains("openmangaformat.org"))
            .unwrap_or(false)
}

pub fn parse_omf_content_js(
    archive: &mut EpubArchive,
    opf_path: &str,
) -> Result<Vec<EpubPage>> {
    let opf_doc = match archive.read_entry_string(opf_path) {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()),
    };
    let doc = crate::epub::parse_xml(&opf_doc)?;

    // Find content.js in the manifest
    let mut js_href = String::new();
    for node in doc.descendants() {
        if node.tag_name().name() == "item" {
            let media = node.attribute("media-type").unwrap_or("");
            if media.contains("javascript") {
                if let Some(href) = node.attribute("href") {
                    if href.to_lowercase().contains("content") {
                        js_href = href.to_string();
                        break;
                    }
                }
            }
        }
    }

    if js_href.is_empty() {
        return Ok(Vec::new());
    }

    let js_path = resolve_root_path(opf_path, &js_href);
    let content = archive.read_entry_string(&js_path)?;

    let re = regex::Regex::new(
        r#"addPageConfig\(\s*(\d+)\s*,\s*"([^"]+)"\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*"([^"]*)"#,
    )?;

    let mut pages = Vec::new();
    let js_dir = std::path::Path::new(&js_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    for caps in re.captures_iter(&content) {
        let image_rel = &caps[2];
        let width: u32 = caps[3].parse().unwrap_or(0);
        let height: u32 = caps[4].parse().unwrap_or(0);
        let spread = caps[5].to_string();

        let image_path = if js_dir.is_empty() {
            image_rel.to_string()
        } else {
            format!("{js_dir}/{image_rel}")
        };

        const WIDE_IMAGE_RATIO: f64 = 1.125;
        let is_double = width as f64 >= height as f64 * WIDE_IMAGE_RATIO && width > 0 && height > 0;

        let size = archive.entry_size(&image_path).unwrap_or(0);

        pages.push(EpubPage {
            page: caps[1].to_string(),
            image: image_path,
            spread: match spread.as_str() {
                "left" => "page-spread-left".to_string(),
                "right" => "page-spread-right".to_string(),
                _ => String::new(),
            },
            doublepage: is_double,
            width,
            height,
            size,
            ..Default::default()
        });
    }

    Ok(pages)
}
