use std::sync::OnceLock;

use openfx::MultiThread;
#[cfg(test)]
use openfx::image::PixelComponents;
use openfx::image::{ClipImage, RectI};
use openfx::status::OfxResult;
use openfx_pixels::{
    ConvertHost, ConvertSource, ConvertSpec, ConvertedVideo, MediaError, PixelPool, RowWriter,
    convert_window_into,
};

pub use openfx_pixels::copy_image_window;

const LOG_TAG: &str = "OMT";

#[inline(always)]
fn live_spec() -> ConvertSpec {
    ConvertSpec {
        track_alpha: false,
        parallel_rows: true,
        ..ConvertSpec::BGRA_VMX
    }
}

#[inline(always)]
#[cfg(test)]
fn source_has_alpha(components: PixelComponents) -> bool {
    matches!(components, PixelComponents::Rgba)
}

/// Host SMP oversubscription on 8-core/16-thread CPUs thrashes the float copy.
const MAX_OFX_THREADS: u32 = 8;

fn log_serial_once(reason: &str) {
    static ONCE: OnceLock<()> = OnceLock::new();
    let _ = ONCE.get_or_init(|| {
        eprintln!("{LOG_TAG}: {reason}; finishing fused pass serially so video still sends");
    });
}

/// Passthrough copy plus live BGRA convert. Copy and convert share one source scan
/// (AVX2 row copy + SIMD row writer). Parallel rows use the host `multiThread` pool
/// when the render thread is allowed to spawn; otherwise the same fused pass runs
/// serially so OFX output and OMT send still complete.
pub fn pass_bgra(
    source: &ClipImage<'_>,
    output: &ClipImage<'_>,
    window: RectI,
    pool: Option<&PixelPool>,
    multithread: &MultiThread,
    convert: bool,
) -> OfxResult<Option<ConvertedVideo>> {
    if !convert {
        copy_image_window(source, output, window)?;
        return Ok(None);
    }

    match pass_copy_and_convert(source, output, window, pool, multithread, live_spec()) {
        Ok(converted) => Ok(Some(converted)),
        Err(err) => {
            eprintln!("{LOG_TAG}: {err}; passthrough only this frame");
            copy_image_window(source, output, window)?;
            Ok(None)
        }
    }
}

fn pass_copy_and_convert(
    source: &ClipImage<'_>,
    output: &ClipImage<'_>,
    window: RectI,
    pool: Option<&PixelPool>,
    multithread: &MultiThread,
    spec: ConvertSpec,
) -> Result<ConvertedVideo, MediaError> {
    if source.depth != output.depth || source.components != output.components {
        copy_image_window(source, output, window).map_err(|_| MediaError::EmptyWindow)?;
        return convert_live(source, window, pool, multithread, spec);
    }

    let mut conv_window = window;
    if spec.even_align {
        if conv_window.width() % 2 != 0 {
            conv_window.x2 -= 1;
        }
        if conv_window.height() % 2 != 0 {
            conv_window.y2 -= 1;
        }
    }
    let width = conv_window.width();
    let height = conv_window.height();
    if width <= 0 || height <= 0 {
        return Err(MediaError::EmptyWindow);
    }
    let width = width as u32;
    let height = height as u32;
    if width < spec.min_dim || height < spec.min_dim {
        return Err(MediaError::TooSmall {
            width,
            height,
            min_dim: spec.min_dim,
        });
    }

    let copy_x1 = window.x1.max(source.bounds.x1).max(output.bounds.x1);
    let copy_x2 = window.x2.min(source.bounds.x2).min(output.bounds.x2);
    let copy_y1 = window.y1.max(source.bounds.y1).max(output.bounds.y1);
    let copy_y2 = window.y2.min(source.bounds.y2).min(output.bounds.y2);
    if copy_x2 <= copy_x1 || copy_y2 <= copy_y1 {
        return Err(MediaError::EmptyWindow);
    }

    let bpp = source.bytes_per_pixel();
    let copy_width_bytes = (copy_x2 - copy_x1) as usize * bpp;
    let conv_x1 = conv_window.x1.max(source.bounds.x1);
    let conv_x2 = conv_window.x2.min(source.bounds.x2);
    if conv_x2 <= conv_x1 {
        return Err(MediaError::EmptyWindow);
    }
    let conv_count = (conv_x2 - conv_x1) as usize;
    let conv_dst_x0 = (conv_x1 - conv_window.x1) as usize;
    let stride = (width as usize).saturating_mul(4);
    let needed = stride.saturating_mul(height as usize);

    let fully_covered = conv_dst_x0 == 0
        && conv_count == width as usize
        && conv_window.y1 >= source.bounds.y1
        && conv_window.y2 <= source.bounds.y2;
    let mut packed = pool.map(PixelPool::take).unwrap_or_default();
    packed.clear();
    if fully_covered {
        if packed.capacity() < needed {
            packed.reserve(needed);
        }
        unsafe { packed.set_len(needed) };
    } else {
        packed.resize(needed, 0);
    }

    let bpp_i = bpp as isize;
    let ctx = PassCtx {
        src_data: source.data as usize,
        dst_data: output.data as usize,
        src_row_bytes: source.row_bytes as isize,
        dst_row_bytes: output.row_bytes as isize,
        src_y1: source.bounds.y1,
        dst_y1: output.bounds.y1,
        packed: packed.as_mut_ptr() as usize,
        packed_stride: stride,
        copy_width_bytes,
        y1: copy_y1,
        y2: copy_y2,
        conv_y1: conv_window.y1.max(source.bounds.y1),
        conv_y2: conv_window.y2.min(source.bounds.y2),
        conv_window_y2: conv_window.y2,
        src_copy_x_off: (copy_x1 - source.bounds.x1) as isize * bpp_i,
        dst_copy_x_off: (copy_x1 - output.bounds.x1) as isize * bpp_i,
        src_conv_x_off: (conv_x1 - source.bounds.x1) as isize * bpp_i,
        conv_count,
        conv_dst_x0,
        writer: RowWriter::resolve(spec.order, source.depth, source.components),
    };

    pass_rows(multithread, ctx)?;

    Ok(ConvertedVideo {
        width,
        height,
        stride: stride as i32,
        data: packed,
        has_alpha: false,
        order: spec.order,
    })
}

