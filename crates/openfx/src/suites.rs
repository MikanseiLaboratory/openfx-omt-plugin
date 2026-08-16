use std::ffi::{CStr, c_char, c_int, c_void};
use std::ptr;

use crate::bindings::{
    OfxHost, OfxImageClipHandle, OfxImageEffectHandle, OfxImageEffectSuiteV1, OfxParamHandle,
    OfxParamSetHandle, OfxParameterSuiteV1, OfxPropertySetHandle, OfxPropertySuiteV1, OfxRectD,
    OfxRectI, OfxTime, kOfxImageEffectSuite, kOfxParameterSuite, kOfxPropertySuite,
};
use crate::status::{OfxResult, OfxStatus, kOfxStat};

/// Host bootstrap data. Pointers remain valid while the plugin binary is loaded.
#[derive(Clone, Copy)]
pub struct Host {
    pub host: *mut crate::bindings::OfxPropertySetStruct,
    pub fetch_suite: unsafe extern "C" fn(
        host: OfxPropertySetHandle,
        suite_name: *const c_char,
        suite_version: c_int,
    ) -> *const c_void,
}

unsafe impl Send for Host {}
unsafe impl Sync for Host {}

impl Host {
    pub unsafe fn from_raw(host: *mut OfxHost) -> OfxResult<Self> {
        let host_struct = unsafe { host.as_ref() }.ok_or(kOfxStat::Failed)?;
        let host_props = host_struct.host;
        if host_props.is_null() {
            return Err(kOfxStat::Failed);
        }
        let fetch_suite = host_struct.fetchSuite.ok_or(kOfxStat::Failed)?;
        Ok(Self {
            host: host_props,
            fetch_suite,
        })
    }

    unsafe fn fetch(&self, name: &CStr, version: c_int) -> *const c_void {
        unsafe { (self.fetch_suite)(self.host, name.as_ptr(), version) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::OfxHost;

    #[test]
    fn null_host_is_failed() {
        assert!(unsafe { Host::from_raw(std::ptr::null_mut()) }.is_err());
    }

    #[test]
    fn host_without_fetch_suite_fails() {
        let mut host = OfxHost {
            host: std::ptr::null_mut(),
            fetchSuite: None,
        };
        assert!(unsafe { Host::from_raw(&mut host) }.is_err());
    }
}

/// Property / ImageEffect / Parameter suites fetched from the host.
#[derive(Clone, Copy)]
pub struct Suites {
    pub property: &'static OfxPropertySuiteV1,
    pub image_effect: &'static OfxImageEffectSuiteV1,
    pub parameter: &'static OfxParameterSuiteV1,
}

unsafe impl Send for Suites {}
unsafe impl Sync for Suites {}

impl Suites {
    pub unsafe fn fetch(host: Host) -> OfxResult<Self> {
        let property = unsafe { host.fetch(kOfxPropertySuite, 1) } as *const OfxPropertySuiteV1;
        let image_effect =
            unsafe { host.fetch(kOfxImageEffectSuite, 1) } as *const OfxImageEffectSuiteV1;
        let parameter = unsafe { host.fetch(kOfxParameterSuite, 1) } as *const OfxParameterSuiteV1;
        Ok(Self {
            property: unsafe { property.as_ref() }.ok_or(kOfxStat::ErrMissingHostFeature)?,
            image_effect: unsafe { image_effect.as_ref() }
                .ok_or(kOfxStat::ErrMissingHostFeature)?,
            parameter: unsafe { parameter.as_ref() }.ok_or(kOfxStat::ErrMissingHostFeature)?,
        })
    }

    pub fn effect_properties(&self, effect: OfxImageEffectHandle) -> OfxResult<PropertySet<'_>> {
        let get = self
            .image_effect
            .getPropertySet
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut props: OfxPropertySetHandle = ptr::null_mut();
        unsafe { get(effect, &mut props) }.ofx_ok()?;
        PropertySet::new(props, self.property)
    }

    pub fn clip_define(
        &self,
        effect: OfxImageEffectHandle,
        name: &CStr,
    ) -> OfxResult<PropertySet<'_>> {
        let define = self
            .image_effect
            .clipDefine
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut props: OfxPropertySetHandle = ptr::null_mut();
        unsafe { define(effect, name.as_ptr(), &mut props) }.ofx_ok()?;
        PropertySet::new(props, self.property)
    }

