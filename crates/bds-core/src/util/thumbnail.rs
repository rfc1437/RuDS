use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageReader, RgbImage, RgbaImage};
use std::fs;
use std::io::Cursor;
use std::path::Path;

/// Thumbnail size configuration.
pub struct ThumbnailSize {
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub format: ThumbnailFormat,
    pub fit: ThumbnailFit,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ThumbnailFormat {
    Webp,
    Jpeg,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ThumbnailFit {
    /// Scale to fit inside, no enlargement.
    Inside,
    /// Scale to fill, crop center (cover).
    Cover,
    /// Scale to fit, letterbox with black background.
    Contain,
}

/// Standard thumbnail sizes matching spec: small/medium/large are
/// width-constrained aspect-preserving; AI is letterboxed on black.
pub const THUMBNAIL_SIZES: &[ThumbnailSize] = &[
    ThumbnailSize {
        name: "small",
        width: 150,
        height: u32::MAX,
        format: ThumbnailFormat::Webp,
        fit: ThumbnailFit::Inside,
    },
    ThumbnailSize {
        name: "medium",
        width: 400,
        height: u32::MAX,
        format: ThumbnailFormat::Webp,
        fit: ThumbnailFit::Inside,
    },
    ThumbnailSize {
        name: "large",
        width: 800,
        height: u32::MAX,
        format: ThumbnailFormat::Webp,
        fit: ThumbnailFit::Inside,
    },
    ThumbnailSize {
        name: "ai",
        width: 448,
        height: 448,
        format: ThumbnailFormat::Jpeg,
        fit: ThumbnailFit::Contain,
    },
];

/// Generate a single thumbnail from a source image file.
pub fn generate_thumbnail(
    source: &Path,
    dest: &Path,
    size: &ThumbnailSize,
    quality: u8,
) -> Result<(), String> {
    let img = load_and_orient(source)?;
    generate_thumbnail_from_image(&img, dest, size, quality)
}

fn generate_thumbnail_from_image(
    img: &DynamicImage,
    dest: &Path,
    size: &ThumbnailSize,
    quality: u8,
) -> Result<(), String> {
    let resized = match size.fit {
        ThumbnailFit::Inside => {
            let (orig_w, orig_h) = img.dimensions();
            // Don't enlarge
            if orig_w <= size.width && orig_h <= size.height {
                img.clone()
            } else {
                img.resize(size.width, size.height, FilterType::Lanczos3)
            }
        }
        ThumbnailFit::Cover => img.resize_to_fill(size.width, size.height, FilterType::Lanczos3),
        ThumbnailFit::Contain => {
            // Resize to fit, then place on black background
            let resized = img.resize(size.width, size.height, FilterType::Lanczos3);
            let (rw, rh) = resized.dimensions();
            let mut canvas = DynamicImage::new_rgb8(size.width, size.height);
            let offset_x = (size.width - rw) / 2;
            let offset_y = (size.height - rh) / 2;
            image::imageops::overlay(&mut canvas, &resized, offset_x as i64, offset_y as i64);
            canvas
        }
    };

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create dir: {e}"))?;
    }

    match size.format {
        ThumbnailFormat::Webp => {
            let rgba = resized.to_rgba8();
            let encoded = webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height())
                .encode_simple(false, f32::from(quality))
                .map_err(|e| format!("WebP encode: {e:?}"))?;
            fs::write(dest, &*encoded).map_err(|e| format!("write: {e}"))?;
        }
        ThumbnailFormat::Jpeg => {
            let rgb = resized.to_rgb8();
            let mut buf = Cursor::new(Vec::new());
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
            encoder
                .encode_image(&rgb)
                .map_err(|e| format!("JPEG encode: {e}"))?;
            fs::write(dest, buf.into_inner()).map_err(|e| format!("write: {e}"))?;
        }
    }

    Ok(())
}

/// Generate all standard thumbnails for a media item.
pub fn generate_all_thumbnails(
    source: &Path,
    thumbnails_dir: &Path,
    media_id: &str,
) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    let prefix = &media_id[..2.min(media_id.len())];
    let image = load_and_orient(source)?;

    for size in THUMBNAIL_SIZES {
        let ext = match size.format {
            ThumbnailFormat::Webp => "webp",
            ThumbnailFormat::Jpeg => "jpg",
        };
        let dest = thumbnails_dir
            .join(prefix)
            .join(format!("{media_id}-{}.{ext}", size.name));
        let quality = if size.format == ThumbnailFormat::Jpeg {
            85
        } else {
            80
        };
        generate_thumbnail_from_image(&image, &dest, size, quality)?;
        paths.push(dest.to_string_lossy().to_string());
    }

    Ok(paths)
}

