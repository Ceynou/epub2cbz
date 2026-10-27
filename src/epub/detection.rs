use super::EpubArchive;
use crate::types;
use std::io::Read;
use std::path::Path;

pub fn check_epub(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 4];
    if file.read_exact(&mut buf).is_err() {
        return false;
    }
    buf == [0x50, 0x4B, 0x03, 0x04]
}

pub fn check_drm_protection(archive: &mut EpubArchive) -> Result<(), String> {
    let names = archive.entry_names();
    for name in &names {
        let lower = name.to_lowercase();
        if lower.ends_with(".xhtml") || lower.ends_with(".html") || lower.ends_with(".xml") {
            if lower.contains("container.xml") || lower.contains("content.opf") {
                continue;
            }
            if let Ok(content) = archive.read_entry_string(name) {
                if content.to_lowercase().contains("html") {
                    return Ok(());
                } else {
                    return Err("Content appears DRM-protected".to_string());
                }
            }
        } else if types::is_image_extension(name) {
            let Ok(data) = archive.read_entry(name) else {
                continue;
            };
            if data.len() < 12 {
                continue;
            }
            let is_jpeg = data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF;
            let is_png =
                data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47;
            let is_gif =
                data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 && data[3] == 0x38;
            let is_webp = data[0] == 0x52
                && data[1] == 0x49
                && data[2] == 0x46
                && data[3] == 0x46
                && data[8] == 0x57
                && data[9] == 0x45
                && data[10] == 0x42
                && data[11] == 0x50;

            if is_jpeg || is_png || is_gif || is_webp {
                return Ok(());
            } else {
                return Err("Image content appears DRM-protected".to_string());
            }
        }
    }
    Ok(())
}

pub fn is_fixed_layout_epub(opf_content: &str, archive: &mut EpubArchive) -> bool {
    let doc = match roxmltree::Document::parse(opf_content) {
        Ok(d) => d,
        Err(_) => return false,
    };

    if is_open_manga_format(&doc) {
        return true;
    }

    let root = match doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "metadata")
    {
        Some(m) => m,
        None => return false,
    };

    if is_epub2(&doc) {
        check_epub2_fixed_layout(&root, archive)
    } else {
        check_epub3_fixed_layout(&root)
    }
}

fn check_epub3_fixed_layout(metadata: &roxmltree::Node) -> bool {
    for meta in metadata.children().filter(|n| n.tag_name().name() == "meta") {
        let name = meta.attribute("name").unwrap_or("");
        let content = meta.attribute("content").unwrap_or("");
        let property = meta.attribute("property").unwrap_or("");
        let inner = meta.text().unwrap_or("").trim();

        if name == "book-type" && content == "comic" {
            return true;
        }
        if name == "fixed-layout" && content == "true" {
            return true;
        }
        if property == "rendition:layout"
            && (inner == "pre-paginated" || inner == "fixed")
        {
            return true;
        }
    }
    false
}

fn check_epub2_fixed_layout(
    metadata: &roxmltree::Node,
    archive: &mut EpubArchive,
) -> bool {
    for meta in metadata.children().filter(|n| n.tag_name().name() == "meta") {
        let name = meta.attribute("name").unwrap_or("");
        let content = meta.attribute("content").unwrap_or("");
        if name == "BNContentKind" && content == "ComicsDRP" {
            return true;
        }
    }

    let paths = [
        "META-INF/com.kobobooks.display-options.xml",
        "META-INF/com.apple.ibooks.display-options.xml",
    ];

    for path in paths {
        if let Ok(content) = archive.read_entry_string(path) {
            if let Ok(doc) = roxmltree::Document::parse(&content) {
                let found = doc.descendants().any(|n| {
                    n.tag_name().name() == "option"
                        && n.attribute("name") == Some("fixed-layout")
                        && n.text().map(|t| t.trim()) == Some("true")
                });
                if found {
                    return true;
                }
            }
        }
    }
    false
}

fn is_epub2(doc: &roxmltree::Document) -> bool {
    let root = doc.root_element();
    root.tag_name().name() == "package" && root.attribute("version") == Some("2.0")
}

fn is_open_manga_format(doc: &roxmltree::Document) -> bool {
    let root = doc.root_element();
    root.tag_name().name() == "package"
        && root
            .attribute("prefix")
            .map(|p| p.to_lowercase().contains("openmangaformat.org"))
            .unwrap_or(false)
}