fn convert_live(
    image: &ClipImage<'_>,
    window: RectI,
    pool: Option<&PixelPool>,
    multithread: &MultiThread,
    spec: ConvertSpec,
) -> Result<ConvertedVideo, MediaError> {
    let source = ConvertSource {
        window,
        bounds: image.bounds,
        row_bytes: image.row_bytes,
        data: image.data,
        depth: image.depth,
        components: image.components,
    };
    let mut spec = spec;
    if multithread.is_spawned_thread() {
        log_serial_once("render thread is already an OFX spawned thread");
        spec.parallel_rows = false;
    }
    let scratch = pool.map(PixelPool::take).unwrap_or_default();
    let video = unsafe {
        match convert_window_into(
            scratch,
            source,
            spec,
            spec.parallel_rows.then_some(ConvertHost { multithread }),
        ) {
            Ok(video) => video,
            Err(MediaError::ParallelFailed) => {
                log_serial_once("OfxMultiThreadSuite::multiThread failed");
                spec.parallel_rows = false;
                convert_window_into(Vec::new(), source, spec, None)?
            }
            Err(err) => return Err(err),
        }
    };
    Ok(ConvertedVideo {
        has_alpha: false,
        ..video
    })
}

#[derive(Clone, Copy)]
struct PassCtx {
    src_data: usize,
    dst_data: usize,
    src_row_bytes: isize,
    dst_row_bytes: isize,
    src_y1: i32,
    dst_y1: i32,
    packed: usize,
    packed_stride: usize,
    copy_width_bytes: usize,
    y1: i32,
    y2: i32,
    conv_y1: i32,
    conv_y2: i32,
    conv_window_y2: i32,
    src_copy_x_off: isize,
    dst_copy_x_off: isize,
    src_conv_x_off: isize,
    conv_count: usize,
    conv_dst_x0: usize,
    writer: RowWriter,
}

