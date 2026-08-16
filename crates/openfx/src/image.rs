use std::ffi::CStr;
use std::ptr;

use crate::bindings::{
    OfxImageClipHandle, OfxImageEffectSuiteV1, OfxPropertySetHandle, OfxRectI, OfxTime,
    kOfxImageEffectPropComponents, kOfxImageEffectPropPixelDepth, kOfxImagePropBounds,
    kOfxImagePropData, kOfxImagePropRowBytes,
};
use crate::status::{OfxResult, kOfxStat};
use crate::suites::{PropertySet, Suites};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectI {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

impl RectI {
    pub fn from_ofx(rect: OfxRectI) -> Self {
        Self {
            x1: rect.x1,
            y1: rect.y1,
            x2: rect.x2,
            y2: rect.y2,
        }
    }

    pub fn width(self) -> i32 {
        self.x2.saturating_sub(self.x1)
    }

    pub fn height(self) -> i32 {
        self.y2.saturating_sub(self.y1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelDepth {
    Byte,
    Short,
    Float,
}

impl PixelDepth {
    pub fn bytes_per_channel(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Short => 2,
            Self::Float => 4,
        }
    }

    pub fn from_cstr(value: &CStr) -> OfxResult<Self> {
        match value.to_bytes() {
            b"OfxBitDepthByte" => Ok(Self::Byte),
            b"OfxBitDepthShort" => Ok(Self::Short),
            b"OfxBitDepthFloat" => Ok(Self::Float),
            _ => Err(kOfxStat::ErrUnsupported),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelComponents {
    Rgb,
    Rgba,
}

impl PixelComponents {
    pub fn count(self) -> usize {
        match self {
            Self::Rgb => 3,
            Self::Rgba => 4,
        }
    }

    pub fn from_cstr(value: &CStr) -> OfxResult<Self> {
        match value.to_bytes() {
            b"OfxImageComponentRGB" => Ok(Self::Rgb),
            b"OfxImageComponentRGBA" => Ok(Self::Rgba),
            _ => Err(kOfxStat::ErrUnsupported),
        }
    }
}

/// RAII wrapper around `clipGetImage` / `clipReleaseImage`.
pub struct ClipImage<'a> {
    handle: OfxPropertySetHandle,
    suite: &'a OfxImageEffectSuiteV1,
    props: PropertySet<'a>,
    pub bounds: RectI,
    pub row_bytes: i32,
    pub data: *mut u8,
    pub depth: PixelDepth,
    pub components: PixelComponents,
}

impl<'a> ClipImage<'a> {
    pub unsafe fn fetch(
        suites: &'a Suites,
        clip: OfxImageClipHandle,
        time: OfxTime,
    ) -> OfxResult<Self> {
        let get = suites
            .image_effect
            .clipGetImage
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut handle: OfxPropertySetHandle = ptr::null_mut();
        unsafe { get(clip, time, ptr::null_mut(), &mut handle) }.ofx_ok()?;
        if handle.is_null() {
            return Err(kOfxStat::Failed);
        }
        let props = PropertySet::new(handle, suites.property)?;
        let bounds = RectI::from_ofx(props.get_rect_i(kOfxImagePropBounds)?);
        let row_bytes = props.get_int(kOfxImagePropRowBytes, 0)?;
        if row_bytes == 0 {
            return Err(kOfxStat::ErrValue);
        }
        let data = props.get_pointer(kOfxImagePropData, 0)? as *mut u8;
        if data.is_null() {
            return Err(kOfxStat::Failed);
        }
        let depth = PixelDepth::from_cstr(props.get_string(kOfxImageEffectPropPixelDepth, 0)?)?;
        let components =
            PixelComponents::from_cstr(props.get_string(kOfxImageEffectPropComponents, 0)?)?;
        Ok(Self {
            handle,
            suite: suites.image_effect,
            props,
            bounds,
            row_bytes,
            data,
            depth,
            components,
        })
    }

    pub fn bytes_per_pixel(&self) -> usize {
        self.depth.bytes_per_channel() * self.components.count()
    }

    /// Pointer to pixel `(x, y)` in image bounds coordinates.
    pub unsafe fn pixel_ptr(&self, x: i32, y: i32) -> OfxResult<*mut u8> {
        let offset = pixel_byte_offset(self.bounds, self.row_bytes, self.bytes_per_pixel(), x, y)?;
        Ok(unsafe { self.data.offset(offset) })
    }

    pub fn props(&self) -> &PropertySet<'a> {
        &self.props
    }
}

/// Byte offset from `kOfxImagePropData` to pixel `(x, y)`. Supports signed `rowBytes`.
pub fn pixel_byte_offset(
    bounds: RectI,
    row_bytes: i32,
    bytes_per_pixel: usize,
    x: i32,
    y: i32,
) -> OfxResult<isize> {
    if x < bounds.x1 || x >= bounds.x2 || y < bounds.y1 || y >= bounds.y2 {
        return Err(kOfxStat::ErrBadIndex);
    }
    let bpp = bytes_per_pixel as isize;
    let row = row_bytes as isize;
    Ok((y - bounds.y1) as isize * row + (x - bounds.x1) as isize * bpp)
}

impl Drop for ClipImage<'_> {
    fn drop(&mut self) {
        if let Some(release) = self.suite.clipReleaseImage {
            let _ = unsafe { release(self.handle) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_size() {
        let rect = RectI {
            x1: 10,
            y1: 20,
            x2: 42,
            y2: 28,
        };
        assert_eq!(rect.width(), 32);
        assert_eq!(rect.height(), 8);
    }

    #[test]
    fn depth_sizes() {
        assert_eq!(PixelDepth::Byte.bytes_per_channel(), 1);
        assert_eq!(PixelDepth::Short.bytes_per_channel(), 2);
        assert_eq!(PixelDepth::Float.bytes_per_channel(), 4);
        assert_eq!(PixelComponents::Rgb.count(), 3);
        assert_eq!(PixelComponents::Rgba.count(), 4);
    }

    #[test]
    fn signed_rowbytes_offset() {
        let bounds = RectI {
            x1: 2,
            y1: 4,
            x2: 6,
            y2: 8,
        };
        assert_eq!(pixel_byte_offset(bounds, 16, 4, 2, 4).unwrap(), 0);
        assert_eq!(pixel_byte_offset(bounds, 16, 4, 3, 5).unwrap(), 20);
        assert_eq!(pixel_byte_offset(bounds, -16, 4, 2, 5).unwrap(), -16);
        assert_eq!(
            pixel_byte_offset(bounds, 16, 4, 1, 4).unwrap_err(),
            kOfxStat::ErrBadIndex
        );
    }

    #[test]
    fn depth_from_cstr() {
        assert_eq!(
            PixelDepth::from_cstr(c"OfxBitDepthByte").unwrap(),
            PixelDepth::Byte
        );
        assert_eq!(
            PixelComponents::from_cstr(c"OfxImageComponentRGBA").unwrap(),
            PixelComponents::Rgba
        );
        assert_eq!(
            PixelDepth::from_cstr(c"OfxBitDepthNone").unwrap_err(),
            kOfxStat::ErrUnsupported
        );
    }
}
