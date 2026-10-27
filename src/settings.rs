use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // ComicInfo fields
    pub comic_info: bool,
    pub extract_images: bool,
    pub series: bool,
    pub title: bool,
    pub volume: bool,
    pub page_count: bool,
    pub author: bool,
    pub producer: bool,
    pub translator: bool,
    pub language: bool,
    pub publisher: bool,
    pub date: bool,
    pub description: bool,
    pub reading_direction: bool,
    pub isbn_asin: bool,
    pub chapters: bool,
    pub image_size: bool,
    pub file_size: bool,

    // Processing options
    pub duplicate_cover: bool,
    pub every_page_is_chapter: bool,
    pub page_spread: bool,
    pub split_page_spread: bool,
    pub blank_image: bool,
    pub higher_resolution_cover: bool,
    pub insert_additional_blank: bool,
    pub metadata_title: bool,
    pub chapter_folders: bool,
    pub remove_first_page: bool,
    pub add_alternative_cover: bool,
    pub offset_chapters: bool,
    pub replace_series: bool,
    pub replace_series_value: String,
    pub drm_protection: bool,
    pub simple_extraction: bool,

    // Output format
    pub output_zip: bool,
    pub output_format_jpg: bool,
    pub output_format_webp: bool,

    // ComicInfo pages
    pub comic_info_pages: bool,

    // Compression
    pub compression_level: u8,

    // Parallelism
    pub parallelism: usize,

    // Resize
    pub resize_images: bool,
    pub resize_width: u32,
    pub resize_height: u32,

    // Crop
    pub crop_images: bool,
    pub crop_padding: u32,
    pub crop_color_tolerance: u8,
    pub crop_deviation_tolerance: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            comic_info: true,
            extract_images: true,
            series: true,
            title: true,
            volume: true,
            page_count: true,
            author: true,
            producer: true,
            translator: true,
            language: true,
            publisher: true,
            date: true,
            description: true,
            reading_direction: true,
            isbn_asin: true,
            chapters: true,
            image_size: true,
            file_size: true,
            duplicate_cover: true,
            every_page_is_chapter: true,
            page_spread: true,
            split_page_spread: true,
            blank_image: true,
            higher_resolution_cover: true,
            insert_additional_blank: false,
            metadata_title: false,
            chapter_folders: false,
            remove_first_page: false,
            add_alternative_cover: false,
            offset_chapters: false,
            replace_series: false,
            replace_series_value: String::new(),
            drm_protection: true,
            simple_extraction: false,
            output_zip: false,
            output_format_jpg: true,
            output_format_webp: false,
            comic_info_pages: true,
            compression_level: 0,
            parallelism: 0,
            resize_images: false,
            resize_width: 0,
            resize_height: 0,
            crop_images: false,
            crop_padding: 5,
            crop_color_tolerance: 15,
            crop_deviation_tolerance: 1,
        }
    }
}

impl Settings {
    pub fn config_path() -> PathBuf {
        let config_dir = dirs::config_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("epub2cbz");
        config_dir.join("epub2cbz.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    pub fn image_extension(&self) -> String {
        if self.output_format_webp {
            ".webp".to_string()
        } else if self.output_format_jpg {
            ".jpg".to_string()
        } else {
            ".png".to_string()
        }
    }

    pub fn archive_extension(&self) -> &str {
        if self.output_zip {
            ".zip"
        } else {
            ".cbz"
        }
    }

    pub fn zip_compression(&self) -> zip::CompressionMethod {
        match self.compression_level {
            0 => zip::CompressionMethod::Stored,
            _ => zip::CompressionMethod::Deflated,
        }
    }

    pub fn max_parallelism(&self) -> usize {
        if self.parallelism > 0 {
            self.parallelism
        } else {
            std::thread::available_parallelism()
                .map(|n| n.get().saturating_sub(1).max(1))
                .unwrap_or(1)
        }
    }
}
