use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use openmediatransport::{
    Codec, ColorSpace, Discovery, FrameType, MediaFrame, Sender, SenderConfig, SenderInfo,
    VideoFlags,
};

use crate::config::{PLUGIN_AUTHOR, PLUGIN_LABEL, PluginConfig, QualitySetting};
use openfx_pixels::{ConvertedVideo, PixelPool, packed_frame_hash};

#[derive(Debug, Clone)]
pub struct VideoJob {
    pub width: u32,
    pub height: u32,
    pub stride: i32,
    pub bgra: Vec<u8>,
    pub has_alpha: bool,
    pub timestamp: i64,
    pub fps_n: i32,
    pub fps_d: i32,
    pub ofx_time: f64,
}

impl From<ConvertedVideo> for VideoJob {
    fn from(value: ConvertedVideo) -> Self {
        Self {
            width: value.width,
            height: value.height,
            stride: value.stride,
            bgra: value.data,
            has_alpha: value.has_alpha,
            timestamp: 0,
            fps_n: 60,
            fps_d: 1,
            ofx_time: 0.0,
        }
    }
}

/// Depth-1 latest-wins slot. Pushing while occupied drops the older value.
#[derive(Debug)]
pub struct LatestSlot<T> {
    slot: Mutex<Option<T>>,
    drops: AtomicU64,
}

impl<T> LatestSlot<T> {
    pub fn new() -> Self {
        Self {
            slot: Mutex::new(None),
            drops: AtomicU64::new(0),
        }
    }

    pub fn push(&self, item: T) {
        drop(self.push_replacing(item));
    }

    pub fn push_replacing(&self, item: T) -> Option<T> {
        let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        let old = slot.replace(item);
        if old.is_some() {
            self.drops.fetch_add(1, Ordering::Relaxed);
        }
        old
    }

    pub fn take(&self) -> Option<T> {
        self.slot.lock().unwrap_or_else(|e| e.into_inner()).take()
    }

    pub fn clear(&self) {
        let _ = self.take();
    }

    pub fn drops(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
}

impl<T> Default for LatestSlot<T> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SendSession {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    video_slot: Arc<LatestSlot<VideoJob>>,
    quality: Arc<Mutex<QualitySetting>>,
    pool: Arc<PixelPool>,
}

impl SendSession {
    pub fn start(config: PluginConfig) -> Result<Self, String> {
        Self::start_with_pool(config, Arc::new(PixelPool::new()))
    }

    pub fn start_with_pool(config: PluginConfig, pool: Arc<PixelPool>) -> Result<Self, String> {
        let mut sender = Sender::create_with_config(
            config.source_name.clone(),
            FrameType::VIDEO | FrameType::METADATA,
            SenderConfig {
                send_queue_depth: crate::config::DEFAULT_QUEUE_DEPTH,
                ..SenderConfig::default()
            },
        )
        .map_err(|e| format!("OMT sender create failed: {e}"))?;
        sender.set_quality(config.quality.to_omt());
        sender.set_sender_info(SenderInfo::new(
            PLUGIN_LABEL,
            PLUGIN_AUTHOR,
            env!("CARGO_PKG_VERSION"),
        ));

        let mut discovery = Discovery::new().ok();
        if let Some(discovery) = discovery.as_mut()
            && let Err(e) = discovery.register(&config.source_name, sender.port())
        {
            eprintln!("OMT DNS-SD register failed: {e}");
        }

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let video_slot = Arc::new(LatestSlot::new());
        let video_slot_thread = Arc::clone(&video_slot);
        let quality = Arc::new(Mutex::new(config.quality));
        let quality_thread = Arc::clone(&quality);
        let pool_thread = Arc::clone(&pool);
        let source_name = config.source_name.clone();

        let join = thread::Builder::new()
            .name("openfx-omt-sender".into())
            .spawn(move || {
                sender_loop(
                    sender,
                    discovery,
                    &source_name,
                    video_slot_thread,
                    quality_thread,
                    pool_thread,
                    stop_thread,
                );
            })
            .map_err(|e| format!("failed to spawn OMT sender thread: {e}"))?;

        Ok(Self {
            stop,
            join: Some(join),
            video_slot,
            quality,
            pool,
        })
    }

    pub fn push_video(&self, job: VideoJob) {
        if let Some(old) = self.video_slot.push_replacing(job) {
            self.pool.release(old.bgra);
        }
    }