#[inline(always)]
unsafe fn pass_one_y(y: i32, ctx: &PassCtx) {
    let src_copy = unsafe {
        (ctx.src_data as *const u8)
            .offset((y - ctx.src_y1) as isize * ctx.src_row_bytes + ctx.src_copy_x_off)
    };
    #[cfg(target_arch = "x86_64")]
    if y + 1 < ctx.y2 {
        let next = unsafe { src_copy.offset(ctx.src_row_bytes) };
        unsafe {
            std::arch::x86_64::_mm_prefetch(next as *const i8, std::arch::x86_64::_MM_HINT_T0);
        }
    }
    let dst_copy = unsafe {
        (ctx.dst_data as *mut u8)
            .offset((y - ctx.dst_y1) as isize * ctx.dst_row_bytes + ctx.dst_copy_x_off)
    };
    unsafe { copy_row(src_copy, dst_copy, ctx.copy_width_bytes) };
    if y < ctx.conv_y1 || y >= ctx.conv_y2 {
        return;
    }
    let src_conv = if ctx.src_conv_x_off == ctx.src_copy_x_off {
        src_copy
    } else {
        unsafe {
            (ctx.src_data as *const u8)
                .offset((y - ctx.src_y1) as isize * ctx.src_row_bytes + ctx.src_conv_x_off)
        }
    };
    let out_y = ctx.conv_window_y2 - 1 - y;
    let dst_row = out_y as usize * ctx.packed_stride + ctx.conv_dst_x0 * 4;
    unsafe {
        ctx.writer.write_row(
            src_conv,
            std::slice::from_raw_parts_mut(
                (ctx.packed as *mut u8).add(dst_row),
                ctx.conv_count * 4,
            ),
            ctx.conv_count,
            false,
        );
    }
}

unsafe fn pass_rows_serial(ctx: PassCtx) {
    for y in ctx.y1..ctx.y2 {
        unsafe { pass_one_y(y, &ctx) };
    }
    sfence();
}

#[inline]
fn row_band(y1: i32, y2: i32, thread_index: u32, thread_max: u32) -> (i32, i32) {
    let rows = (y2 - y1).max(0) as u64;
    let max = u64::from(thread_max.max(1));
    let index = u64::from(thread_index);
    let start = y1 + ((rows * index) / max) as i32;
    let end = y1 + ((rows * (index + 1)) / max) as i32;
    (start, end)
}

fn ofx_cpus(multithread: &MultiThread) -> u32 {
    static CPUS: OnceLock<u32> = OnceLock::new();
    if let Some(n) = CPUS.get() {
        return *n;
    }
    let n = multithread
        .num_cpus()
        .ok()
        .unwrap_or(1)
        .max(1)
        .min(MAX_OFX_THREADS);
    *CPUS.get_or_init(|| n)
}

fn pass_rows(multithread: &MultiThread, ctx: PassCtx) -> Result<(), MediaError> {
    let rows = ctx.y2 - ctx.y1;
    if rows <= 1 {
        unsafe { pass_rows_serial(ctx) };
        return Ok(());
    }
    if multithread.is_spawned_thread() {
        log_serial_once("render thread is already an OFX spawned thread");
        unsafe { pass_rows_serial(ctx) };
        return Ok(());
    }
    if unsafe { pass_rows_ofx(multithread, ctx) }.is_err() {
        log_serial_once("OfxMultiThreadSuite::multiThread failed");
        unsafe { pass_rows_serial(ctx) };
    }
    Ok(())
}

struct PassWork {
    ctx: PassCtx,
}

unsafe extern "C" fn pass_rows_worker(
    thread_index: u32,
    thread_max: u32,
    custom_arg: *mut std::ffi::c_void,
) {
    let work = unsafe { &*(custom_arg as *const PassWork) };
    let (start, end) = row_band(work.ctx.y1, work.ctx.y2, thread_index, thread_max);
    for y in start..end {
        unsafe { pass_one_y(y, &work.ctx) };
    }
    sfence();
}

unsafe fn pass_rows_ofx(multithread: &MultiThread, ctx: PassCtx) -> Result<(), MediaError> {
    let rows = (ctx.y2 - ctx.y1).max(1) as u32;
    let n_threads = ofx_cpus(multithread).min(rows).max(1);
    let work = PassWork { ctx };
    multithread
        .parallel(
            n_threads,
            Some(pass_rows_worker),
            &work as *const PassWork as *mut std::ffi::c_void,
        )
        .map_err(|_| MediaError::ParallelFailed)
}

#[inline(always)]
fn has_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        static AVX2: OnceLock<bool> = OnceLock::new();
        *AVX2.get_or_init(|| is_x86_feature_detected!("avx2"))
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

#[inline(always)]
fn sfence() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::arch::x86_64::_mm_sfence();
    }
}

