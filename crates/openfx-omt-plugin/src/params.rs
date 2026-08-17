use std::ffi::CStr;
use std::sync::OnceLock;

use openfx::bindings::{
    OfxImageEffectHandle, kOfxParamPropAnimates, kOfxParamPropChoiceOption, kOfxParamPropDefault,
    kOfxParamPropHint, kOfxParamPropScriptName, kOfxParamTypeBoolean, kOfxParamTypeChoice,
    kOfxParamTypeString, kOfxPropLabel,
};
use openfx::status::OfxResult;
use openfx::suites::Suites;

use crate::config::{PluginConfig, QualitySetting};

pub const PARAM_ENABLED: &CStr = c"Enabled";
pub const PARAM_SOURCE_NAME: &CStr = c"SourceName";
pub const PARAM_QUALITY: &CStr = c"Quality";

fn cstring_pool() -> &'static [std::ffi::CString] {
    static POOL: OnceLock<Vec<std::ffi::CString>> = OnceLock::new();
    POOL.get_or_init(|| {
        let mut out = vec![
            std::ffi::CString::new("Enabled").unwrap(),
            std::ffi::CString::new("Send frames over OMT while previewing").unwrap(),
            std::ffi::CString::new("Source Name").unwrap(),
            std::ffi::CString::new("OMT source name advertised on the network").unwrap(),
            std::ffi::CString::new("Quality").unwrap(),
            std::ffi::CString::new("OMT encode quality").unwrap(),
            std::ffi::CString::new(crate::config::DEFAULT_SOURCE_NAME).unwrap(),
        ];
        for quality in QualitySetting::ALL {
            out.push(std::ffi::CString::new(quality.as_label()).unwrap());
        }
        out
    })
}

pub fn describe(suites: &Suites, effect: OfxImageEffectHandle) -> OfxResult<()> {
    let param_set = suites.param_set(effect)?;
    let strings = cstring_pool();

    let enabled = suites.param_define(param_set.handle, kOfxParamTypeBoolean, PARAM_ENABLED)?;
    enabled.set_string(kOfxPropLabel, 0, c"Enabled")?;
    enabled.set_string(kOfxParamPropHint, 0, strings[1].as_c_str())?;
    enabled.set_string(kOfxParamPropScriptName, 0, PARAM_ENABLED)?;
    enabled.set_int(kOfxParamPropDefault, 0, 1)?;
    enabled.set_int(kOfxParamPropAnimates, 0, 0)?;

    let name = suites.param_define(param_set.handle, kOfxParamTypeString, PARAM_SOURCE_NAME)?;
    name.set_string(kOfxPropLabel, 0, strings[2].as_c_str())?;
    name.set_string(kOfxParamPropHint, 0, strings[3].as_c_str())?;
    name.set_string(kOfxParamPropScriptName, 0, PARAM_SOURCE_NAME)?;
    name.set_string(kOfxParamPropDefault, 0, strings[6].as_c_str())?;
    name.set_int(kOfxParamPropAnimates, 0, 0)?;

    let quality = suites.param_define(param_set.handle, kOfxParamTypeChoice, PARAM_QUALITY)?;
    quality.set_string(kOfxPropLabel, 0, strings[4].as_c_str())?;
    quality.set_string(kOfxParamPropHint, 0, strings[5].as_c_str())?;
    quality.set_string(kOfxParamPropScriptName, 0, PARAM_QUALITY)?;
    for i in 0..QualitySetting::ALL.len() {
        let cstr = strings[7 + i].as_c_str();
        quality.set_string(kOfxParamPropChoiceOption, i as i32, cstr)?;
    }
    quality.set_int(kOfxParamPropDefault, 0, 0)?;
    quality.set_int(kOfxParamPropAnimates, 0, 0)?;
    Ok(())
}

pub fn read_config(
    suites: &Suites,
    effect: OfxImageEffectHandle,
    time: f64,
) -> OfxResult<PluginConfig> {
    let params = suites.param_set(effect)?;
    let enabled = params.get_bool_at(PARAM_ENABLED, time)?;
    let source_name = params.get_string_at(PARAM_SOURCE_NAME, time)?;
    let quality = QualitySetting::from_index(params.get_choice_at(PARAM_QUALITY, time)?);
    Ok(PluginConfig {
        enabled,
        source_name,
        quality,
    }
    .clamped())
}
