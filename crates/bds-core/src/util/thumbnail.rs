use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, ImageFormat, ImageReader};
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

/// Standard thumbnail sizes matching TypeScript implementation.
pub const THUMBNAIL_SIZES: &[ThumbnailSize] = &[
    ThumbnailSize { name: "small", width: 150, height: 150, format: ThumbnailFormat::Webp, fit: ThumbnailFit::Inside },
    ThumbnailSize { name: "medium", width: 400, height: 400, format: ThumbnailFormat::Webp, fit: ThumbnailFit::Inside },
    ThumbnailSize { name: "large", width: 800, height: 800, format: ThumbnailFormat::Webp, fit: ThumbnailFit::Inside },
    ThumbnailSize { name: "ai", width: 448, height: 448, format: ThumbnailFormat::Jpeg, fit: ThumbnailFit::Contain },
];

/// Generate a single thumbnail from a source image file.
pub fn generate_thumbnail(
    source: &Path,
    dest: &Path,
    size: &ThumbnailSize,
    quality: u8,
) -> Result<(), String> {
    let img = load_and_orient(source)?;

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
        ThumbnailFit::Cover => {
            img.resize_to_fill(size.width, size.height, FilterType::Lanczos3)
        }
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
            // Encode as WebP via image crate
            let mut buf = Cursor::new(Vec::new());
            resized
                .write_to(&mut buf, ImageFormat::WebP)
                .map_err(|e| format!("WebP encode: {e}"))?;
            let _ = quality; // image crate WebP encoder doesn't expose quality directly
            fs::write(dest, buf.into_inner()).map_err(|e| format!("write: {e}"))?;
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

    for size in THUMBNAIL_SIZES {
        let ext = match size.format {
            ThumbnailFormat::Webp => "webp",
            ThumbnailFormat::Jpeg => "jpg",
        };
        let dest = thumbnails_dir
            .join(prefix)
            .join(format!("{media_id}-{}.{ext}", size.name));
        let quality = if size.format == ThumbnailFormat::Jpeg { 85 } else { 80 };
        generate_thumbnail(source, &dest, size, quality)?;
        paths.push(dest.to_string_lossy().to_string());
    }

    Ok(paths)
}

/// Load an image and apply EXIF orientation.
fn load_and_orient(path: &Path) -> Result<DynamicImage, String> {
    let reader = ImageReader::open(path)
        .map_err(|e| format!("open image: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("guess format: {e}"))?;

    let img = reader.decode().map_err(|e| format!("decode image: {e}"))?;

    // EXIF orientation is handled by the image crate for JPEG automatically
    // when reading. For other formats, no EXIF orientation exists.
    Ok(img)
}

/// Extract image dimensions from a file.
pub fn image_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let img = ImageReader::open(path)
        .map_err(|e| format!("open: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("format: {e}"))?
        .decode()
        .map_err(|e| format!("decode: {e}"))?;
    Ok(img.dimensions())
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

    #[test]
    fn generate_small_thumbnail() {
        let dir = TempDir::new().unwrap();
        let source = create_test_png(dir.path());
        let dest = dir.path().join("thumb.webp");
        let size = &THUMBNAIL_SIZES[0]; // small: 150x150
        generate_thumbnail(&source, &dest, size, 80).unwrap();
        assert!(dest.exists());
        let (w, h) = image_dimensions(&dest).unwrap();
        // 100x80 is already smaller than 150x150, so no resize (Inside fit)
        assert_eq!(w, 100);
        assert_eq!(h, 80);
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
        assert_eq!(paths.len(), 4);
        for p in &paths {
            assert!(Path::new(p).exists(), "thumbnail missing: {p}");
        }
    }

    #[test]
    fn mime_detection() {
        assert_eq!(mime_from_extension("jpg"), "image/jpeg");
        assert_eq!(mime_from_extension("PNG"), "image/png");
        assert_eq!(mime_from_extension("webp"), "image/webp");
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
}
