mod config;
mod instance;
mod omt_file_log;
mod params;
mod pixels;
mod sender;

pub use config::{PluginConfig, QualitySetting, fps_to_rational};
pub use openfx_pixels::{
    ConvertedVideo, SessionClock, convert_window_to_bgra, video_interval_ticks,
};
pub use sender::{LatestSlot, SendSession, VideoJob};

use std::ffi::{CStr, c_char, c_void};
use std::sync::OnceLock;

use openfx::MultiThread;
use openfx::bindings::{
    OfxHost, OfxImageEffectHandle, OfxPropertySetHandle, kOfxActionCreateInstance,
    kOfxActionDescribe, kOfxActionDestroyInstance, kOfxActionInstanceChanged, kOfxActionLoad,
    kOfxActionUnload, kOfxBitDepthByte, kOfxBitDepthFloat, kOfxBitDepthShort,
    kOfxImageComponentRGB, kOfxImageComponentRGBA, kOfxImageEffectActionDescribeInContext,
    kOfxImageEffectActionRender, kOfxImageEffectContextFilter, kOfxImageEffectContextGeneral,
    kOfxImageEffectFrameVarying, kOfxImageEffectOutputClipName, kOfxImageEffectPluginPropGrouping,
    kOfxImageEffectPluginPropHostFrameThreading, kOfxImageEffectPluginRenderThreadSafety,
    kOfxImageEffectPropFrameRate, kOfxImageEffectPropRenderWindow,
    kOfxImageEffectPropSupportedComponents, kOfxImageEffectPropSupportedContexts,
    kOfxImageEffectPropSupportedPixelDepths, kOfxImageEffectPropSupportsTiles,
    kOfxImageEffectRenderInstanceSafe, kOfxImageEffectSimpleSourceClipName, kOfxPropLabel,
    kOfxPropTime,
};
use openfx::export_image_effect_plugin;
use openfx::image::ClipImage;
use openfx::instance::{drop_instance_data, get_instance_data, set_instance_data};
use openfx::plugin::{ImageEffectPlugin, catch_plugin_panic};
use openfx::status::{OfxResult, OfxStatus, kOfxStat};
use openfx::suites::{Host, Suites};

use crate::config::{PLUGIN_GROUPING, PLUGIN_IDENTIFIER, PLUGIN_LABEL};
use crate::instance::PluginInstance;

const _: &str = PLUGIN_LABEL;
const _: &str = PLUGIN_GROUPING;
const _: &str = PLUGIN_IDENTIFIER;

struct Shared {
    suites: Suites,
    multithread: MultiThread,
}

static HOST: OnceLock<Host> = OnceLock::new();
static SHARED: OnceLock<Shared> = OnceLock::new();

struct OmtFilter;

impl ImageEffectPlugin for OmtFilter {
    const IDENTIFIER: &'static CStr = c"jp.mikanseilaboratory.OpenFXOMT";
    const VERSION_MAJOR: u32 = 0;
    const VERSION_MINOR: u32 = 1;

    fn set_host(host: *mut OfxHost) -> OfxStatus {
        match unsafe { set_host_inner(host) } {
            Ok(()) => kOfxStat::OK,
            Err(status) => status,
        }
    }

    fn main_entry(
        action: *const c_char,
        handle: *const c_void,
        in_args: OfxPropertySetHandle,
        out_args: OfxPropertySetHandle,
    ) -> OfxStatus {
        catch_plugin_panic(|| {
            let action = if action.is_null() {
                return kOfxStat::ReplyDefault;
            } else {
                unsafe { CStr::from_ptr(action) }
            };
            let effect = handle as OfxImageEffectHandle;
            match dispatch(action, effect, in_args, out_args) {
                Ok(()) => kOfxStat::OK,
                Err(status) => status,
            }
        })
    }
}

unsafe fn set_host_inner(host: *mut OfxHost) -> OfxResult<()> {
    let host = unsafe { Host::from_raw(host) }?;
    let _ = HOST.set(host);
    Ok(())
}

fn shared() -> OfxResult<&'static Shared> {
    SHARED.get().ok_or(kOfxStat::Failed)
}

fn action_load() -> OfxResult<()> {
    omt_file_log::init();
    let host = *HOST.get().ok_or(kOfxStat::Failed)?;
    let suites = unsafe { Suites::fetch(host) }?;
    let multithread = unsafe { MultiThread::fetch(host) }?;
    let _ = SHARED.set(Shared {
        suites,
        multithread,
    });
    Ok(())
}

fn dispatch(
    action: &CStr,
    effect: OfxImageEffectHandle,
    in_args: OfxPropertySetHandle,
    _out_args: OfxPropertySetHandle,
) -> OfxResult<()> {
    if action == kOfxActionLoad {
        action_load()
    } else if action == kOfxActionUnload {
        Ok(())
    } else if action == kOfxActionDescribe {
        action_describe(effect)
    } else if action == kOfxImageEffectActionDescribeInContext {
        action_describe_in_context(effect)
    } else if action == kOfxActionCreateInstance {
        action_create_instance(effect)
    } else if action == kOfxActionDestroyInstance {
        action_destroy_instance(effect)
    } else if action == kOfxImageEffectActionRender {
        action_render(effect, in_args)
    } else if action == kOfxActionInstanceChanged {
        action_instance_changed(effect, in_args)
    } else {
        Err(kOfxStat::ReplyDefault)
    }
}