/// Load an image and apply EXIF orientation.
fn load_and_orient(path: &Path) -> Result<DynamicImage, String> {
    if is_heic_path(path) {
        return load_heic(path);
    }

    let reader = ImageReader::open(path)
        .map_err(|e| format!("open image: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("guess format: {e}"))?;

    let mut img = reader.decode().map_err(|e| format!("decode image: {e}"))?;

    // Try to read EXIF orientation from JPEG files
    if let Ok(data) = fs::read(path)
        && let Some(orientation) = read_exif_orientation(&data)
    {
        img = apply_orientation(img, orientation);
    }

    Ok(img)
}

fn is_heic_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "heic" | "heif"))
}

fn load_heic(path: &Path) -> Result<DynamicImage, String> {
    let bytes = fs::read(path).map_err(|error| format!("read HEIC image: {error}"))?;
    let decoded = hpvcd::Decoder::default()
        .with_decode_gain_map(false)
        .decode(&bytes)
        .map_err(|error| format!("decode HEIC image: {error}"))?;
    let shift = decoded.bit_depth.minus8();
    let rgb = match decoded.pixels {
        hpvcd::ImageBuffer::Rgb8(pixels) => pixels,
        hpvcd::ImageBuffer::Rgb16(samples) => samples
            .into_iter()
            .map(|sample| (sample >> shift) as u8)
            .collect(),
        hpvcd::ImageBuffer::Luma8(samples) => {
            samples.into_iter().flat_map(|sample| [sample; 3]).collect()
        }
        hpvcd::ImageBuffer::Luma16(samples) => samples
            .into_iter()
            .flat_map(|sample| [(sample >> shift) as u8; 3])
            .collect(),
    };
    let mut image = if let Some(alpha) = decoded.alpha {
        let alpha: Vec<u8> = match alpha {
            hpvcd::SampleBuf::U8(samples) => samples,
            hpvcd::SampleBuf::U16(samples) => samples
                .into_iter()
                .map(|sample| (sample >> shift) as u8)
                .collect(),
        };
        if alpha.len().checked_mul(3) != Some(rgb.len()) {
            return Err("decode HEIC image: decoder returned an invalid alpha buffer".to_string());
        }
        let rgba = rgb
            .chunks_exact(3)
            .zip(alpha)
            .flat_map(|(rgb, alpha)| [rgb[0], rgb[1], rgb[2], alpha])
            .collect();
        DynamicImage::ImageRgba8(
            RgbaImage::from_raw(decoded.width, decoded.height, rgba).ok_or_else(|| {
                "decode HEIC image: decoder returned an invalid pixel buffer".to_string()
            })?,
        )
    } else {
        DynamicImage::ImageRgb8(
            RgbImage::from_raw(decoded.width, decoded.height, rgb).ok_or_else(|| {
                "decode HEIC image: decoder returned an invalid pixel buffer".to_string()
            })?,
        )
    };

    if let Some(aperture) = decoded.clean_aperture
        && let Some((x, y, width, height)) =
            clean_aperture_crop(decoded.width, decoded.height, decoded.orientation, aperture)
    {
        image = image.crop_imm(x, y, width, height);
    }

    Ok(image)
}