#[inline(always)]
unsafe fn copy_row(src: *const u8, dst: *mut u8, len: usize) {
    #[cfg(target_arch = "x86_64")]
    {
        if len >= 64 && has_avx2() {
            if (dst as usize).is_multiple_of(32) {
                unsafe { copy_row_avx2_stream(src, dst, len) };
            } else {
                unsafe { copy_row_avx2(src, dst, len) };
            }
            return;
        }
    }
    unsafe {
        std::ptr::copy_nonoverlapping(src, dst, len);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn copy_row_avx2(src: *const u8, dst: *mut u8, len: usize) {
    use std::arch::x86_64::*;

    unsafe {
        let mut i = 0;
        while i + 64 <= len {
            let v0 = _mm256_loadu_si256(src.add(i) as *const __m256i);
            let v1 = _mm256_loadu_si256(src.add(i + 32) as *const __m256i);
            _mm256_storeu_si256(dst.add(i) as *mut __m256i, v0);
            _mm256_storeu_si256(dst.add(i + 32) as *mut __m256i, v1);
            i += 64;
        }
        while i + 32 <= len {
            let v = _mm256_loadu_si256(src.add(i) as *const __m256i);
            _mm256_storeu_si256(dst.add(i) as *mut __m256i, v);
            i += 32;
        }
        if i < len {
            std::ptr::copy_nonoverlapping(src.add(i), dst.add(i), len - i);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn copy_row_avx2_stream(src: *const u8, dst: *mut u8, len: usize) {
    use std::arch::x86_64::*;

    unsafe {
        let mut i = 0;
        while i + 64 <= len {
            let v0 = _mm256_loadu_si256(src.add(i) as *const __m256i);
            let v1 = _mm256_loadu_si256(src.add(i + 32) as *const __m256i);
            _mm256_stream_si256(dst.add(i) as *mut __m256i, v0);
            _mm256_stream_si256(dst.add(i + 32) as *mut __m256i, v1);
            i += 64;
        }
        while i + 32 <= len {
            let v = _mm256_loadu_si256(src.add(i) as *const __m256i);
            _mm256_stream_si256(dst.add(i) as *mut __m256i, v);
            i += 32;
        }
        if i < len {
            std::ptr::copy_nonoverlapping(src.add(i), dst.add(i), len - i);
        }
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
    fn live_spec_keeps_pixel_bytes() {
        let mut src = vec![0u8; 16 * 16 * 4];
        src[0..4].copy_from_slice(&[10, 20, 30, 40]);
        let window = RectI {
            x1: 0,
            y1: 0,
            x2: 16,
            y2: 16,
        };
        let tracked = convert_buffer(
            16,
            16,
            PixelDepth::Byte,
            PixelComponents::Rgba,
            16 * 4,
            &src,
        );
        let live = unsafe {
            convert_window_into(
                Vec::new(),
                ConvertSource {
                    window,
                    bounds: window,
                    row_bytes: 16 * 4,
                    data: src.as_ptr(),
                    depth: PixelDepth::Byte,
                    components: PixelComponents::Rgba,
                },
                ConvertSpec {
                    parallel_rows: false,
                    ..live_spec()
                },
                None,
            )
        }
        .unwrap();
        assert_eq!(tracked.data, live.data);
        assert!(!live.has_alpha);
        assert!(source_has_alpha(PixelComponents::Rgba));
        assert!(!source_has_alpha(PixelComponents::Rgb));
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

    #[test]
    fn avx2_copy_row_matches_memcpy() {
        let src: Vec<u8> = (0..192).map(|i| i as u8).collect();
        let mut dst = vec![0u8; 192];
        unsafe { copy_row(src.as_ptr(), dst.as_mut_ptr(), src.len()) };
        assert_eq!(src, dst);
    }

    #[test]
    fn row_bands_cover_all_rows() {
        let y1 = 10;
        let y2 = 1080;
        let n = 8u32;
        let mut prev = y1;
        let mut covered = 0;
        for i in 0..n {
            let (start, end) = row_band(y1, y2, i, n);
            assert_eq!(start, prev);
            assert!(end >= start);
            covered += end - start;
            prev = end;
        }
        assert_eq!(prev, y2);
        assert_eq!(covered, y2 - y1);
        assert_eq!(row_band(0, 10, 0, 1), (0, 10));
        assert_eq!(row_band(0, 10, 0, 3), (0, 3));
        assert_eq!(row_band(0, 10, 2, 3), (6, 10));
    }
}
