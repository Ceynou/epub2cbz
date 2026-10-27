use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct EpubPage {
    pub page: String,
    pub image: String,
    pub spread: String,
    pub doublepage: bool,
    pub width: u32,
    pub height: u32,
    pub size: u64,
    pub bookmark: String,
    pub blank: bool,
}

#[derive(Debug, Clone, Default)]
pub struct EpubChapter {
    pub title: String,
    pub page: String,
}

#[derive(Debug, Clone, Default)]
pub struct EpubSpineEntry {
    pub pages: String,
    pub ids: String,
    pub spread: String,
}

#[derive(Debug, Clone)]
pub struct ProcessingResult {
    pub filename: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct EpubMetadata {
    pub fields: HashMap<String, String>,
    pub reading_direction: ReadingDirection,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReadingDirection {
    #[default]
    No,
    RightToLeft,
}

impl ReadingDirection {
    pub fn as_manga_value(&self) -> &str {
        match self {
            ReadingDirection::No => "No",
            ReadingDirection::RightToLeft => "YesAndRightToLeft",
        }
    }

    pub fn is_rtl(&self) -> bool {
        matches!(self, ReadingDirection::RightToLeft)
    }
}

pub const IMAGE_EXTENSIONS: &[&str] = &[".jpeg", ".jpg", ".png", ".webp", ".gif"];

pub fn is_image_extension(path: &str) -> bool {
    let lower = path.to_lowercase();
    IMAGE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

pub fn html_decode(s: &str) -> String {
    let mut result = s.to_string();
    result = result.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&quot;", "\"");
    result = result.replace("&apos;", "'");
    result = result.replace("&#39;", "'");
    result = result.replace("\u{00a0}", " ");
    result = regex::Regex::new(r"&#(\d+);")
        .unwrap()
        .replace_all(&result, |caps: &regex::Captures| {
            caps[1]
                .parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string();
    result = regex::Regex::new(r"&#x([0-9a-fA-F]+);")
        .unwrap()
        .replace_all(&result, |caps: &regex::Captures| {
            u32::from_str_radix(&caps[1], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|c| c.to_string())
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string();
    result
}

pub fn sanitize_filename(name: &str) -> String {
    let invalid: &[char] = &[
        '<', '>', ':', '"', '/', '\\', '|', '?', '*', '\0', '\x01',
    ];
    let mut result: String = name.chars().map(|c| if invalid.contains(&c) { '_' } else { c }).collect();
    while result.contains("__") {
        result = result.replace("__", "_");
    }
    result
}
