use crate::settings::Settings;
use crate::types::EpubPage;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;

pub fn get_image_dimensions(data: &[u8]) -> (u32, u32) {
    image::ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .unwrap_or((0, 0))
}

pub fn compare_images(data1: &[u8], data2: &[u8]) -> bool {
    if data2.is_empty() {
        return false;
    }
    let (Ok(h1), Ok(h2)) = (difference_hash(data1), difference_hash(data2)) else {
        return false;
    };
    hash_similarity(h1, h2) >= 97.5
}

fn difference_hash(data: &[u8]) -> anyhow::Result<u64> {
    let img = image::load_from_memory(data)?
        .resize_exact(9, 8, FilterType::Lanczos3)
        .to_luma8();
    let mut hash: u64 = 0;
    for y in 0..8u32 {
        for x in 0..8u32 {
            let left = img.get_pixel(x, y)[0];
            let right = img.get_pixel(x + 1, y)[0];
            if left > right {
                hash |= 1 << (y * 8 + x);
            }
        }
    }
    Ok(hash)
}

fn hash_similarity(h1: u64, h2: u64) -> f64 {
    let matching = 64 - (h1 ^ h2).count_ones();
    (matching as f64 / 64.0) * 100.0
}

pub fn is_image_blank_white(data: &[u8]) -> bool {
    let Ok(img) = image::load_from_memory(data) else {
        return false;
    };
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w < 5 || h < 5 {
        return false;
    }

    for y in 2..h - 2 {
        for x in 2..w - 2 {
            let p = rgba.get_pixel(x, y);
            if p[3] < 250 || p[0] < 250 || p[1] < 250 || p[2] < 250 {
                return false;
            }
        }
    }
    true
}

