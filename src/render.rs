use crate::color::Color;
use crate::config::FormatOptions;
use std::path::Path;
use wayland_client::protocol::wl_shm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn from_points(x0: u32, y0: u32, x1: u32, y1: u32) -> Self {
        let min_x = x0.min(x1);
        let max_x = x0.max(x1);
        let min_y = y0.min(y1);
        let max_y = y0.max(y1);

        Self {
            x: min_x,
            y: min_y,
            width: max_x - min_x + 1,
            height: max_y - min_y + 1,
        }
    }

    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        let inter_x0 = self.x.max(other.x);
        let inter_y0 = self.y.max(other.y);
        let inter_x1 = (self.x + self.width).min(other.x + other.width);
        let inter_y1 = (self.y + self.height).min(other.y + other.height);
        inter_x1 > inter_x0 && inter_y1 > inter_y0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormatType {
    Png,
    Jpeg,
    Webp,
    Bmp,
}

impl ImageFormatType {
    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        match ext.as_str() {
            "webp" => Self::Webp,
            "jpg" | "jpeg" => Self::Jpeg,
            "bmp" => Self::Bmp,
            _ => Self::Png,
        }
    }
}

pub fn precompute_dimmed(original: &[u8], dim_color: Color, format: wl_shm::Format) -> Vec<u8> {
    let mut dimmed = original.to_vec();

    for chunk in dimmed.as_chunks_mut::<4>().0 {
        match format {
            wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {
                let (r, g, b) = dim_color.blend_over(chunk[2], chunk[1], chunk[0]);
                chunk[0] = b;
                chunk[1] = g;
                chunk[2] = r;
                chunk[3] = 255;
            }
            _ => {
                let (r, g, b) = dim_color.blend_over(chunk[0], chunk[1], chunk[2]);
                chunk[0] = r;
                chunk[1] = g;
                chunk[2] = b;
                chunk[3] = 255;
            }
        }
    }

    dimmed
}

