use openfx::MultiThread;
use openfx::image::{ClipImage, RectI};
use openfx_pixels::{
    ConvertHost, ConvertSource, ConvertSpec, ConvertedVideo, MediaError, PixelPool,
    convert_window_into,
};

pub use openfx_pixels::copy_image_window;

fn convert_host_spec<'a>(
    spec: ConvertSpec,
    multithread: Option<&'a MultiThread>,
) -> (ConvertSpec, Option<ConvertHost<'a>>) {
    match multithread {
        Some(multithread) => (spec, Some(ConvertHost { multithread })),
        None => (
            ConvertSpec {
                parallel_rows: false,
                ..spec
            },
            None,
        ),
    }
}

pub fn image_to_bgra(
    image: &ClipImage<'_>,
    window: RectI,
    pool: Option<&PixelPool>,
    multithread: Option<&MultiThread>,
) -> Result<ConvertedVideo, MediaError> {
    let scratch = pool.map(PixelPool::take).unwrap_or_default();
    let (spec, host) = convert_host_spec(ConvertSpec::BGRA_VMX, multithread);
    unsafe {
        convert_window_into(
            scratch,
            ConvertSource {
                window,
                bounds: image.bounds,
                row_bytes: image.row_bytes,
                data: image.data,
                depth: image.depth,
                components: image.components,
            },
            spec,
            host,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfx::image::{PixelComponents, PixelDepth};
    use openfx_pixels::{PackedOrder, convert_window_to_bgra, packed_row_to_pixel};

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
        unsafe {
            convert_window_to_bgra(window, window, row_bytes, data.as_ptr(), depth, components)
        }
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
        assert_eq!(&converted.data[last_row..last_row + 4], &[30, 20, 10, 40]);
        assert!(converted.has_alpha);
        assert_eq!(converted.order, PackedOrder::Bgra);
    }

    #[test]
    fn rgb8_fills_opaque_alpha() {
        let src = [1u8, 2, 3].repeat(16 * 16);
        let converted =
            convert_buffer(16, 16, PixelDepth::Byte, PixelComponents::Rgb, 16 * 3, &src);
        assert_eq!(converted.data[3], 255);
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
            &converted.data[last_row..last_row + 4],
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
        assert_eq!(&converted.data[last_row..last_row + 4], &[0, 0, 255, 255]);
    }

    #[test]
    fn negative_rowbytes_flip() {
        let bpp = 4;
        let width = 16i32;
        let height = 16i32;
        let stride = width * bpp;
        let mut src = vec![0u8; (stride * height) as usize];
        src[0..4].copy_from_slice(&[9, 8, 7, 255]);
        let window = RectI {
            x1: 0,
            y1: 0,
            x2: width,
            y2: height,
        };
        let data = unsafe { src.as_ptr().add(((height - 1) * stride) as usize) };
        let converted = unsafe {
            convert_window_to_bgra(
                window,
                window,
                -stride,
                data,
                PixelDepth::Byte,
                PixelComponents::Rgba,
            )
        }
        .unwrap();
        assert_eq!(&converted.data[0..4], &[7, 8, 9, 255]);
    }

    #[test]
    fn packed_pixel_helper() {
        assert_eq!(
            packed_row_to_pixel(
                PackedOrder::Bgra,
                PixelDepth::Byte,
                PixelComponents::Rgb,
                &[1, 2, 3]
            ),
            [3, 2, 1, 255]
        );
    }
}
