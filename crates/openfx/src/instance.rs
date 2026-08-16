use std::any::TypeId;
use std::ffi::c_void;

use crate::bindings::{OfxImageEffectHandle, kOfxPropInstanceData};
use crate::status::{OfxResult, kOfxStat};
use crate::suites::Suites;

#[repr(C)]
struct InstanceSlot<T> {
    type_id: TypeId,
    value: T,
}

/// Store `data` on the effect instance. Fails if instance data is already set.
pub fn set_instance_data<T: 'static>(
    suites: &Suites,
    effect: OfxImageEffectHandle,
    data: T,
) -> OfxResult<()> {
    let props = suites.effect_properties(effect)?;
    let existing = props
        .get_pointer(kOfxPropInstanceData, 0)
        .unwrap_or(std::ptr::null_mut());
    if !existing.is_null() {
        return Err(kOfxStat::ErrExists);
    }
    let boxed = Box::new(InstanceSlot {
        type_id: TypeId::of::<T>(),
        value: data,
    });
    let ptr = Box::into_raw(boxed) as *mut c_void;
    if let Err(status) = props.set_pointer(kOfxPropInstanceData, 0, ptr) {
        unsafe {
            drop(Box::from_raw(ptr as *mut InstanceSlot<T>));
        }
        return Err(status);
    }
    Ok(())
}

/// Borrow instance data. Returns `Err` if missing or the stored type does not match `T`.
pub fn get_instance_data<T: 'static>(
    suites: &Suites,
    effect: OfxImageEffectHandle,
) -> OfxResult<&T> {
    let slot = instance_slot::<T>(suites, effect)?;
    Ok(&slot.value)
}

/// Take ownership of instance data and clear the host pointer.
pub fn drop_instance_data<T: 'static>(
    suites: &Suites,
    effect: OfxImageEffectHandle,
) -> OfxResult<()> {
    let props = suites.effect_properties(effect)?;
    let ptr = props.get_pointer(kOfxPropInstanceData, 0)?;
    if ptr.is_null() {
        return Err(kOfxStat::Failed);
    }
    let type_id = unsafe { *(ptr as *const TypeId) };
    if type_id != TypeId::of::<T>() {
        return Err(kOfxStat::ErrBadHandle);
    }
    props.set_pointer(kOfxPropInstanceData, 0, std::ptr::null_mut())?;
    unsafe {
        drop(Box::from_raw(ptr as *mut InstanceSlot<T>));
    }
    Ok(())
}

fn instance_slot<T: 'static>(
    suites: &Suites,
    effect: OfxImageEffectHandle,
) -> OfxResult<&InstanceSlot<T>> {
    let props = suites.effect_properties(effect)?;
    let ptr = props.get_pointer(kOfxPropInstanceData, 0)?;
    if ptr.is_null() {
        return Err(kOfxStat::Failed);
    }
    let type_id = unsafe { *(ptr as *const TypeId) };
    if type_id != TypeId::of::<T>() {
        return Err(kOfxStat::ErrBadHandle);
    }
    Ok(unsafe { &*(ptr as *const InstanceSlot<T>) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings::{
        OfxImageEffectHandle, OfxImageEffectSuiteV1, OfxParameterSuiteV1, OfxPropertySetHandle,
        OfxPropertySuiteV1, kOfxPropInstanceData,
    };
    use crate::status::OfxStatus;
    use crate::suites::Suites;
    use std::ffi::{c_char, c_int, c_void};
    use std::ptr;
    use std::sync::Mutex;

    struct MockState {
        instance_data: *mut c_void,
    }

    unsafe impl Send for MockState {}
    unsafe impl Sync for MockState {}

    struct MockHost {
        state: Box<MockState>,
        suites: Suites,
    }

    fn mock_host() -> MockHost {
        static PROPERTY: std::sync::OnceLock<OfxPropertySuiteV1> = std::sync::OnceLock::new();
        static EFFECT: std::sync::OnceLock<OfxImageEffectSuiteV1> = std::sync::OnceLock::new();
        static PARAMETER: std::sync::OnceLock<OfxParameterSuiteV1> = std::sync::OnceLock::new();
        static SETUP: Mutex<()> = Mutex::new(());
        let _guard = SETUP.lock().unwrap_or_else(|e| e.into_inner());

        let property = PROPERTY.get_or_init(|| {
            let mut suite: OfxPropertySuiteV1 = unsafe { std::mem::zeroed() };
            suite.propGetPointer = Some(mock_get_pointer);
            suite.propSetPointer = Some(mock_set_pointer);
            suite
        });
        let image_effect = EFFECT.get_or_init(|| {
            let mut suite: OfxImageEffectSuiteV1 = unsafe { std::mem::zeroed() };
            suite.getPropertySet = Some(mock_get_property_set);
            suite
        });
        let parameter = PARAMETER.get_or_init(|| unsafe { std::mem::zeroed() });

        MockHost {
            state: Box::new(MockState {
                instance_data: ptr::null_mut(),
            }),
            suites: Suites {
                property,
                image_effect,
                parameter,
            },
        }
    }

    fn effect_handle(host: &MockHost) -> OfxImageEffectHandle {
        ptr::from_ref(host.state.as_ref()) as OfxImageEffectHandle
    }

    unsafe extern "C" fn mock_get_property_set(
        effect: OfxImageEffectHandle,
        prop_handle: *mut OfxPropertySetHandle,
    ) -> OfxStatus {
        unsafe { *prop_handle = effect as OfxPropertySetHandle };
        kOfxStat::OK
    }

    unsafe extern "C" fn mock_get_pointer(
        handle: OfxPropertySetHandle,
        name: *const c_char,
        _index: c_int,
        value: *mut *mut c_void,
    ) -> OfxStatus {
        let expected = kOfxPropInstanceData.to_bytes();
        let actual = unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes();
        if actual != expected {
            return kOfxStat::ErrValue;
        }
        let state = unsafe { &*(handle as *const MockState) };
        unsafe { *value = state.instance_data };
        kOfxStat::OK
    }

    unsafe extern "C" fn mock_set_pointer(
        handle: OfxPropertySetHandle,
        name: *const c_char,
        _index: c_int,
        value: *mut c_void,
    ) -> OfxStatus {
        let expected = kOfxPropInstanceData.to_bytes();
        let actual = unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes();
        if actual != expected {
            return kOfxStat::ErrValue;
        }
        let state = unsafe { &mut *(handle as *mut MockState) };
        state.instance_data = value;
        kOfxStat::OK
    }

    #[test]
    fn stores_and_drops_typed_instance_data() {
        let host = mock_host();
        let effect = effect_handle(&host);
        set_instance_data(&host.suites, effect, 42u32).unwrap();
        assert_eq!(*get_instance_data::<u32>(&host.suites, effect).unwrap(), 42);
        assert_eq!(
            get_instance_data::<i32>(&host.suites, effect).unwrap_err(),
            kOfxStat::ErrBadHandle
        );
        assert_eq!(
            set_instance_data(&host.suites, effect, 1u32).unwrap_err(),
            kOfxStat::ErrExists
        );
        drop_instance_data::<u32>(&host.suites, effect).unwrap();
        assert_eq!(
            get_instance_data::<u32>(&host.suites, effect).unwrap_err(),
            kOfxStat::Failed
        );
        assert_eq!(
            drop_instance_data::<u32>(&host.suites, effect).unwrap_err(),
            kOfxStat::Failed
        );
    }
}
