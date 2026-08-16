use std::time::Instant;

use crate::config::{MIN_VIDEO_DIM, TICKS_PER_SECOND};
use openfx::image::{PixelComponents, PixelDepth, RectI};

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
/// `row` is called as `row(x, y) -> pixel bytes` where `(x, y)` are OFX coordinates.
pub fn convert_window_to_bgra(
    window: RectI,
    depth: PixelDepth,
    components: PixelComponents,
    mut pixel: impl FnMut(i32, i32) -> Option<Vec<u8>>,
) -> Result<ConvertedVideo, MediaError> {
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
    let mut has_alpha = false;

    for out_y in 0..height as i32 {
        let src_y = window.y2 - 1 - out_y;
        for out_x in 0..width as i32 {
            let src_x = window.x1 + out_x;
            let Some(bytes) = pixel(src_x, src_y) else {
                continue;
            };
            if bytes.len() < bpp {
                continue;
            }
            let [b, g, r, a] = packed_row_to_bgra_pixel(depth, components, &bytes);
            if a != 255 {
                has_alpha = true;
            }
            let dst = (out_y as usize * stride) + (out_x as usize * 4);
            bgra[dst] = b;
            bgra[dst + 1] = g;
            bgra[dst + 2] = r;
            bgra[dst + 3] = a;
        }
    }

    Ok(ConvertedVideo {
        width,
        height,
        stride: stride as i32,
        bgra,
        has_alpha,
    })
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
        let err =
            convert_window_to_bgra(window, PixelDepth::Byte, PixelComponents::Rgba, |_, _| {
                Some(vec![1, 2, 3, 255])
            })
            .unwrap_err();
        assert!(matches!(err, MediaError::TooSmall { .. }));
    }
}