fn action_describe(effect: OfxImageEffectHandle) -> OfxResult<()> {
    let suites = &shared()?.suites;
    let props = suites.effect_properties(effect)?;
    props.set_string(kOfxPropLabel, 0, c"OMT Output")?;
    props.set_string(kOfxImageEffectPluginPropGrouping, 0, c"Mikansei Laboratory")?;
    props.set_string(
        kOfxImageEffectPropSupportedContexts,
        0,
        kOfxImageEffectContextFilter,
    )?;
    props.set_string(
        kOfxImageEffectPropSupportedContexts,
        1,
        kOfxImageEffectContextGeneral,
    )?;
    props.set_string(
        kOfxImageEffectPropSupportedPixelDepths,
        0,
        kOfxBitDepthFloat,
    )?;
    props.set_string(
        kOfxImageEffectPropSupportedPixelDepths,
        1,
        kOfxBitDepthShort,
    )?;
    props.set_string(kOfxImageEffectPropSupportedPixelDepths, 2, kOfxBitDepthByte)?;
    props.set_string(
        kOfxImageEffectPluginRenderThreadSafety,
        0,
        kOfxImageEffectRenderInstanceSafe,
    )?;
    props.set_int(kOfxImageEffectPluginPropHostFrameThreading, 0, 0)?;
    props.set_int(kOfxImageEffectPropSupportsTiles, 0, 0)?;
    Ok(())
}

fn action_describe_in_context(effect: OfxImageEffectHandle) -> OfxResult<()> {
    let suites = &shared()?.suites;
    for name in [
        kOfxImageEffectOutputClipName,
        kOfxImageEffectSimpleSourceClipName,
    ] {
        let clip = suites.clip_define(effect, name)?;
        clip.set_string(
            kOfxImageEffectPropSupportedComponents,
            0,
            kOfxImageComponentRGBA,
        )?;
        clip.set_string(
            kOfxImageEffectPropSupportedComponents,
            1,
            kOfxImageComponentRGB,
        )?;
        clip.set_int(kOfxImageEffectPropSupportsTiles, 0, 0)?;
        if name == kOfxImageEffectOutputClipName {
            let _ = clip.set_int(kOfxImageEffectFrameVarying, 0, 1);
        }
    }
    params::describe(suites, effect)
}

fn action_create_instance(effect: OfxImageEffectHandle) -> OfxResult<()> {
    let suites = shared()?.suites;
    let instance = PluginInstance::create(suites, effect)?;
    set_instance_data(&suites, effect, instance)
}

fn action_destroy_instance(effect: OfxImageEffectHandle) -> OfxResult<()> {
    let suites = &shared()?.suites;
    if let Ok(instance) = get_instance_data::<PluginInstance>(suites, effect) {
        instance.shutdown();
    }
    drop_instance_data::<PluginInstance>(suites, effect)
}

fn action_instance_changed(
    effect: OfxImageEffectHandle,
    in_args: OfxPropertySetHandle,
) -> OfxResult<()> {
    let suites = &shared()?.suites;
    let props = openfx::suites::PropertySet::new(in_args, suites.property)?;
    let time = props.get_double(kOfxPropTime, 0).unwrap_or(0.0);
    let instance = get_instance_data::<PluginInstance>(suites, effect)?;
    instance.sync_from_params(effect, time)
}

fn action_render(effect: OfxImageEffectHandle, in_args: OfxPropertySetHandle) -> OfxResult<()> {
    let shared = shared()?;
    let suites = &shared.suites;
    let in_props = openfx::suites::PropertySet::new(in_args, suites.property)?;
    let time = in_props.get_double(kOfxPropTime, 0)?;
    let mut window_vals = [0; 4];
    in_props.get_int_n(kOfxImageEffectPropRenderWindow, &mut window_vals)?;
    let window = openfx::image::RectI {
        x1: window_vals[0],
        y1: window_vals[1],
        x2: window_vals[2],
        y2: window_vals[3],
    };

    let source_clip = suites.clip_handle(effect, kOfxImageEffectSimpleSourceClipName)?;
    let output_clip = suites.clip_handle(effect, kOfxImageEffectOutputClipName)?;
    let source = unsafe { ClipImage::fetch(suites, source_clip, time) }?;
    let output = unsafe { ClipImage::fetch(suites, output_clip, time) }?;
    let instance = get_instance_data::<PluginInstance>(suites, effect)?;
    let converted = pixels::pass_bgra(
        &source,
        &output,
        window,
        Some(instance.bgra_pool()),
        &shared.multithread,
        instance.is_enabled(),
    )?;
    if let Some(converted) = converted {
        let (fps_n, fps_d) = instance.cached_fps(|| {
            let fps = suites
                .clip_properties(source_clip)
                .ok()
                .and_then(|props| props.get_double(kOfxImageEffectPropFrameRate, 0).ok())
                .unwrap_or(60.0);
            fps_to_rational(fps)
        });
        let mut job = VideoJob::from(converted);
        job.timestamp = instance.next_timestamp();
        job.fps_n = fps_n;
        job.fps_d = fps_d;
        job.ofx_time = time;
        instance.push_video(job);
    }
    Ok(())
}

export_image_effect_plugin!(OmtFilter);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_matches_config() {
        assert_eq!(OmtFilter::IDENTIFIER.to_str().unwrap(), PLUGIN_IDENTIFIER);
    }
}
