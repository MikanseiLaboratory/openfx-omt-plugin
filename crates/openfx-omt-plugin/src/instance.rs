use std::sync::{Arc, Mutex};

use openfx::bindings::{OfxImageEffectHandle, OfxTime};
use openfx::status::OfxResult;
use openfx::suites::Suites;

use crate::config::PluginConfig;
use crate::params;
use crate::sender::{SendSession, VideoJob};
use openfx_pixels::{PixelPool, SessionClock};

pub struct PluginInstance {
    pub suites: Suites,
    config: Mutex<PluginConfig>,
    session: Mutex<Option<SendSession>>,
    clock: Mutex<SessionClock>,
    bgra_pool: Arc<PixelPool>,
}

#[cfg(test)]
pub struct DuplicateTimeGuard {
    last: Mutex<Option<OfxTime>>,
}

#[cfg(test)]
impl DuplicateTimeGuard {
    pub fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }

    pub fn should_send(&self, time: OfxTime) -> bool {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if *last == Some(time) {
            return false;
        }
        *last = Some(time);
        true
    }
}

#[cfg(test)]
impl Default for DuplicateTimeGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginInstance {
    pub fn create(suites: Suites, effect: OfxImageEffectHandle) -> OfxResult<Self> {
        let config = params::read_config(&suites, effect, 0.0)?;
        let instance = Self {
            suites,
            config: Mutex::new(config.clone()),
            session: Mutex::new(None),
            clock: Mutex::new(SessionClock::new()),
            bgra_pool: Arc::new(PixelPool::new()),
        };
        instance.apply_config(config);
        Ok(instance)
    }

    pub fn sync_from_params(&self, effect: OfxImageEffectHandle, time: OfxTime) -> OfxResult<()> {
        let Ok(config) = params::read_config(&self.suites, effect, time) else {
            return Ok(());
        };
        let changed = {
            let current = self.config.lock().unwrap_or_else(|e| e.into_inner());
            *current != config
        };
        if changed {
            self.apply_config(config);
        }
        Ok(())
    }

    pub fn apply_config(&self, config: PluginConfig) {
        let previous = {
            let mut current = self.config.lock().unwrap_or_else(|e| e.into_inner());
            let previous = current.clone();
            *current = config.clone();
            previous
        };
        let mut session = self.session.lock().unwrap_or_else(|e| e.into_inner());
        if !config.enabled {
            if let Some(existing) = session.as_mut() {
                existing.stop();
            }
            *session = None;
            return;
        }
        if session.is_none() || config.needs_sender_restart(&previous) {
            if let Some(existing) = session.as_mut() {
                existing.stop();
            }
            *session = None;
            match SendSession::start_with_pool(config, Arc::clone(&self.bgra_pool)) {
                Ok(started) => *session = Some(started),
                Err(err) => eprintln!("OMT sender start failed: {err}"),
            }
            return;
        }
        if previous.quality != config.quality
            && let Some(existing) = session.as_mut()
        {
            existing.set_quality(config.quality);
        }
    }

    pub fn config_snapshot(&self) -> PluginConfig {
        self.config
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn bgra_pool(&self) -> &PixelPool {
        &self.bgra_pool
    }

    pub fn next_timestamp(&self) -> i64 {
        self.clock
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .next_monotonic()
    }

    pub fn push_video(&self, job: VideoJob) {
        if let Some(session) = self
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            session.push_video(job);
        }
    }

    pub fn shutdown(&self) {
        if let Some(session) = self
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            session.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::QualitySetting;

    #[test]
    fn duplicate_times_are_suppressed() {
        let guard = DuplicateTimeGuard::new();
        assert!(guard.should_send(1.0));
        assert!(!guard.should_send(1.0));
        assert!(guard.should_send(2.0));
    }

    #[test]
    fn timestamps_are_monotonic() {
        let mut clock = SessionClock::new();
        let a = clock.next_monotonic();
        let b = clock.next_monotonic();
        assert!(b > a);
        assert!(a >= 0);
    }

    #[test]
    fn config_equality_detects_restart() {
        let a = PluginConfig::default();
        let mut b = a.clone();
        b.quality = QualitySetting::High;
        assert_ne!(a, b);
        assert!(!a.needs_sender_restart(&b));
        assert!(!b.needs_sender_restart(&a));
        b = a.clone();
        b.enabled = false;
        assert_ne!(a, b);
        assert!(a.needs_sender_restart(&b));
        b = a.clone();
        b.source_name = "Other".into();
        assert!(a.needs_sender_restart(&b));
    }
}
