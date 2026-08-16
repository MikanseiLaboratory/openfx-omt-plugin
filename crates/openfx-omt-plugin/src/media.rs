use std::time::Instant;

use crate::config::{MIN_VIDEO_DIM, TICKS_PER_SECOND};
use openfx::image::{PixelComponents, PixelDepth, RectI, pixel_byte_offset};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaError {
    TooSmall { width: u32, height: u32 },
    EmptyWindow,
}

impl std::fmt::Display for MediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooSmall { width, height } => {
                write!(
                    f,
                    "video {width}x{height} is smaller than {MIN_VIDEO_DIM}x{MIN_VIDEO_DIM}"
                )
            }
            Self::EmptyWindow => write!(f, "render window is empty"),
        }
    }
}

impl std::error::Error for MediaError {}

#[derive(Debug, Clone)]
pub struct ConvertedVideo {
    pub width: u32,
    pub height: u32,
    pub stride: i32,
    pub bgra: Vec<u8>,
    pub has_alpha: bool,
}

#[derive(Debug)]
pub struct SessionClock {
    start: Instant,
    last: i64,
}

impl SessionClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            last: -1,
        }
    }

    pub fn next_monotonic(&mut self) -> i64 {
        let nanos = self.start.elapsed().as_nanos();
        let candidate = (nanos / 100) as i64;
        let next = if candidate <= self.last {
            self.last.saturating_add(1)
        } else {
            candidate
        };
        self.last = next;
        next
    }
}

impl Default for SessionClock {
    fn default() -> Self {
        Self::new()
    }
}

pub fn video_interval_ticks(fps_n: i32, fps_d: i32) -> i64 {
    let n = fps_n.max(1) as i64;
    let d = fps_d.max(1) as i64;
    (TICKS_PER_SECOND * d) / n
}

#[cfg(test)]
fn sample_u8(depth: PixelDepth, bytes: &[u8]) -> u8 {
    match depth {
        PixelDepth::Byte => bytes[0],
        PixelDepth::Short => {
            let value = u16::from_le_bytes([bytes[0], bytes[1]]);
            (value >> 8) as u8
        }
        PixelDepth::Float => {
            let value = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            (value.clamp(0.0, 1.0) * 255.0).round() as u8
        }
    }
}

#[cfg(test)]
pub fn packed_row_to_bgra_pixel(
    depth: PixelDepth,
    components: PixelComponents,
    bytes: &[u8],
) -> [u8; 4] {
    let ch = depth.bytes_per_channel();
    let r = sample_u8(depth, &bytes[0..]);
    let g = sample_u8(depth, &bytes[ch..]);
    let b = sample_u8(depth, &bytes[ch * 2..]);
    let a = if components == PixelComponents::Rgba {
        sample_u8(depth, &bytes[ch * 3..])
    } else {
        255
    };
    [b, g, r, a]
}

/// Convert an OFX image window to tightly packed top-down BGRA8.
///
/// `data` is `kOfxImagePropData`. `row_bytes` may be negative.
///
/// # Safety
/// `data` must remain valid for `bounds` / `row_bytes` for the duration of the call.
pub unsafe fn convert_window_to_bgra(
    window: RectI,
    bounds: RectI,
    row_bytes: i32,
    data: *const u8,
    depth: PixelDepth,
    components: PixelComponents,
) -> Result<ConvertedVideo, MediaError> {
    let mut window = window;
    if window.width() % 2 != 0 {
        window.x2 -= 1;
    }
    if window.height() % 2 != 0 {
        window.y2 -= 1;
    }
    let width = window.width();
    let height = window.height();
    if width <= 0 || height <= 0 {
        return Err(MediaError::EmptyWindow);
    }
    let width = width as u32;
    let height = height as u32;
    if width < MIN_VIDEO_DIM || height < MIN_VIDEO_DIM {
        return Err(MediaError::TooSmall { width, height });
    }

    let bpp = depth.bytes_per_channel() * components.count();
    let stride = (width as usize).saturating_mul(4);
    let mut bgra = vec![0u8; stride.saturating_mul(height as usize)];
    let x1 = window.x1.max(bounds.x1);
    let x2 = window.x2.min(bounds.x2);
    if x2 <= x1 {
        return Err(MediaError::EmptyWindow);
    }
    let count = (x2 - x1) as usize;
    let dst_x0 = (x1 - window.x1) as usize;

    let mut has_alpha = false;
    for out_y in 0..height as i32 {
        let src_y = window.y2 - 1 - out_y;
        if src_y < bounds.y1 || src_y >= bounds.y2 {
            continue;
        }
        let Ok(offset) = pixel_byte_offset(bounds, row_bytes, bpp, x1, src_y) else {
            continue;
        };
        let src = unsafe { data.offset(offset) };
        let dst_row = out_y as usize * stride + dst_x0 * 4;
        has_alpha |= unsafe {
            write_bgra_row(
                depth,
                components,
                src,
                &mut bgra[dst_row..dst_row + count * 4],
                count,
            )
        };
    }

    Ok(ConvertedVideo {
        width,
        height,
        stride: stride as i32,
        bgra,
        has_alpha,
    })
}

