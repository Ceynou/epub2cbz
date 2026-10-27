use crate::settings::Settings;
use crate::types::*;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::io::Cursor;
use std::path::Path;

pub fn generate_comic_info(
    book_full: &[EpubPage],
    metadata: &EpubMetadata,
    filename: &str,
    settings: &Settings,
) -> String {
    let mut writer = Writer::new_with_indent(Cursor::new(Vec::new()), b' ', 2);
    let _ = writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)));

    let mut root = BytesStart::new("ComicInfo");
    root.push_attribute(("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"));
    root.push_attribute(("xmlns:xsd", "http://www.w3.org/2001/XMLSchema"));
    let _ = writer.write_event(Event::Start(root));

    if let Some(title) = metadata.fields.get("Title") {
        write_element(&mut writer, "Title", title);
    }
    if let Some(series) = metadata.fields.get("Series") {
        write_element(&mut writer, "Series", series);
    }

    let (volume, number) = parse_volume_number(filename, metadata);
    if !volume.is_empty() {
        write_element(&mut writer, "Volume", &volume);
    }
    if !number.is_empty() {
        write_element(&mut writer, "Number", &number);
    }

    if let Some(desc) = metadata.fields.get("Description") {
        write_element(&mut writer, "Summary", desc);
    }
    if let Some(authors) = metadata.fields.get("Authors") {
        write_element(&mut writer, "Writer", authors);
    }
    if let Some(publisher) = metadata.fields.get("Publisher") {
        write_element(&mut writer, "Publisher", publisher);
    }

    let page_count = book_full.len();
    write_element(&mut writer, "PageCount", &page_count.to_string());

    if let Some(lang) = metadata.fields.get("Language") {
        write_element(&mut writer, "LanguageISO", lang);
    }
    if let Some(isbn) = metadata.fields.get("ISBN") {
        write_element(&mut writer, "Notes", isbn);
    }

    if metadata.reading_direction.is_rtl() {
        write_element(&mut writer, "Manga", "Yes");
    } else {
        write_element(&mut writer, "Manga", "No");
    }

    if let Some(date) = metadata.fields.get("Date") {
        if date.len() >= 4 {
            write_element(&mut writer, "Year", &date[..4]);
        }
        if date.len() >= 7 {
            if let Ok(month) = date[5..7].parse::<u32>() {
                write_element(&mut writer, "Month", &month.to_string());
            }
        }
        if date.len() >= 10 {
            if let Ok(day) = date[8..10].parse::<u32>() {
                write_element(&mut writer, "Day", &day.to_string());
            }
        }
    }

    if let Some(translators) = metadata.fields.get("Translators") {
        write_element(&mut writer, "Translator", translators);
    }

    // Pages element
    if settings.comic_info_pages {
        let _ = writer.write_event(Event::Start(BytesStart::new("Pages")));
        let mut page_idx = 0u32;
        for (i, page) in book_full.iter().enumerate() {
            let mut elem = BytesStart::new("Page");
            elem.push_attribute(("Image", page_idx.to_string().as_str()));

            if !page.bookmark.is_empty() {
                elem.push_attribute(("Bookmark", page.bookmark.as_str()));
            }

            if page.width > 0 {
                elem.push_attribute(("ImageWidth", page.width.to_string().as_str()));
            }
            if page.height > 0 {
                elem.push_attribute(("ImageHeight", page.height.to_string().as_str()));
            }
            if page.size > 0 {
                elem.push_attribute(("ImageSize", page.size.to_string().as_str()));
            }

            let page_type = if i == 0 {
                "FrontCover"
            } else if page.doublepage {
                "DoublePage"
            } else {
                "Story"
            };
            elem.push_attribute(("Type", page_type));

            let _ = writer.write_event(Event::Empty(elem));
            page_idx += 1;
        }
        let _ = writer.write_event(Event::End(BytesEnd::new("Pages")));
    }

    let _ = writer.write_event(Event::End(BytesEnd::new("ComicInfo")));

    let result = writer.into_inner().into_inner();
    String::from_utf8(result).unwrap_or_default()
}

fn write_element(writer: &mut Writer<Cursor<Vec<u8>>>, name: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    let _ = writer.write_event(Event::Start(BytesStart::new(name)));
    let _ = writer.write_event(Event::Text(BytesText::new(value)));
    let _ = writer.write_event(Event::End(BytesEnd::new(name)));
}

fn parse_volume_number(
    filename: &str,
    metadata: &EpubMetadata,
) -> (String, String) {
    let mut volume = String::new();
    let mut number = String::new();

    if let Some(series_index) = metadata.fields.get("SeriesIndex") {
        number = series_index.clone();
        return (volume, number);
    }

    let stem = Path::new(filename)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    let patterns = [
        (r"[Vv](?:ol(?:ume)?\.?\s?)(\d+(?:\.\d+)?)", true),
        (r"(?:^|\s)(\d+(?:\.\d+)?)\s*$", false),
        (r"[#第](\d+(?:\.\d+)?)", false),
    ];

    for (pattern, is_vol) in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(caps) = re.captures(&stem) {
                if let Some(m) = caps.get(1) {
                    if *is_vol {
                        volume = m.as_str().to_string();
                    } else {
                        number = m.as_str().to_string();
                    }
                    break;
                }
            }
        }
    }

    (volume, number)
}