    pub fn set_quality(&self, quality: QualitySetting) {
        *self.quality.lock().unwrap_or_else(|e| e.into_inner()) = quality;
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
        if let Some(job) = self.video_slot.take() {
            self.pool.release(job.bgra);
        }
    }
}

impl Drop for SendSession {
    fn drop(&mut self) {
        self.stop();
    }
}

fn sender_loop(
    mut sender: Sender,
    mut discovery: Option<Discovery>,
    source_name: &str,
    video_slot: Arc<LatestSlot<VideoJob>>,
    quality: Arc<Mutex<QualitySetting>>,
    pool: Arc<PixelPool>,
    stop: Arc<AtomicBool>,
) {
    let mut applied_quality: Option<QualitySetting> = None;
    let mut last_time = f64::NAN;
    let mut last_wh = (0u32, 0u32);
    let mut last_hash = 0u64;
    while !stop.load(Ordering::Acquire) {
        let had_job = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            drain_accepts(&mut sender);
            if let Err(e) = sender.poll_peer_metadata() {
                eprintln!("OMT poll_peer_metadata failed: {e}");
            }
            let current_quality = *quality.lock().unwrap_or_else(|e| e.into_inner());
            if applied_quality != Some(current_quality) {
                sender.set_quality(current_quality.to_omt());
                applied_quality = Some(current_quality);
            }
            if let Some(job) = video_slot.take() {
                // Playback always has a new OFX time — skip the 8 MiB CRC.
                // Pause/scrub repeats ofx_time; hash only then.
                if job.ofx_time == last_time && last_wh == (job.width, job.height) {
                    let hash = packed_frame_hash(job.width, job.height, &job.bgra);
                    if hash == last_hash {
                        pool.release(job.bgra);
                        return true;
                    }
                    last_hash = hash;
                } else {
                    last_time = job.ofx_time;
                    last_wh = (job.width, job.height);
                    last_hash = 0;
                }
                if let Err(e) = sender.send_video(video_frame(job)) {
                    eprintln!("OMT send_video failed: {e}");
                }
                true
            } else {
                false
            }
        }));
        match had_job {
            Ok(true) => {}
            Ok(false) => thread::sleep(Duration::from_millis(1)),
            Err(_) => {
                eprintln!("OMT sender loop panicked; keeping sender thread alive");
                thread::sleep(Duration::from_millis(1));
            }
        }
    }

    if let Some(discovery) = discovery.as_mut() {
        let _ = discovery.deregister(source_name);
    }
    let _ = sender.send_metadata(0, "<OMTMetadata />");
    let _ = Instant::now();
}

fn drain_accepts(sender: &mut Sender) {
    loop {
        match sender.poll_accept() {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                eprintln!("OMT poll_accept failed: {e}");
                break;
            }
        }
    }
}

fn video_frame(job: VideoJob) -> MediaFrame {
    let flags = if job.has_alpha {
        VideoFlags::ALPHA
    } else {
        VideoFlags::NONE
    };
    MediaFrame {
        frame_type: FrameType::VIDEO,
        timestamp: job.timestamp,
        codec: Codec::Bgra as i32,
        width: job.width as i32,
        height: job.height as i32,
        stride: job.stride,
        flags,
        frame_rate_n: job.fps_n,
        frame_rate_d: job.fps_d,
        aspect_ratio: if job.height == 0 {
            1.0
        } else {
            job.width as f32 / job.height as f32
        },
        color_space: ColorSpace::Bt709,
        data: job.bgra,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_slot_drops_old() {
        let slot = LatestSlot::new();
        slot.push(1);
        slot.push(2);
        assert_eq!(slot.take(), Some(2));
        assert_eq!(slot.drops(), 1);
        assert_eq!(slot.take(), None);
    }

    #[test]
    fn latest_wins_under_contention() {
        let slot = Arc::new(LatestSlot::new());
        let mut handles = Vec::new();
        for i in 0..8 {
            let slot = Arc::clone(&slot);
            handles.push(thread::spawn(move || {
                for j in 0..50 {
                    slot.push(i * 100 + j);
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert!(slot.take().is_some());
    }

    #[test]
    fn stop_is_idempotent() {
        let mut session = SendSession {
            stop: Arc::new(AtomicBool::new(false)),
            join: None,
            video_slot: Arc::new(LatestSlot::new()),
            quality: Arc::new(Mutex::new(QualitySetting::Default)),
            pool: Arc::new(PixelPool::new()),
        };
        session.stop();
        session.stop();
        assert!(session.stop.load(Ordering::Acquire));
    }

    #[test]
    fn quality_can_be_updated_without_stop() {
        let session = SendSession {
            stop: Arc::new(AtomicBool::new(false)),
            join: None,
            video_slot: Arc::new(LatestSlot::new()),
            quality: Arc::new(Mutex::new(QualitySetting::Default)),
            pool: Arc::new(PixelPool::new()),
        };
        session.set_quality(QualitySetting::High);
        assert_eq!(*session.quality.lock().unwrap(), QualitySetting::High);
    }

    #[test]
    fn send_session_start_and_stop() {
        let mut session = SendSession::start(PluginConfig {
            enabled: true,
            source_name: "openfx-omt-stop-test".into(),
            quality: crate::config::QualitySetting::Low,
        })
        .expect("start sender");
        session.push_video(VideoJob {
            width: 16,
            height: 16,
            stride: 64,
            bgra: vec![0u8; 16 * 16 * 4],
            has_alpha: false,
            timestamp: 1,
            fps_n: 60,
            fps_d: 1,
            ofx_time: 0.0,
        });
        session.stop();
        session.stop();
    }
}
