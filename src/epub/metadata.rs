use crate::settings::Settings;
use crate::types::*;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

const OPF_NS: &str = "http://www.idpf.org/2007/opf";
const DC_NS: &str = "http://purl.org/dc/elements/1.1/";

pub fn parse_metadata_xml(
    opf_content: &str,
    settings: &Settings,
) -> Result<EpubMetadata> {
    let doc = roxmltree::Document::parse(opf_content)?;
    let mut metadata = EpubMetadata::default();

    // Reading direction from spine
    for node in doc.descendants() {
        if node.tag_name().name() == "spine" {
            if node.attribute("page-progression-direction") == Some("rtl") {
                metadata.reading_direction = ReadingDirection::RightToLeft;
            }
            break;
        }
    }

    if !settings.comic_info {
        return Ok(metadata);
    }

    let metadata_node = match doc
        .descendants()
        .find(|n| n.tag_name().name() == "metadata")
    {
        Some(m) => m,
        None => return Ok(metadata),
    };

    if settings.translator {
        let val = return_metadata_contributors("trl", &metadata_node);
        if !val.is_empty() {
            metadata.fields.insert("Translators".to_string(), val);
        }
    }
    if settings.producer {
        let val = return_metadata_contributors("pro", &metadata_node);
        if !val.is_empty() {
            metadata.fields.insert("Producers".to_string(), val);
        }
    }
    if settings.isbn_asin {
        let val = return_metadata_isbn(&metadata_node);
        if !val.is_empty() {
            metadata.fields.insert("ISBN".to_string(), val);
        }
    }

    let mut title: Option<String> = None;
    let mut publisher: Option<String> = None;
    let mut date: Option<String> = None;
    let mut description: Option<String> = None;
    let mut series: Option<String> = None;
    let mut series_index: Option<String> = None;
    let mut authors: HashSet<String> = HashSet::new();
    let mut languages: Vec<String> = Vec::new();

    for child in metadata_node.children().filter(|n| n.is_element()) {
        let ns = child.tag_name().namespace().unwrap_or("");
        let local = child.tag_name().name();
        let value = child.text().unwrap_or("").trim().to_string();

        if ns == DC_NS {
            match local {
                "title" if title.is_none() => title = Some(value),
                "publisher" if publisher.is_none() => publisher = Some(value),
                "date" if date.is_none() => date = Some(value),
                "description" if description.is_none() => description = Some(value),
                "creator" if !value.is_empty() => {
                    authors.insert(value);
                }
                "language" if !value.is_empty() => {
                    let lang = if value.len() > 2 {
                        value[..2].to_string()
                    } else {
                        value
                    };
                    languages.push(lang);
                }
                _ => {}
            }
        } else if local == "meta" {
            let name = child.attribute("name").unwrap_or("");
            let content = child
                .attribute("content")
                .or_else(|| child.text())
                .unwrap_or("")
                .trim();

            match name {
                "calibre:series" => series = Some(content.to_string()),
                "calibre:series_index" => series_index = Some(content.to_string()),
                _ => {}
            }
        }
    }

    if settings.title || settings.metadata_title {
        if let Some(ref t) = title {
            metadata.fields.insert("Title".to_string(), t.clone());
        }
    }
    if settings.series {
        if let Some(ref s) = series {
            metadata.fields.insert("Series".to_string(), s.clone());
        }
    }
    if settings.volume {
        if let Some(ref si) = series_index {
            metadata.fields.insert("SeriesIndex".to_string(), si.clone());
        }
    }
    if settings.author && !authors.is_empty() {
        let authors_str: Vec<&str> = authors.iter().map(|s| s.as_str()).collect();
        metadata
            .fields
            .insert("Authors".to_string(), authors_str.join(" & "));
    }
    if settings.language {
        let lang = if languages.contains(&"en".to_string()) {
            "en"
        } else if languages.contains(&"ja".to_string()) {
            "ja"
        } else {
            languages.first().map(|s| s.as_str()).unwrap_or("")
        };
        if !lang.is_empty() {
            metadata.fields.insert("Language".to_string(), lang.to_string());
        }
    }
    if settings.publisher {
        if let Some(ref p) = publisher {
            metadata.fields.insert("Publisher".to_string(), p.clone());
        }
    }
    if settings.date {
        if let Some(ref d) = date {
            let trimmed = if d.len() > 10 { &d[..10] } else { d };
            metadata
                .fields
                .insert("Date".to_string(), trimmed.to_string());
        }
    }
    if settings.description {
        if let Some(ref desc) = description {
            let cleaned = regex::Regex::new(r"\s{2,}")
                .unwrap()
                .replace_all(desc, "\n")
                .trim()
                .to_string();
            metadata.fields.insert("Description".to_string(), cleaned);
        }
    }

    Ok(metadata)
}