fn clean_aperture_crop(
    oriented_width: u32,
    oriented_height: u32,
    orientation: hpvcd::Orientation,
    aperture: hpvcd::CleanAperture,
) -> Option<(u32, u32, u32, u32)> {
    let width = aperture.width_pixels()?;
    let height = aperture.height_pixels()?;
    if width == 0 || height == 0 {
        return None;
    }

    let swaps_axes = matches!(
        orientation,
        hpvcd::Orientation::Rotate90
            | hpvcd::Orientation::Rotate270
            | hpvcd::Orientation::Transpose
            | hpvcd::Orientation::Transverse
    );
    let (source_width, source_height) = if swaps_axes {
        (oriented_height, oriented_width)
    } else {
        (oriented_width, oriented_height)
    };
    if width > source_width || height > source_height {
        return None;
    }

    let horizontal_offset =
        f64::from(aperture.horiz_off_n) / f64::from(aperture.horiz_off_d.max(1));
    let vertical_offset = f64::from(aperture.vert_off_n) / f64::from(aperture.vert_off_d.max(1));
    let source_x = ((f64::from(source_width - width) / 2.0) + horizontal_offset)
        .round()
        .clamp(0.0, f64::from(source_width - width)) as u32;
    let source_y = ((f64::from(source_height - height) / 2.0) + vertical_offset)
        .round()
        .clamp(0.0, f64::from(source_height - height)) as u32;

    let crop = match orientation {
        hpvcd::Orientation::Normal => (source_x, source_y, width, height),
        hpvcd::Orientation::FlipH => (source_width - source_x - width, source_y, width, height),
        hpvcd::Orientation::FlipV => (source_x, source_height - source_y - height, width, height),
        hpvcd::Orientation::Rotate180 => (
            source_width - source_x - width,
            source_height - source_y - height,
            width,
            height,
        ),
        hpvcd::Orientation::Rotate90 => {
            (source_height - source_y - height, source_x, height, width)
        }
        hpvcd::Orientation::Rotate270 => (source_y, source_width - source_x - width, height, width),
        hpvcd::Orientation::Transpose => (
            source_height - source_y - height,
            source_width - source_x - width,
            height,
            width,
        ),
        hpvcd::Orientation::Transverse => (source_y, source_x, height, width),
    };
    (crop.0 + crop.2 <= oriented_width && crop.1 + crop.3 <= oriented_height).then_some(crop)
}

/// Read EXIF orientation tag from raw file bytes.
/// Returns orientation value 1-8, or None if not found.
fn read_exif_orientation(data: &[u8]) -> Option<u16> {
    if data.len() < 12 {
        return None;
    }
    // Must be JPEG: FF D8
    if data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut pos = 2;
    while pos + 4 < data.len() {
        if data[pos] != 0xFF {
            break;
        }
        let marker = data[pos + 1];
        let len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        if marker == 0xE1 && pos + 10 < data.len() {
            // Check for "Exif\0\0"
            if &data[pos + 4..pos + 10] == b"Exif\0\0" {
                let tiff_start = pos + 10;
                return parse_tiff_orientation(data, tiff_start);
            }
        }
        pos += 2 + len;
    }
    None
}

fn parse_tiff_orientation(data: &[u8], tiff_start: usize) -> Option<u16> {
    if tiff_start + 8 > data.len() {
        return None;
    }
    let is_le = data[tiff_start] == b'I' && data[tiff_start + 1] == b'I';
    let read_u16 = |offset: usize| -> Option<u16> {
        if offset + 2 > data.len() {
            return None;
        }
        if is_le {
            Some(u16::from_le_bytes([data[offset], data[offset + 1]]))
        } else {
            Some(u16::from_be_bytes([data[offset], data[offset + 1]]))
        }
    };
    let read_u32 = |offset: usize| -> Option<u32> {
        if offset + 4 > data.len() {
            return None;
        }
        if is_le {
            Some(u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]))
        } else {
            Some(u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]))
        }
    };

    let ifd_offset = read_u32(tiff_start + 4)? as usize;
    let ifd_pos = tiff_start + ifd_offset;
    let entry_count = read_u16(ifd_pos)? as usize;

    for i in 0..entry_count {
        let entry_pos = ifd_pos + 2 + i * 12;
        if entry_pos + 12 > data.len() {
            break;
        }
        let tag = read_u16(entry_pos)?;
        if tag == 0x0112 {
            // Orientation tag
            return read_u16(entry_pos + 8);
        }
    }
    None
}

fn apply_orientation(img: DynamicImage, orientation: u16) -> DynamicImage {
    match orientation {
        1 => img,                     // Normal
        2 => img.fliph(),             // Mirrored horizontal
        3 => img.rotate180(),         // Rotated 180
        4 => img.flipv(),             // Mirrored vertical
        5 => img.rotate90().fliph(),  // Mirrored horizontal + 270 CW
        6 => img.rotate90(),          // Rotated 90 CW
        7 => img.rotate270().fliph(), // Mirrored horizontal + 90 CW
        8 => img.rotate270(),         // Rotated 270 CW
        _ => img,
    }
}

