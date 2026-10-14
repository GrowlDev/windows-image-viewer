//! File -> GPU-ready frames. Runs on loader threads, never on the UI thread.

use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use eframe::egui::ColorImage;
use image::codecs::gif::GifDecoder;
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage};

/// Largest decoded animation we keep in memory (all frames, RGBA).
const MAX_ANIMATION_BYTES: usize = 1 << 30;

pub struct Frame {
    pub image: Arc<ColorImage>,
    pub delay: Duration,
}

pub struct Decoded {
    /// One frame for still images.
    pub frames: Vec<Frame>,
    /// Size of the file's image, before any downscale to fit the GPU.
    pub width: u32,
    pub height: u32,
    pub has_alpha: bool,
    pub format: &'static str,
    pub file_size: u64,
    pub modified: Option<SystemTime>,
}

impl Decoded {
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    #[cfg(test)]
    /// Texture size actually shown (smaller than width/height only for huge images).
    pub fn texture_size(&self) -> [usize; 2] {
        self.frames[0].image.size
    }
}

pub fn decode(path: &Path, max_side: usize) -> Result<Decoded, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let format = image::guess_format(&bytes)
        .ok()
        .or_else(|| ImageFormat::from_path(path).ok())
        .ok_or("Not a recognised image format")?;

    let raw = match format {
        ImageFormat::Gif => {
            let decoder = GifDecoder::new(Cursor::new(&bytes)).map_err(err)?;
            animation(decoder.into_frames())?
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::new(Cursor::new(&bytes)).map_err(err)?;
            if decoder.is_apng().map_err(err)? {
                animation(decoder.apng().map_err(err)?.into_frames())?
            } else {
                vec![(still(decoder)?, Duration::ZERO)]
            }
        }
        ImageFormat::WebP => {
            let decoder = WebPDecoder::new(Cursor::new(&bytes)).map_err(err)?;
            if decoder.has_animation() {
                animation(decoder.into_frames())?
            } else {
                vec![(still(decoder)?, Duration::ZERO)]
            }
        }
        _ => {
            let decoder = ImageReader::with_format(Cursor::new(&bytes), format)
                .into_decoder()
                .map_err(err)?;
            vec![(still(decoder)?, Duration::ZERO)]
        }
    };

    let (width, height) = raw[0].0.dimensions();
    let has_alpha = raw.iter().any(|(img, _)| img.pixels().any(|p| p[3] < 255));
    let frames = raw
        .into_iter()
        .map(|(img, delay)| {
            let img = fit_texture(img, max_side);
            let size = [img.width() as usize, img.height() as usize];
            Frame { image: Arc::new(ColorImage::from_rgba_unmultiplied(size, &img)), delay }
        })
        .collect();

    Ok(Decoded {
        frames,
        width,
        height,
        has_alpha,
        format: format_name(format),
        file_size: meta.len(),
        modified: meta.modified().ok(),
    })
}

fn err(e: image::ImageError) -> String {
    e.to_string()
}

/// The first frame at full resolution, upright (for the clipboard).
pub fn full_rgba(path: &Path) -> Result<RgbaImage, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let decoder = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(err)?;
    still(decoder)
}

/// A still image, turned the right way up (camera JPEGs store rotation in EXIF).
fn still(mut decoder: impl ImageDecoder) -> Result<RgbaImage, String> {
    let orientation = decoder.orientation().ok();
    let mut img = DynamicImage::from_decoder(decoder).map_err(err)?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    Ok(img.into_rgba8())
}

fn animation(frames: image::Frames<'_>) -> Result<Vec<(RgbaImage, Duration)>, String> {
    let mut out = Vec::new();
    let mut total = 0usize;
    for frame in frames {
        let frame = frame.map_err(err)?;
        let (num, den) = frame.delay().numer_denom_ms();
        let delay = frame_delay(num as f64 / den.max(1) as f64);
        let buffer = frame.into_buffer();
        total += buffer.len();
        if total > MAX_ANIMATION_BYTES {
            return Err("Animation is too large to play".into());
        }
        out.push((buffer, delay));
    }
    if out.is_empty() {
        return Err("Animation has no frames".into());
    }
    Ok(out)
}