fn return_metadata_contributors(role: &str, metadata_node: &roxmltree::Node) -> String {
    let mut target_ids: Vec<String> = Vec::new();
    let mut contributor_map: HashMap<String, String> = HashMap::new();

    for child in metadata_node.children().filter(|n| n.is_element()) {
        let local = child.tag_name().name();
        let ns = child.tag_name().namespace().unwrap_or("");

        if local == "meta" {
            let property = child.attribute("property").unwrap_or("");
            let scheme = child.attribute("scheme").unwrap_or("");
            let value = child.text().unwrap_or("").trim();

            if property == "role"
                && scheme == "marc:relators"
                && value.eq_ignore_ascii_case(role)
            {
                if let Some(refines) = child.attribute("refines") {
                    let id = refines.strip_prefix('#').unwrap_or(refines);
                    target_ids.push(id.to_string());
                }
            }
        } else if ns == DC_NS && local == "contributor" {
            if let Some(id) = child.attribute("id") {
                let value = child.text().unwrap_or("").trim().to_string();
                contributor_map.insert(id.to_string(), value);
            }
        }
    }

    if target_ids.is_empty() {
        return String::new();
    }

    let mut matched: HashSet<String> = HashSet::new();
    for id in &target_ids {
        if let Some(name) = contributor_map.get(id) {
            matched.insert(name.clone());
        }
    }
    let items: Vec<&str> = matched.iter().map(|s| s.as_str()).collect();
    items.join(" & ")
}

fn return_metadata_isbn(metadata_node: &roxmltree::Node) -> String {
    let mut asin: Option<String> = None;
    let mut identifier: Option<String> = None;
    let mut asin_identifier: Option<String> = None;
    let mut source: Option<String> = None;

    for child in metadata_node.children().filter(|n| n.is_element()) {
        let local = child.tag_name().name();
        let ns = child.tag_name().namespace().unwrap_or("");

        if local == "meta" {
            if asin.is_none() && child.attribute("name") == Some("ASIN") {
                asin = child.attribute("content").map(|s| s.to_string());
            }
        } else if ns == DC_NS && local == "identifier" {
            let value = child.text().unwrap_or("").trim().to_string();
            if identifier.is_none() {
                identifier = Some(value.clone());
            }
            if asin_identifier.is_none()
                && child.attribute((OPF_NS, "scheme")) == Some("MOBI-ASIN")
            {
                asin_identifier = Some(value);
            }
        } else if ns == DC_NS && local == "source" && source.is_none() {
            source = child.text().map(|s| s.trim().to_string());
        }
    }

    if let Some(ref a) = asin {
        if a.len() == 10 {
            return format!("ASIN: {a}");
        }
    }
    if let Some(ref a) = asin_identifier {
        if a.len() == 10 {
            return format!("ASIN: {a}");
        }
    }
    if let Some(ref id) = identifier {
        let lower = id.to_lowercase();
        if lower.starts_with("urn:asin:") && id.len() == 19 {
            return format!("ASIN: {}", &id[9..]);
        }
    }

    if let Some(ref id) = identifier {
        let lower = id.to_lowercase();
        if !lower.starts_with("urn:uuid:") && !lower.starts_with("calibre:") {
            let result = calculate_isbn(id);
            if !result.is_empty() {
                return result;
            }
        }
    }

    if let Some(ref src) = source {
        if src.to_lowercase().starts_with("urn:isbn:") {
            return calculate_isbn(src);
        }
    }

    String::new()
}

fn calculate_isbn(source: &str) -> String {
    let cleaned: String = source
        .to_lowercase()
        .replace("urn:isbn:", "")
        .replace('-', "")
        .replace(' ', "")
        .trim()
        .to_string();

    if cleaned.len() == 13 && (cleaned.starts_with("978") || cleaned.starts_with("979")) {
        return format!("ISBN: {cleaned}");
    }

    if cleaned.len() == 10 && check_isbn10(&cleaned) {
        let checksum = calculate_isbn13_checksum(&cleaned[..9]);
        return format!("ISBN: 978{}{checksum}", &cleaned[..9]);
    }

    String::new()
}

fn check_isbn10(id: &str) -> bool {
    let chars: Vec<char> = id.chars().collect();
    if chars.len() != 10 {
        return false;
    }
    let mut checksum: i32 = 0;
    for i in 0..9 {
        let digit = match chars[i].to_digit(10) {
            Some(d) => d as i32,
            None => return false,
        };
        checksum += digit * (10 - i as i32);
    }
    checksum = 11 - (checksum % 11);
    let check_char = if checksum == 11 {
        '0'
    } else if checksum == 10 {
        'X'
    } else {
        char::from_digit(checksum as u32, 10).unwrap_or('?')
    };
    check_char == chars[9].to_ascii_uppercase()
}

fn calculate_isbn13_checksum(first_nine: &str) -> char {
    let full = format!("978{first_nine}");
    let mut checksum: i32 = 0;
    for (i, ch) in full.chars().enumerate().take(12) {
        let digit = ch.to_digit(10).unwrap_or(0) as i32;
        checksum += digit * if i % 2 == 0 { 1 } else { 3 };
    }
    checksum = 10 - (checksum % 10);
    if checksum == 10 {
        '0'
    } else {
        char::from_digit(checksum as u32, 10).unwrap_or('0')
    }
}