    pub fn clip_handle(
        &self,
        effect: OfxImageEffectHandle,
        name: &CStr,
    ) -> OfxResult<OfxImageClipHandle> {
        let get = self
            .image_effect
            .clipGetHandle
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut clip: OfxImageClipHandle = ptr::null_mut();
        unsafe { get(effect, name.as_ptr(), &mut clip, ptr::null_mut()) }.ofx_ok()?;
        if clip.is_null() {
            return Err(kOfxStat::Failed);
        }
        Ok(clip)
    }

    pub fn clip_properties(&self, clip: OfxImageClipHandle) -> OfxResult<PropertySet<'_>> {
        let get = self
            .image_effect
            .clipGetPropertySet
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut props: OfxPropertySetHandle = ptr::null_mut();
        unsafe { get(clip, &mut props) }.ofx_ok()?;
        PropertySet::new(props, self.property)
    }

    pub fn param_set(&self, effect: OfxImageEffectHandle) -> OfxResult<ParamSet<'_>> {
        let get = self
            .image_effect
            .getParamSet
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut set: OfxParamSetHandle = ptr::null_mut();
        unsafe { get(effect, &mut set) }.ofx_ok()?;
        if set.is_null() {
            return Err(kOfxStat::Failed);
        }
        Ok(ParamSet {
            handle: set,
            suites: self,
        })
    }

    pub fn param_define(
        &self,
        param_set: OfxParamSetHandle,
        param_type: &CStr,
        name: &CStr,
    ) -> OfxResult<PropertySet<'_>> {
        let define = self
            .parameter
            .paramDefine
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut props: OfxPropertySetHandle = ptr::null_mut();
        unsafe { define(param_set, param_type.as_ptr(), name.as_ptr(), &mut props) }.ofx_ok()?;
        PropertySet::new(props, self.property)
    }
}

pub struct PropertySet<'a> {
    pub handle: OfxPropertySetHandle,
    suite: &'a OfxPropertySuiteV1,
}

impl<'a> PropertySet<'a> {
    pub fn new(handle: OfxPropertySetHandle, suite: &'a OfxPropertySuiteV1) -> OfxResult<Self> {
        if handle.is_null() {
            return Err(kOfxStat::Failed);
        }
        Ok(Self { handle, suite })
    }

    pub fn set_string(&self, name: &CStr, index: c_int, value: &CStr) -> OfxResult<()> {
        let set = self
            .suite
            .propSetString
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        unsafe { set(self.handle, name.as_ptr(), index, value.as_ptr()) }.ofx_ok()
    }