/// Extract image dimensions from a file (header-only, no full decode).
pub fn image_dimensions(path: &Path) -> Result<(u32, u32), String> {
    if is_heic_path(path) {
        let bytes = fs::read(path).map_err(|error| format!("read HEIC image: {error}"))?;
        let info =
            hpvcd::read_heic_info(&bytes).map_err(|error| format!("HEIC dimensions: {error}"))?;
        let swaps_axes = matches!(
            info.orientation,
            hpvcd::Orientation::Rotate90
                | hpvcd::Orientation::Rotate270
                | hpvcd::Orientation::Transpose
                | hpvcd::Orientation::Transverse
        );
        if let Some(aperture) = info.clean_aperture
            && let (Some(width), Some(height)) = (aperture.width_pixels(), aperture.height_pixels())
            && width > 0
            && height > 0
        {
            return Ok(if swaps_axes {
                (height, width)
            } else {
                (width, height)
            });
        }
        return Ok((info.width, info.height));
    }

    let reader = ImageReader::open(path)
        .map_err(|e| format!("open: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("format: {e}"))?;
    let (w, h) = reader
        .into_dimensions()
        .map_err(|e| format!("dimensions: {e}"))?;
    Ok((w, h))
}

/// Detect MIME type from file extension.
pub fn mime_from_extension(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "tiff" | "tif" => "image/tiff",
        "bmp" => "image/bmp",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_png(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("test.png");
        // Create a small 100x80 red PNG
        let img = DynamicImage::new_rgb8(100, 80);
        img.save(&path).unwrap();
        path
    }

    fn heic_fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("close.heic")
    }

    #[test]
    fn generate_small_thumbnail() {
        let dir = TempDir::new().unwrap();
        let source = create_test_png(dir.path());
        let dest = dir.path().join("thumb.webp");
        let size = &THUMBNAIL_SIZES[0]; // small: 150 width-constrained
        generate_thumbnail(&source, &dest, size, 80).unwrap();
        assert!(dest.exists());
        let (w, h) = image_dimensions(&dest).unwrap();
        // 100x80 is smaller than 150 wide, no enlargement
        assert_eq!(w, 100);
        assert_eq!(h, 80);
        let encoded = fs::read(&dest).unwrap();
        assert_eq!(&encoded[12..16], b"VP8 ", "thumbnail must use lossy WebP");
    }

    #[test]
    fn generate_ai_thumbnail_contain() {
        let dir = TempDir::new().unwrap();
        let source = create_test_png(dir.path());
        let dest = dir.path().join("thumb-ai.jpg");
        let size = &THUMBNAIL_SIZES[3]; // ai: 448x448 contain
        generate_thumbnail(&source, &dest, size, 85).unwrap();
        assert!(dest.exists());
        let (w, h) = image_dimensions(&dest).unwrap();
        assert_eq!(w, 448);
        assert_eq!(h, 448);
    }

    #[test]
    fn generate_all() {
        let dir = TempDir::new().unwrap();
        let source = create_test_png(dir.path());
        let thumb_dir = dir.path().join("thumbnails");
        let paths = generate_all_thumbnails(&source, &thumb_dir, "ab123456-test-uuid").unwrap();
        assert_eq!(paths.len(), 4); // 4 thumbnails (no source copy)
        for p in &paths {
            assert!(Path::new(p).exists(), "thumbnail missing: {p}");
        }
    }

    #[test]
    fn mime_detection() {
        assert_eq!(mime_from_extension("jpg"), "image/jpeg");
        assert_eq!(mime_from_extension("PNG"), "image/png");
        assert_eq!(mime_from_extension("webp"), "image/webp");
        assert_eq!(mime_from_extension("HEIC"), "image/heic");
        assert_eq!(mime_from_extension("heif"), "image/heif");
        assert_eq!(mime_from_extension("xyz"), "application/octet-stream");
    }

    #[test]
    fn dimensions_extraction() {
        let dir = TempDir::new().unwrap();
        let source = create_test_png(dir.path());
        let (w, h) = image_dimensions(&source).unwrap();
        assert_eq!(w, 100);
        assert_eq!(h, 80);
    }

    #[test]
    fn heic_dimensions_and_thumbnails() {
        let dir = TempDir::new().unwrap();
        let source = heic_fixture();

        assert_eq!(image_dimensions(&source).unwrap(), (27, 27));

        let paths =
            generate_all_thumbnails(&source, &dir.path().join("thumbnails"), "heic-test").unwrap();
        assert_eq!(paths.len(), THUMBNAIL_SIZES.len());
        assert!(paths.iter().all(|path| Path::new(path).is_file()));
        assert_eq!(image_dimensions(Path::new(&paths[0])).unwrap(), (27, 27));
        assert_eq!(image_dimensions(Path::new(&paths[3])).unwrap(), (448, 448));
    }
}
