use std::ffi::{CStr, c_char, c_int, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

use crate::bindings::{OfxHost, OfxPlugin, OfxPropertySetHandle, kOfxImageEffectPluginApi};
use crate::status::{OfxStatus, kOfxStat};

pub trait ImageEffectPlugin: Send + Sync + 'static {
    const IDENTIFIER: &'static CStr;
    const VERSION_MAJOR: u32;
    const VERSION_MINOR: u32;

    fn set_host(host: *mut OfxHost) -> OfxStatus;
    fn main_entry(
        action: *const c_char,
        handle: *const c_void,
        in_args: OfxPropertySetHandle,
        out_args: OfxPropertySetHandle,
    ) -> OfxStatus;
}

pub fn catch_plugin_panic<F>(f: F) -> OfxStatus
where
    F: FnOnce() -> OfxStatus,
{
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(status) => status,
        Err(payload) => {
            if let Some(msg) = payload.downcast_ref::<&str>() {
                eprintln!("OpenFX plugin panic: {msg}");
            } else if let Some(msg) = payload.downcast_ref::<String>() {
                eprintln!("OpenFX plugin panic: {msg}");
            } else {
                eprintln!("OpenFX plugin panic");
            }
            kOfxStat::ErrFatal
        }
    }
}

/// Export a single image-effect plugin using the OpenFX C ABI.
#[macro_export]
macro_rules! export_image_effect_plugin {
    ($plugin:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn OfxGetNumberOfPlugins() -> std::ffi::c_int {
            1
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn OfxGetPlugin(nth: std::ffi::c_int) -> *const $crate::OfxPlugin {
            $crate::plugin::plugin_entry::<$plugin>(nth)
        }
    };
}

pub fn plugin_entry<P: ImageEffectPlugin>(nth: c_int) -> *const OfxPlugin {
    if nth != 0 {
        return std::ptr::null();
    }
    static PLUGIN: OnceLock<OfxPlugin> = OnceLock::new();
    let plugin = PLUGIN.get_or_init(|| OfxPlugin {
        pluginApi: kOfxImageEffectPluginApi.as_ptr(),
        apiVersion: 1,
        pluginIdentifier: P::IDENTIFIER.as_ptr(),
        pluginVersionMajor: P::VERSION_MAJOR as std::ffi::c_uint,
        pluginVersionMinor: P::VERSION_MINOR as std::ffi::c_uint,
        setHost: Some(set_host_thunk::<P>),
        mainEntry: Some(main_entry_thunk::<P>),
    });
    plugin as *const OfxPlugin
}

unsafe extern "C" fn set_host_thunk<P: ImageEffectPlugin>(host: *mut OfxHost) {
    let _ = catch_plugin_panic(|| P::set_host(host));
}

unsafe extern "C" fn main_entry_thunk<P: ImageEffectPlugin>(
    action: *const c_char,
    handle: *const c_void,
    in_args: OfxPropertySetHandle,
    out_args: OfxPropertySetHandle,
) -> OfxStatus {
    catch_plugin_panic(|| P::main_entry(action, handle, in_args, out_args))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_becomes_fatal() {
        let status = catch_plugin_panic(|| panic!("boom"));
        assert_eq!(status, kOfxStat::ErrFatal);
    }

    #[test]
    fn ok_passes_through() {
        let status = catch_plugin_panic(|| kOfxStat::OK);
        assert_eq!(status, kOfxStat::OK);
    }

    struct DummyPlugin;

    impl ImageEffectPlugin for DummyPlugin {
        const IDENTIFIER: &'static CStr = c"test.openfx.dummy";
        const VERSION_MAJOR: u32 = 1;
        const VERSION_MINOR: u32 = 2;

        fn set_host(_host: *mut OfxHost) -> OfxStatus {
            kOfxStat::OK
        }

        fn main_entry(
            _action: *const c_char,
            _handle: *const c_void,
            _in_args: OfxPropertySetHandle,
            _out_args: OfxPropertySetHandle,
        ) -> OfxStatus {
            kOfxStat::ReplyDefault
        }
    }

    #[test]
    fn exports_single_plugin() {
        let plugin = plugin_entry::<DummyPlugin>(0);
        assert!(!plugin.is_null());
        unsafe {
            assert_eq!((*plugin).pluginVersionMajor, 1);
            assert_eq!((*plugin).pluginVersionMinor, 2);
            assert!((*plugin).setHost.is_some());
            assert!((*plugin).mainEntry.is_some());
        }
        assert!(plugin_entry::<DummyPlugin>(1).is_null());
    }
}