    pub fn get_string(&self, name: &CStr, index: c_int) -> OfxResult<&'a CStr> {
        let get = self
            .suite
            .propGetString
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut ptr: *mut c_char = ptr::null_mut();
        unsafe { get(self.handle, name.as_ptr(), index, &mut ptr) }.ofx_ok()?;
        if ptr.is_null() {
            return Err(kOfxStat::Failed);
        }
        Ok(unsafe { CStr::from_ptr(ptr) })
    }

    pub fn set_int(&self, name: &CStr, index: c_int, value: c_int) -> OfxResult<()> {
        let set = self
            .suite
            .propSetInt
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        unsafe { set(self.handle, name.as_ptr(), index, value) }.ofx_ok()
    }

    pub fn get_int(&self, name: &CStr, index: c_int) -> OfxResult<c_int> {
        let get = self
            .suite
            .propGetInt
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut value = 0;
        unsafe { get(self.handle, name.as_ptr(), index, &mut value) }.ofx_ok()?;
        Ok(value)
    }

    pub fn get_int_n(&self, name: &CStr, values: &mut [c_int]) -> OfxResult<()> {
        let get = self
            .suite
            .propGetIntN
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        unsafe {
            get(
                self.handle,
                name.as_ptr(),
                values.len() as c_int,
                values.as_mut_ptr(),
            )
        }
        .ofx_ok()
    }

    pub fn get_double(&self, name: &CStr, index: c_int) -> OfxResult<f64> {
        let get = self
            .suite
            .propGetDouble
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut value = 0.0;
        unsafe { get(self.handle, name.as_ptr(), index, &mut value) }.ofx_ok()?;
        Ok(value)
    }

    pub fn get_pointer(&self, name: &CStr, index: c_int) -> OfxResult<*mut c_void> {
        let get = self
            .suite
            .propGetPointer
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut value: *mut c_void = ptr::null_mut();
        unsafe { get(self.handle, name.as_ptr(), index, &mut value) }.ofx_ok()?;
        Ok(value)
    }

    pub fn set_pointer(&self, name: &CStr, index: c_int, value: *mut c_void) -> OfxResult<()> {
        let set = self
            .suite
            .propSetPointer
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        unsafe { set(self.handle, name.as_ptr(), index, value) }.ofx_ok()
    }

    pub fn get_rect_i(&self, name: &CStr) -> OfxResult<OfxRectI> {
        let mut values = [0; 4];
        self.get_int_n(name, &mut values)?;
        Ok(OfxRectI {
            x1: values[0],
            y1: values[1],
            x2: values[2],
            y2: values[3],
        })
    }
}

pub struct ParamSet<'a> {
    pub handle: OfxParamSetHandle,
    suites: &'a Suites,
}

impl<'a> ParamSet<'a> {
    fn param_handle(&self, name: &CStr) -> OfxResult<OfxParamHandle> {
        let get = self
            .suites
            .parameter
            .paramGetHandle
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        let mut handle: OfxParamHandle = ptr::null_mut();
        unsafe { get(self.handle, name.as_ptr(), &mut handle, ptr::null_mut()) }.ofx_ok()?;
        if handle.is_null() {
            return Err(kOfxStat::Failed);
        }
        Ok(handle)
    }

    pub fn get_bool_at(&self, name: &CStr, time: OfxTime) -> OfxResult<bool> {
        let handle = self.param_handle(name)?;
        let get = self
            .suites
            .parameter
            .paramGetValueAtTime
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        type FnInt = unsafe extern "C" fn(OfxParamHandle, OfxTime, *mut c_int) -> OfxStatus;
        let get: FnInt = unsafe { std::mem::transmute(get) };
        let mut value = 0;
        unsafe { get(handle, time, &mut value) }.ofx_ok()?;
        Ok(value != 0)
    }

    pub fn get_choice_at(&self, name: &CStr, time: OfxTime) -> OfxResult<c_int> {
        let handle = self.param_handle(name)?;
        let get = self
            .suites
            .parameter
            .paramGetValueAtTime
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        type FnInt = unsafe extern "C" fn(OfxParamHandle, OfxTime, *mut c_int) -> OfxStatus;
        let get: FnInt = unsafe { std::mem::transmute(get) };
        let mut value = 0;
        unsafe { get(handle, time, &mut value) }.ofx_ok()?;
        Ok(value)
    }

    pub fn get_string_at(&self, name: &CStr, time: OfxTime) -> OfxResult<String> {
        let handle = self.param_handle(name)?;
        let get = self
            .suites
            .parameter
            .paramGetValueAtTime
            .ok_or(kOfxStat::ErrMissingHostFeature)?;
        type FnStr = unsafe extern "C" fn(OfxParamHandle, OfxTime, *mut *mut c_char) -> OfxStatus;
        let get: FnStr = unsafe { std::mem::transmute(get) };
        let mut ptr: *mut c_char = ptr::null_mut();
        unsafe { get(handle, time, &mut ptr) }.ofx_ok()?;
        if ptr.is_null() {
            return Ok(String::new());
        }
        Ok(unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned())
    }
}

/// Region passed to `clipGetImage`. `None` means the full image.
pub type OptionalRegion<'a> = Option<&'a OfxRectD>;