#[inline(always)]
fn put_border_pixel(
    dest: &mut [u8],
    offset: usize,
    format: wl_shm::Format,
    border_color: Color,
    border_bytes: [u8; 4],
) {
    if border_color.a == 255 {
        dest[offset..offset + 4].copy_from_slice(&border_bytes);
    } else {
        match format {
            wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {
                let (r, g, b) = border_color.blend_over(dest[offset + 2], dest[offset + 1], dest[offset]);
                dest[offset..offset + 4].copy_from_slice(&[b, g, r, 255]);
            }
            _ => {
                let (r, g, b) = border_color.blend_over(dest[offset], dest[offset + 1], dest[offset + 2]);
                dest[offset..offset + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_overlay(
    dest: &mut [u8],
    original: &[u8],
    dimmed: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
    selection: Option<Rect>,
    brighten_interior: bool,
    border_color: Color,
    border_width: u32,
) {
    dest.copy_from_slice(dimmed);

    let Some(rect) = selection else {
        return;
    };

    if rect.width <= 2 || rect.height <= 2 {
        return;
    }

    let x_end = (rect.x + rect.width).min(width);
    let y_end = (rect.y + rect.height).min(height);
    let x_start = rect.x.min(width);
    let y_start = rect.y.min(height);

    if x_end <= x_start || y_end <= y_start {
        return;
    }

    let bw = border_width
        .min((x_end - x_start) / 2)
        .min((y_end - y_start) / 2)
        .max(1);

    let border_bytes = match format {
        wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {
            [border_color.b, border_color.g, border_color.r, 255]
        }
        _ => {
            [border_color.r, border_color.g, border_color.b, 255]
        }
    };

    let stride = stride as usize;

    for y in y_start..y_end {
        let row_offset = (y as usize) * stride;

        if y < y_start + bw || y >= y_end - bw {
            for x in x_start..x_end {
                let px_offset = row_offset + (x as usize) * 4;
                put_border_pixel(dest, px_offset, format, border_color, border_bytes);
            }
        } else {
            for x in x_start..x_start + bw {
                let px_offset = row_offset + (x as usize) * 4;
                put_border_pixel(dest, px_offset, format, border_color, border_bytes);
            }

            if brighten_interior {
                let inner_start_x = x_start + bw;
                let inner_end_x = x_end - bw;
                if inner_end_x > inner_start_x {
                    let start_byte = row_offset + (inner_start_x as usize) * 4;
                    let end_byte = row_offset + (inner_end_x as usize) * 4;
                    dest[start_byte..end_byte].copy_from_slice(&original[start_byte..end_byte]);
                }
            }

            for x in x_end - bw..x_end {
                let px_offset = row_offset + (x as usize) * 4;
                put_border_pixel(dest, px_offset, format, border_color, border_bytes);
            }
        }
    }
}

pub fn patch_overlapping_windows(
    base_img: &mut image::RgbaImage,
    target_rect: Rect,
    overlap_rects: &[Rect],
    toplevel_img: &image::RgbaImage,
) {
    for overlap in overlap_rects {
        let inter_x0 = target_rect.x.max(overlap.x);
        let inter_y0 = target_rect.y.max(overlap.y);
        let inter_x1 = (target_rect.x + target_rect.width).min(overlap.x + overlap.width);
        let inter_y1 = (target_rect.y + target_rect.height).min(overlap.y + overlap.height);

        if inter_x1 <= inter_x0 || inter_y1 <= inter_y0 {
            continue;
        }

        for gy in inter_y0..inter_y1 {
            let local_y = gy - target_rect.y;
            for gx in inter_x0..inter_x1 {
                let local_x = gx - target_rect.x;
                if local_x < base_img.width()
                    && local_y < base_img.height()
                    && local_x < toplevel_img.width()
                    && local_y < toplevel_img.height()
                {
                    let top_px = toplevel_img.get_pixel(local_x, local_y);
                    if top_px[3] == 255 {
                        base_img.put_pixel(local_x, local_y, *top_px);
                    } else if top_px[3] > 0 {
                        let base_px = base_img.get_pixel(local_x, local_y);
                        let a = top_px[3] as u32;
                        let inv_a = 255 - a;
                        let r = ((top_px[0] as u32 * a + base_px[0] as u32 * inv_a) / 255) as u8;
                        let g = ((top_px[1] as u32 * a + base_px[1] as u32 * inv_a) / 255) as u8;
                        let b = ((top_px[2] as u32 * a + base_px[2] as u32 * inv_a) / 255) as u8;
                        base_img.put_pixel(local_x, local_y, image::Rgba([r, g, b, 255]));
                    }
                }
            }
        }
    }
}

pub fn encode_webp(
    img: &image::RgbaImage,
    webp_cfg: &crate::config::WebpConfig,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    unsafe {
        let mut config = std::mem::MaybeUninit::<libwebp_sys::WebPConfig>::uninit();
        if libwebp_sys::WebPConfigInitInternal(
            config.as_mut_ptr(),
            libwebp_sys::WebPPreset::WEBP_PRESET_DEFAULT,
            75.0,
            libwebp_sys::WEBP_ENCODER_ABI_VERSION as _,
        ) == 0
        {
            return Err("Failed to initialize WebPConfig".into());
        }
        let mut config = config.assume_init();
        config.lossless = if webp_cfg.lossless { 1 } else { 0 };

        let quality = if webp_cfg.lossless && (webp_cfg.quality - 90.0).abs() < f32::EPSILON {
            25.0
        } else {
            webp_cfg.quality
        };

        config.quality = quality.clamp(0.0, 100.0);
        config.method = webp_cfg.method.clamp(0, 6);
        config.exact = if webp_cfg.exact { 1 } else { 0 };
        config.thread_level = 1;

        if libwebp_sys::WebPValidateConfig(&config) == 0 {
            return Err("Invalid WebPConfig configuration".into());
        }

        let mut picture = std::mem::MaybeUninit::<libwebp_sys::WebPPicture>::uninit();
        if libwebp_sys::WebPPictureInitInternal(
            picture.as_mut_ptr(),
            libwebp_sys::WEBP_ENCODER_ABI_VERSION as _,
        ) == 0
        {
            return Err("Failed to initialize WebPPicture".into());
        }
        let mut picture = picture.assume_init();
        picture.use_argb = 1;
        picture.width = img.width() as i32;
        picture.height = img.height() as i32;

        if libwebp_sys::WebPPictureImportRGBA(
            &mut picture,
            img.as_raw().as_ptr(),
            img.width() as i32 * 4,
        ) == 0
        {
            libwebp_sys::WebPPictureFree(&mut picture);
            return Err("Failed to import RGBA pixels into WebPPicture".into());
        }

        let mut mem_writer = std::mem::MaybeUninit::<libwebp_sys::WebPMemoryWriter>::uninit();
        libwebp_sys::WebPMemoryWriterInit(mem_writer.as_mut_ptr());
        picture.writer = Some(libwebp_sys::WebPMemoryWrite);
        picture.custom_ptr = mem_writer.as_mut_ptr() as *mut std::ffi::c_void;

        let status = libwebp_sys::WebPEncode(&config, &mut picture);
        let mem_writer = mem_writer.assume_init();
        libwebp_sys::WebPPictureFree(&mut picture);

        if status != 0 {
            let slice = std::slice::from_raw_parts(mem_writer.mem, mem_writer.size);
            let result = slice.to_vec();
            libwebp_sys::WebPFree(mem_writer.mem as *mut std::ffi::c_void);
            Ok(result)
        } else {
            libwebp_sys::WebPFree(mem_writer.mem as *mut std::ffi::c_void);
            Err("WebP encoding failed".into())
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn save_screenshot(
    original: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
    y_invert: bool,
    selection: Option<Rect>,
    toplevel_patch: Option<(&image::RgbaImage, &[Rect])>,
    format_options: &FormatOptions,
    path: &Path,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let rect = selection.unwrap_or(Rect {
        x: 0,
        y: 0,
        width,
        height,
    });

    let x_start = rect.x.min(width);
    let y_start = rect.y.min(height);
    let actual_width = rect.width.min(width - x_start);
    let actual_height = rect.height.min(height - y_start);

    if actual_width == 0 || actual_height == 0 {
        return Err("Screenshot size is 0".into());
    }

    let mut img = image::RgbaImage::new(actual_width, actual_height);
    let stride = stride as usize;

    for ly in 0..actual_height {
        let gy = if y_invert {
            height - 1 - (y_start + ly)
        } else {
            y_start + ly
        };

        let row_offset = (gy as usize) * stride;

        for lx in 0..actual_width {
            let gx = x_start + lx;
            let px_offset = row_offset + (gx as usize) * 4;

            let (r, g, b) = match format {
                wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888 => {
                    let b = original[px_offset];
                    let g = original[px_offset + 1];
                    let r = original[px_offset + 2];
                    (r, g, b)
                }
                _ => {
                    let r = original[px_offset];
                    let g = original[px_offset + 1];
                    let b = original[px_offset + 2];
                    (r, g, b)
                }
            };

            img.put_pixel(lx, ly, image::Rgba([r, g, b, 255]));
        }
    }

    if let Some((toplevel_img, overlap_rects)) = toplevel_patch
        && !overlap_rects.is_empty() {
            patch_overlapping_windows(&mut img, rect, overlap_rects, toplevel_img);
        }

    match ImageFormatType::from_path(path) {
        ImageFormatType::Webp => {
            let memory = encode_webp(&img, &format_options.webp)?;
            std::fs::write(path, memory)?;
        }
        ImageFormatType::Jpeg => {
            let rgb_img = image::DynamicImage::ImageRgba8(img.clone()).into_rgb8();
            let file = std::fs::File::create(path)?;
            let mut writer = std::io::BufWriter::new(file);
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                &mut writer,
                format_options.jpeg.quality.clamp(1, 100),
            );
            encoder.encode_image(&rgb_img)?;
        }
        ImageFormatType::Png => {
            let file = std::fs::File::create(path)?;
            let mut writer = std::io::BufWriter::new(file);
            let compression = match format_options.png.compression.to_ascii_lowercase().as_str() {
                "fast" => image::codecs::png::CompressionType::Fast,
                "best" => image::codecs::png::CompressionType::Best,
                _ => image::codecs::png::CompressionType::Default,
            };
            let encoder = image::codecs::png::PngEncoder::new_with_quality(
                &mut writer,
                compression,
                image::codecs::png::FilterType::Adaptive,
            );
            image::ImageEncoder::write_image(
                encoder,
                img.as_raw(),
                img.width(),
                img.height(),
                image::ExtendedColorType::Rgba8,
            )?;
        }
        ImageFormatType::Bmp => {
            img.save(path)?;
        }
    }

    let mut png_bytes = Vec::new();
    {
        let mut writer = std::io::BufWriter::new(std::io::Cursor::new(&mut png_bytes));
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            &mut writer,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        );
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )?;
    }

    Ok(png_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rect_from_points() {
        let r1 = Rect::from_points(10, 20, 30, 40);
        assert_eq!(r1.x, 10);
        assert_eq!(r1.y, 20);
        assert_eq!(r1.width, 21);
        assert_eq!(r1.height, 21);

        let r2 = Rect::from_points(30, 40, 10, 20);
        assert_eq!(r2.x, 10);
        assert_eq!(r2.y, 20);
        assert_eq!(r2.width, 21);
        assert_eq!(r2.height, 21);
    }

    #[test]
    fn test_precompute_dimmed() {
        let original = vec![50, 100, 200, 255];
        let dim_color = Color::rgba(0, 0, 0, 128);
        let dimmed = precompute_dimmed(&original, dim_color, wl_shm::Format::Xrgb8888);
        assert!((dimmed[0] as i32 - 25).abs() <= 1);
        assert!((dimmed[1] as i32 - 50).abs() <= 1);
        assert!((dimmed[2] as i32 - 100).abs() <= 1);
        assert_eq!(dimmed[3], 255);
    }

    #[test]
    fn test_render_overlay_dim_preserved() {
        let width = 20;
        let height = 20;
        let stride = width * 4;
        let original = vec![200u8; (height * stride) as usize];
        let dimmed = vec![100u8; (height * stride) as usize];
        let mut dest = vec![0u8; (height * stride) as usize];

        let rect = Some(Rect {
            x: 0,
            y: 0,
            width,
            height,
        });

        render_overlay(
            &mut dest,
            &original,
            &dimmed,
            width,
            height,
            stride,
            wl_shm::Format::Xrgb8888,
            rect,
            false,
            Color::rgb(255, 255, 255),
            2,
        );

        let center_offset = (10 * stride + 10 * 4) as usize;
        assert_eq!(dest[center_offset], 100);
        assert_eq!(dest[0], 255);

        render_overlay(
            &mut dest,
            &original,
            &dimmed,
            width,
            height,
            stride,
            wl_shm::Format::Xrgb8888,
            rect,
            true,
            Color::rgb(255, 255, 255),
            2,
        );

        assert_eq!(dest[center_offset], 200);
    }

    #[test]
    fn test_save_screenshot() {
        let width = 20;
        let height = 20;
        let stride = width * 4;
        let mut original = vec![0u8; (height * stride) as usize];

        for chunk in original.chunks_exact_mut(4) {
            chunk[0] = 255;
            chunk[1] = 0;
            chunk[2] = 0;
            chunk[3] = 255;
        }

        let temp_dir = std::env::temp_dir();
        let out_path = temp_dir.join("ssnip_test_screenshot.png");

        let selection = Some(Rect {
            x: 5,
            y: 5,
            width: 10,
            height: 10,
        });

        let res = save_screenshot(
            &original,
            width,
            height,
            stride,
            wl_shm::Format::Xrgb8888,
            false,
            selection,
            None,
            &FormatOptions::default(),
            &out_path,
        );

        assert!(res.is_ok());
        assert!(out_path.exists());
        let metadata = std::fs::metadata(&out_path).unwrap();
        assert!(metadata.len() > 0);
        let _ = std::fs::remove_file(out_path);
    }

    #[test]
    fn test_libwebp_sys_encode() {
        unsafe {
            let mut config = std::mem::MaybeUninit::<libwebp_sys::WebPConfig>::uninit();
            let ok = libwebp_sys::WebPConfigInitInternal(
                config.as_mut_ptr(),
                libwebp_sys::WebPPreset::WEBP_PRESET_DEFAULT,
                90.0,
                libwebp_sys::WEBP_ENCODER_ABI_VERSION as _,
            );
            assert_eq!(ok, 1);
        }
    }

    #[test]
    fn test_save_screenshot_jpeg() {
        let width = 20;
        let height = 20;
        let stride = width * 4;
        let mut original = vec![0u8; (height * stride) as usize];

        for chunk in original.chunks_exact_mut(4) {
            chunk[0] = 255;
            chunk[1] = 128;
            chunk[2] = 64;
            chunk[3] = 255;
        }

        let temp_dir = std::env::temp_dir();
        let out_path = temp_dir.join("ssnip_test_screenshot.jpg");

        let res = save_screenshot(
            &original,
            width,
            height,
            stride,
            wl_shm::Format::Xrgb8888,
            false,
            None,
            None,
            &FormatOptions::default(),
            &out_path,
        );

        assert!(res.is_ok());
        assert!(out_path.exists());
        let metadata = std::fs::metadata(&out_path).unwrap();
        assert!(metadata.len() > 0);
        let _ = std::fs::remove_file(out_path);
    }

    #[test]
    fn test_save_screenshot_webp() {
        let width = 20;
        let height = 20;
        let stride = width * 4;
        let mut original = vec![0u8; (height * stride) as usize];

        for chunk in original.chunks_exact_mut(4) {
            chunk[0] = 255;
            chunk[1] = 128;
            chunk[2] = 64;
            chunk[3] = 255;
        }

        let temp_dir = std::env::temp_dir();
        let out_path = temp_dir.join("ssnip_test_screenshot.webp");

        let res = save_screenshot(
            &original,
            width,
            height,
            stride,
            wl_shm::Format::Xrgb8888,
            false,
            None,
            None,
            &FormatOptions::default(),
            &out_path,
        );

        assert!(res.is_ok());
        assert!(out_path.exists());
        let metadata = std::fs::metadata(&out_path).unwrap();
        assert!(metadata.len() > 0);
        let _ = std::fs::remove_file(out_path);
    }

    #[test]
    fn test_patch_overlapping_windows() {
        let mut base = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 255, 0, 255]));
        let toplevel = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 255, 255]));

        let target_rect = Rect { x: 0, y: 0, width: 100, height: 100 };
        let overlap = Rect { x: 40, y: 40, width: 20, height: 20 };

        patch_overlapping_windows(&mut base, target_rect, &[overlap], &toplevel);

        assert_eq!(base.get_pixel(10, 10), &image::Rgba([0, 255, 0, 255]));
        assert_eq!(base.get_pixel(50, 50), &image::Rgba([0, 0, 255, 255]));
    }
}
