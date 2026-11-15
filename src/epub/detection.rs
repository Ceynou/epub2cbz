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
    // The authoritative DRM signal is the EPUB encryption manifest. Entries for
    // font obfuscation are not DRM and must not block conversion.
    if archive.entry_exists("META-INF/encryption.xml") {
        let xml = archive
            .read_entry_string("META-INF/encryption.xml")
            .map_err(|_| "Unable to read encryption manifest".to_string())?;
        if encryption_protects_content(&xml) {
            return Err("Content appears DRM-protected".to_string());
        }
    }

    // As a fallback, verify that the actual payloads are readable. A DRM'd book
    // either fails to decrypt or decrypts to bytes that are not valid HTML/images.
    let names = archive.entry_names();
    let mut has_content = false;
    let mut has_readable_content = false;
    let mut has_images = false;
    let mut has_readable_image = false;

    for name in &names {
        let lower = name.to_lowercase();
        if lower.ends_with(".xhtml") || lower.ends_with(".html") {
            has_content = true;
            if let Ok(content) = archive.read_entry_string(name) {
                if content.to_lowercase().contains("<html") {
                    has_readable_content = true;
                }
            }
        } else if types::is_image_extension(name) {
            has_images = true;
            if let Ok(header) = archive.read_entry_prefix(name, 12) {
                if is_known_image(&header) {
                    has_readable_image = true;
                }
            }
        }
    }

    if has_content && !has_readable_content {
        return Err("Content appears DRM-protected".to_string());
    }
    if has_images && !has_readable_image {
        return Err("Image content appears DRM-protected".to_string());
    }

    Ok(())
}

fn is_known_image(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    let is_jpeg = data[0] == 0xFF && data[1] == 0xD8 && data[2] == 0xFF;
    let is_png = data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47;
    let is_gif = data[0] == 0x47 && data[1] == 0x49 && data[2] == 0x46 && data[3] == 0x38;
    let is_webp = data[0] == 0x52
        && data[1] == 0x49
        && data[2] == 0x46
        && data[3] == 0x46
        && data[8] == 0x57
        && data[9] == 0x45
        && data[10] == 0x42
        && data[11] == 0x50;

    is_jpeg || is_png || is_gif || is_webp
}

fn encryption_protects_content(xml: &str) -> bool {
    let Ok(doc) = crate::epub::parse_xml(xml) else {
        // A malformed encryption manifest should be treated as suspicious.
        return !xml.trim().is_empty();
    };

    for encrypted in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "EncryptedData")
    {
        let algorithm = encrypted
            .descendants()
            .find(|n| n.tag_name().name() == "EncryptionMethod")
            .and_then(|n| n.attribute("Algorithm"))
            .unwrap_or("");

        // IDPF and Adobe font obfuscation, not DRM.
        if algorithm.contains("www.idpf.org/2008/embedding")
            || algorithm.contains("ns.adobe.com/pdf/enc")
        {
            continue;
        }

        if encrypted
            .descendants()
            .any(|n| n.tag_name().name() == "CipherReference")
        {
            return true;
        }
    }

    false
}

pub fn is_fixed_layout_epub(opf_content: &str, archive: &mut EpubArchive) -> bool {
    let doc = match crate::epub::parse_xml(opf_content) {
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
            if let Ok(doc) = crate::epub::parse_xml(&content) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_obfuscation_is_not_drm() {
        let xml = r#"<?xml version="1.0"?>
            <encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
                        xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
              <enc:EncryptedData>
                <enc:EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/>
                <enc:CipherData>
                  <enc:CipherReference URI="OEBPS/fonts/font.otf"/>
                </enc:CipherData>
              </enc:EncryptedData>
            </encryption>"#;
        assert!(!encryption_protects_content(xml));
    }

    #[test]
    fn encrypted_content_is_drm() {
        let xml = r#"<?xml version="1.0"?>
            <encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
                        xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
              <enc:EncryptedData>
                <enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes256-cbc"/>
                <enc:CipherData>
                  <enc:CipherReference URI="OEBPS/images/page1.jpg"/>
                </enc:CipherData>
              </enc:EncryptedData>
            </encryption>"#;
        assert!(encryption_protects_content(xml));
    }

    #[test]
    fn recognizes_common_image_formats() {
        assert!(is_known_image(&[0xFF, 0xD8, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(is_known_image(&[0x89, 0x50, 0x4E, 0x47, 0, 0, 0, 0, 0, 0, 0, 0]));
        assert!(!is_known_image(b"not an image"));
    }
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
