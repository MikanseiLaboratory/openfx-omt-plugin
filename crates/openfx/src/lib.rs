//! Thin OpenFX ABI helpers for image-effect plugins.
//!
//! Host-compatibility notes (DaVinci Resolve) follow ntsc-rs:
//! Filter + General contexts, Create/Destroy must be handled, tiles off.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]

pub mod bindings;
pub mod image;
pub mod instance;
pub mod plugin;
pub mod status;
pub mod suites;

pub use bindings::{
    OfxHost, OfxImageClipHandle, OfxImageEffectHandle, OfxParamHandle, OfxParamSetHandle,
    OfxPlugin, OfxPropertySetHandle, OfxRectI, OfxTime,
};
pub use image::{ClipImage, PixelComponents, PixelDepth, RectI, pixel_byte_offset};
pub use instance::{drop_instance_data, get_instance_data, set_instance_data};
pub use plugin::{ImageEffectPlugin, catch_plugin_panic};
pub use status::{OfxResult, OfxStatus, kOfxStat};
pub use suites::{Host, ParamSet, PropertySet, Suites};
