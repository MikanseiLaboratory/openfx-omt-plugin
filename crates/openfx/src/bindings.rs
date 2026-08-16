#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
#![allow(clippy::all)]
#![allow(unused)]

use crate::status::OfxStatus;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

unsafe impl Send for OfxPlugin {}
unsafe impl Sync for OfxPlugin {}