pub fn create_blank_image_png(width: u32, height: u32) -> Vec<u8> {
    let img = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(
        encoder,
        img.as_raw(),
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .unwrap_or(());
    buf
}

pub fn apply_cropping(img: &mut DynamicImage, settings: &Settings) -> bool {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();

    let tl = *rgba.get_pixel(0, 0);
    let tr = *rgba.get_pixel(w - 1, 0);
    let bl = *rgba.get_pixel(0, h - 1);
    let br = *rgba.get_pixel(w - 1, h - 1);

    let border_top = avg_pixel(&tl, &tr);
    let border_bottom = avg_pixel(&bl, &br);
    let border_left = avg_pixel(&tl, &bl);
    let border_right = avg_pixel(&tr, &br);

    let black = Rgba([0, 0, 0, 255]);
    let white = Rgba([255, 255, 255, 255]);

    let tolerance = settings.crop_color_tolerance;
    let padding = settings.crop_padding.min(w.min(h) / 2);
    let dev_tolerance = settings.crop_deviation_tolerance as f64 / 100.0;

    let h_threshold = (w as f64 * dev_tolerance) as u32;
    let v_threshold = (h as f64 * dev_tolerance) as u32;

    let mut top = 0u32;
    let mut bottom = h - 1;
    let mut left = 0u32;
    let mut right = w - 1;

    // Find top
    if !is_different(&border_top, &white, tolerance)
        || !is_different(&border_top, &black, tolerance)
    {
        for y in 0..h {
            let mut non_border = 0u32;
            for x in 0..w {
                if is_different(rgba.get_pixel(x, y), &border_top, tolerance) {
                    non_border += 1;
                    if h_threshold == 0 {
                        break;
                    }
                }
            }
            if non_border > h_threshold {
                top = y;
                break;
            }
        }
    }

    // Find bottom
    if !is_different(&border_bottom, &white, tolerance)
        || !is_different(&border_bottom, &black, tolerance)
    {
        for y in (top..h).rev() {
            let mut non_border = 0u32;
            for x in 0..w {
                if is_different(rgba.get_pixel(x, y), &border_bottom, tolerance) {
                    non_border += 1;
                    if h_threshold == 0 {
                        break;
                    }
                }
            }
            if non_border > h_threshold {
                bottom = y;
                break;
            }
        }
    }

    // Find left
    if !is_different(&border_left, &white, tolerance)
        || !is_different(&border_left, &black, tolerance)
    {
        for x in 0..w {
            let mut non_border = 0u32;
            for y in top..=bottom {
                if is_different(rgba.get_pixel(x, y), &border_left, tolerance) {
                    non_border += 1;
                    if v_threshold == 0 {
                        break;
                    }
                }
            }
            if non_border > v_threshold {
                left = x;
                break;
            }
        }
    }

    // Find right
    if !is_different(&border_right, &white, tolerance)
        || !is_different(&border_right, &black, tolerance)
    {
        for x in (left..w).rev() {
            let mut non_border = 0u32;
            for y in top..=bottom {
                if is_different(rgba.get_pixel(x, y), &border_right, tolerance) {
                    non_border += 1;
                    if v_threshold == 0 {
                        break;
                    }
                }
            }
            if non_border > v_threshold {
                right = x;
                break;
            }
        }
    }

    let padded_top = top.saturating_sub(padding);
    let padded_bottom = (bottom + padding).min(h - 1);
    let padded_left = left.saturating_sub(padding);
    let padded_right = (right + padding).min(w - 1);

    let crop_w = padded_right - padded_left + 1;
    let crop_h = padded_bottom - padded_top + 1;

    if crop_w == 0 || crop_h == 0 || (crop_w == w && crop_h == h) {
        return false;
    }

    *img = img.crop_imm(padded_left, padded_top, crop_w, crop_h);
    true
}

pub fn apply_resizing(img: &mut DynamicImage, max_width: u32, max_height: u32) -> bool {
    let (w, h) = img.dimensions();
    if w == max_width && h == max_height {
        return false;
    }

    let w_ratio = max_width as f64 / w as f64;
    let h_ratio = max_height as f64 / h as f64;
    let scale = w_ratio.min(h_ratio);

    let new_w = (w as f64 * scale).round() as u32;
    let new_h = (h as f64 * scale).round() as u32;

    if new_w == 0 || new_h == 0 {
        return false;
    }

    *img = img.resize(new_w, new_h, FilterType::CatmullRom);
    true
}

pub fn get_single_page_resolution(
    book_full: &[EpubPage],
    settings: &Settings,
) -> (u32, u32) {
    let mut dim_x = 0u32;
    let mut dim_y = 0u32;

    for (i, page) in book_full.iter().enumerate() {
        if !page.image.is_empty() && i > 0 && crate::types::is_image_extension(&page.image) {
            dim_x = page.width;
            dim_y = page.height;
            if dim_y > dim_x {
                break;
            }
        }
        if i == book_full.len() - 1 && dim_x > 0 {
            dim_x /= 2;
        }
    }

    if settings.resize_images && settings.resize_height > 0 && settings.resize_width > 0 {
        let w_ratio = settings.resize_width as f64 / dim_x.max(1) as f64;
        let h_ratio = settings.resize_height as f64 / dim_y.max(1) as f64;
        let scale = w_ratio.min(h_ratio);
        let rw = (dim_x as f64 * scale).round() as u32;
        let rh = (dim_y as f64 * scale).round() as u32;
        if rw > 0 && rh > 0 {
            dim_x = rw;
            dim_y = rh;
        }
    }

    (dim_x, dim_y)
}

pub fn encode_image(img: &DynamicImage, extension: &str) -> Vec<u8> {
    let mut buf = Vec::new();
    let format = match extension.to_lowercase().as_str() {
        ".png" => ImageFormat::Png,
        ".gif" => ImageFormat::Gif,
        ".webp" => ImageFormat::WebP,
        _ => ImageFormat::Jpeg,
    };
    let _ = img.write_to(&mut Cursor::new(&mut buf), format);
    buf
}

fn is_different(pixel: &Rgba<u8>, compare: &Rgba<u8>, tolerance: u8) -> bool {
    (pixel[0] as i16 - compare[0] as i16).unsigned_abs() > tolerance as u16
        || (pixel[1] as i16 - compare[1] as i16).unsigned_abs() > tolerance as u16
        || (pixel[2] as i16 - compare[2] as i16).unsigned_abs() > tolerance as u16
}

fn avg_pixel(a: &Rgba<u8>, b: &Rgba<u8>) -> Rgba<u8> {
    Rgba([
        ((a[0] as u16 + b[0] as u16) / 2) as u8,
        ((a[1] as u16 + b[1] as u16) / 2) as u8,
        ((a[2] as u16 + b[2] as u16) / 2) as u8,
        255,
    ])
}