unsafe fn write_bgra_row(
    depth: PixelDepth,
    components: PixelComponents,
    src: *const u8,
    dst: &mut [u8],
    count: usize,
) -> bool {
    let ch = depth.bytes_per_channel();
    let src_bpp = ch * components.count();
    let mut has_alpha = false;
    for i in 0..count {
        let px = unsafe { src.add(i * src_bpp) };
        let r = unsafe { sample_u8_ptr(depth, px) };
        let g = unsafe { sample_u8_ptr(depth, px.add(ch)) };
        let b = unsafe { sample_u8_ptr(depth, px.add(ch * 2)) };
        let a = if components == PixelComponents::Rgba {
            unsafe { sample_u8_ptr(depth, px.add(ch * 3)) }
        } else {
            255
        };
        if a != 255 {
            has_alpha = true;
        }
        let o = i * 4;
        dst[o] = b;
        dst[o + 1] = g;
        dst[o + 2] = r;
        dst[o + 3] = a;
    }
    has_alpha
}

unsafe fn sample_u8_ptr(depth: PixelDepth, ptr: *const u8) -> u8 {
    match depth {
        PixelDepth::Byte => unsafe { *ptr },
        PixelDepth::Short => {
            let bytes = unsafe { [*ptr, *ptr.add(1)] };
            (u16::from_le_bytes(bytes) >> 8) as u8
        }
        PixelDepth::Float => {
            let bytes = unsafe { [*ptr, *ptr.add(1), *ptr.add(2), *ptr.add(3)] };
            (f32::from_le_bytes(bytes).clamp(0.0, 1.0) * 255.0).round() as u8
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_monotonic() {
        let mut clock = SessionClock::new();
        let a = clock.next_monotonic();
        let b = clock.next_monotonic();
        assert!(b > a);
    }

    #[test]
    fn video_interval_matches_100ns_ticks() {
        assert_eq!(video_interval_ticks(60, 1), TICKS_PER_SECOND / 60);
        assert_eq!(
            video_interval_ticks(30_000, 1_001),
            (TICKS_PER_SECOND * 1_001) / 30_000
        );
    }

    #[test]
    fn rejects_small_frames() {
        let window = RectI {
            x1: 0,
            y1: 0,
            x2: 8,
            y2: 8,
        };
        let err = unsafe {
            convert_window_to_bgra(
                window,
                window,
                32,
                [0u8; 8 * 8 * 4].as_ptr(),
                PixelDepth::Byte,
                PixelComponents::Rgba,
            )
        }
        .unwrap_err();
        assert!(matches!(err, MediaError::TooSmall { .. }));
    }

    #[test]
    fn even_aligns_odd_windows() {
        let window = RectI {
            x1: 0,
            y1: 0,
            x2: 17,
            y2: 17,
        };
        let converted = unsafe {
            convert_window_to_bgra(
                window,
                window,
                17 * 4,
                [0u8; 17 * 17 * 4].as_ptr(),
                PixelDepth::Byte,
                PixelComponents::Rgba,
            )
        }
        .expect("odd window should crop to even");
        assert_eq!(converted.width, 16);
        assert_eq!(converted.height, 16);
    }
}
