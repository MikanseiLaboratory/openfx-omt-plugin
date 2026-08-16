use openfx::image::{ClipImage, RectI};
use openfx::status::{OfxResult, kOfxStat};

use crate::media::{ConvertedVideo, MediaError, convert_window_to_bgra};

pub fn copy_image_window(src: &ClipImage<'_>, dst: &ClipImage<'_>, window: RectI) -> OfxResult<()> {
    if src.depth != dst.depth || src.components != dst.components {
        return Err(kOfxStat::ErrUnsupported);
    }
    let bpp = src.bytes_per_pixel();
    let x1 = window.x1.max(src.bounds.x1).max(dst.bounds.x1);
    let x2 = window.x2.min(src.bounds.x2).min(dst.bounds.x2);
    let y1 = window.y1.max(src.bounds.y1).max(dst.bounds.y1);
    let y2 = window.y2.min(src.bounds.y2).min(dst.bounds.y2);
    if x2 <= x1 || y2 <= y1 {
        return Ok(());
    }
    let width_bytes = (x2 - x1) as usize * bpp;
    for y in y1..y2 {
        unsafe {
            let src_ptr = src.pixel_ptr(x1, y)?;
            let dst_ptr = dst.pixel_ptr(x1, y)?;
            std::ptr::copy_nonoverlapping(src_ptr, dst_ptr, width_bytes);
        }
    }
    Ok(())
}

pub fn image_to_bgra(image: &ClipImage<'_>, window: RectI) -> Result<ConvertedVideo, MediaError> {
    convert_window_to_bgra(window, image.depth, image.components, |x, y| {
        if x < image.bounds.x1
            || x >= image.bounds.x2
            || y < image.bounds.y1
            || y >= image.bounds.y2
        {
            return None;
        }
        let bpp = image.bytes_per_pixel();
        unsafe {
            let ptr = image.pixel_ptr(x, y).ok()?;
            Some(std::slice::from_raw_parts(ptr, bpp).to_vec())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::packed_row_to_bgra_pixel;
    use openfx::image::{PixelComponents, PixelDepth};

    fn convert_buffer(
        width: i32,
        height: i32,
        depth: PixelDepth,
        components: PixelComponents,
        row_bytes: i32,
        data: &[u8],
    ) -> ConvertedVideo {
        let window = RectI {
            x1: 0,
            y1: 0,
            x2: width,
            y2: height,
        };
        convert_window_to_bgra(window, depth, components, |x, y| {
            let bpp = depth.bytes_per_channel() * components.count();
            let offset = if row_bytes >= 0 {
                y * row_bytes + x * bpp as i32
            } else {
                (height - 1 - y) * row_bytes.abs() + x * bpp as i32
            };
            let start = offset as usize;
            Some(data[start..start + bpp].to_vec())
        })
        .unwrap()
    }

    #[test]
    fn rgba8_to_bgra() {
        let mut src = vec![0u8; 16 * 16 * 4];
        src[0..4].copy_from_slice(&[10, 20, 30, 40]);
        let converted = convert_buffer(
            16,
            16,
            PixelDepth::Byte,
            PixelComponents::Rgba,
            16 * 4,
            &src,
        );
        let last_row = ((16 - 1) * 16 * 4) as usize;
        assert_eq!(&converted.bgra[last_row..last_row + 4], &[30, 20, 10, 40]);
        assert!(converted.has_alpha);
    }

    #[test]
    fn rgb8_fills_opaque_alpha() {
        let src = [1u8, 2, 3].repeat(16 * 16);
        let converted =
            convert_buffer(16, 16, PixelDepth::Byte, PixelComponents::Rgb, 16 * 3, &src);
        assert_eq!(converted.bgra[3], 255);
        assert!(!converted.has_alpha);
    }

    #[test]
    fn short_and_float_convert() {
        let mut src16 = vec![0u8; 16 * 16 * 8];
        src16[0..8].copy_from_slice(&[0x00, 0x10, 0x00, 0x20, 0x00, 0x30, 0xff, 0xff]);
        let converted = convert_buffer(
            16,
            16,
            PixelDepth::Short,
            PixelComponents::Rgba,
            16 * 8,
            &src16,
        );
        let last_row = ((16 - 1) * 16 * 4) as usize;
        assert_eq!(
            &converted.bgra[last_row..last_row + 4],
            &[0x30, 0x20, 0x10, 0xff]
        );

        let mut srcf = vec![0u8; 16 * 16 * 16];
        srcf[0..16].copy_from_slice(
            &[
                1.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
                0.0f32.to_le_bytes(),
                1.0f32.to_le_bytes(),
            ]
            .concat(),
        );
        let converted = convert_buffer(
            16,
            16,
            PixelDepth::Float,
            PixelComponents::Rgba,
            16 * 16,
            &srcf,
        );
        let last_row = ((16 - 1) * 16 * 4) as usize;
        assert_eq!(&converted.bgra[last_row..last_row + 4], &[0, 0, 255, 255]);
    }

    #[test]
    fn negative_rowbytes_flip() {
        let bpp = 4;
        let width = 16i32;
        let height = 16i32;
        let stride = width * bpp;
        let mut src = vec![0u8; (stride * height) as usize];
        src[0..4].copy_from_slice(&[9, 8, 7, 255]);
        let converted = convert_buffer(
            width,
            height,
            PixelDepth::Byte,
            PixelComponents::Rgba,
            -stride,
            &src,
        );
        assert_eq!(&converted.bgra[0..4], &[7, 8, 9, 255]);
    }

    #[test]
    fn packed_pixel_helper() {
        assert_eq!(
            packed_row_to_bgra_pixel(PixelDepth::Byte, PixelComponents::Rgb, &[1, 2, 3]),
            [3, 2, 1, 255]
        );
    }
}