/// Browsers show 0-10 ms GIF delays at 100 ms; files authored for them expect it.
pub fn frame_delay(ms: f64) -> Duration {
    if ms <= 10.0 { Duration::from_millis(100) } else { Duration::from_secs_f64(ms / 1000.0) }
}

fn fit_texture(img: RgbaImage, max_side: usize) -> RgbaImage {
    let (w, h) = img.dimensions();
    let longest = w.max(h) as usize;
    if longest <= max_side {
        return img;
    }
    let scale = max_side as f64 / longest as f64;
    let nw = ((w as f64 * scale).round() as u32).clamp(1, max_side as u32);
    let nh = ((h as f64 * scale).round() as u32).clamp(1, max_side as u32);
    image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle)
}

fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "PNG",
        ImageFormat::Jpeg => "JPEG",
        ImageFormat::Gif => "GIF",
        ImageFormat::WebP => "WebP",
        ImageFormat::Bmp => "BMP",
        ImageFormat::Ico => "ICO",
        ImageFormat::Tiff => "TIFF",
        ImageFormat::Tga => "TGA",
        ImageFormat::Qoi => "QOI",
        _ => "Image",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::{GifEncoder, Repeat};
    use image::{Delay, Rgba};

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("imageviewer-{}-{name}", std::process::id()))
    }

    #[test]
    fn png_keeps_exact_pixels_and_alpha() {
        let path = temp("still.png");
        let mut img = RgbaImage::from_pixel(3, 2, Rgba([10, 200, 30, 255]));
        img.put_pixel(2, 1, Rgba([0, 0, 0, 0]));
        img.save(&path).unwrap();
        let d = decode(&path, 16384).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!((d.width, d.height, d.format), (3, 2, "PNG"));
        assert!(d.has_alpha && !d.is_animated());
        let px = d.frames[0].image.pixels[0];
        assert_eq!((px.r(), px.g(), px.b(), px.a()), (10, 200, 30, 255));
    }

    #[test]
    fn gif_frames_and_delays() {
        let path = temp("anim.gif");
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut enc = GifEncoder::new(file);
            enc.set_repeat(Repeat::Infinite).unwrap();
            for (i, ms) in [(0u8, 50u32), (1, 0), (2, 200)] {
                let img = RgbaImage::from_pixel(4, 4, Rgba([i * 80, 0, 0, 255]));
                let frame = image::Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(ms, 1));
                enc.encode_frame(frame).unwrap();
            }
        }
        let d = decode(&path, 16384).unwrap();
        std::fs::remove_file(&path).unwrap();
        let delays: Vec<u128> = d.frames.iter().map(|f| f.delay.as_millis()).collect();
        assert_eq!(delays, [50, 100, 200]);
        assert!(d.is_animated() && !d.has_alpha);
    }

    #[test]
    fn oversized_images_shrink_to_texture_limit() {
        let path = temp("big.png");
        RgbaImage::from_pixel(300, 100, Rgba([1, 2, 3, 255])).save(&path).unwrap();
        let d = decode(&path, 150).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!((d.width, d.height), (300, 100));
        assert_eq!(d.texture_size(), [150, 50]);
    }

    #[test]
    fn misnamed_files_decode_by_content() {
        let path = temp("really-a-png.jpg");
        RgbaImage::from_pixel(2, 2, Rgba([9, 9, 9, 255])).save_with_format(&path, ImageFormat::Png).unwrap();
        let d = decode(&path, 16384).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(d.format, "PNG");
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        let path = temp("junk.png");
        std::fs::write(&path, b"definitely not an image").unwrap();
        let result = decode(&path, 16384);
        std::fs::remove_file(&path).unwrap();
        assert!(result.is_err());
    }
}
