//! OMT sender/receiver loopback for synthetic BGRA video.

use std::thread;
use std::time::Duration;

use openfx::image::{PixelComponents, PixelDepth, RectI};
use openfx_omt::{ConvertedVideo, convert_window_to_bgra};
use openmediatransport::{
    Codec, ColorSpace, FrameType, MediaFrame, ReceiverConfig, ReceiverSession, Sender, VideoFlags,
};

fn wait_for_subscribe(sender: &mut Sender) {
    for _ in 0..80 {
        let _ = sender.poll_accept();
        let _ = sender.poll_peer_metadata();
        if sender.video_subscribed() {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    sender.force_subscribe(true, false, true);
}

fn solid_bgra(size: u32, r: u8, g: u8, b: u8, a: u8) -> ConvertedVideo {
    let window = RectI {
        x1: 0,
        y1: 0,
        x2: size as i32,
        y2: size as i32,
    };
    convert_window_to_bgra(window, PixelDepth::Byte, PixelComponents::Rgba, |_, _| {
        Some(vec![r, g, b, a])
    })
    .expect("convert")
}

fn video_frame(converted: ConvertedVideo, timestamp: i64) -> MediaFrame {
    MediaFrame {
        frame_type: FrameType::VIDEO,
        timestamp,
        codec: Codec::Bgra as i32,
        width: converted.width as i32,
        height: converted.height as i32,
        stride: converted.stride,
        flags: if converted.has_alpha {
            VideoFlags::ALPHA
        } else {
            VideoFlags::NONE
        },
        frame_rate_n: 60,
        frame_rate_d: 1,
        color_space: ColorSpace::Bt709,
        data: converted.bgra,
        ..Default::default()
    }
}

#[test]
fn bgra_loopback_timestamps_and_reconnect() {
    let mut sender = Sender::create(
        "openfx-omt-loopback",
        FrameType::VIDEO | FrameType::METADATA,
    )
    .expect("sender");
    let port = sender.port();
    let url = format!("omt://127.0.0.1:{port}");

    let session = ReceiverSession::connect(
        url.clone(),
        ReceiverConfig {
            frame_types: FrameType::VIDEO | FrameType::METADATA,
            connect_timeout: Duration::from_secs(5),
            auto_reconnect: false,
            ..ReceiverConfig::default()
        },
    )
    .expect("connect");

    wait_for_subscribe(&mut sender);

    let converted = solid_bgra(32, 10, 20, 30, 255);
    assert_eq!(converted.width, 32);
    assert_eq!(converted.height, 32);
    let video_ts = 1_000_000i64;
    sender
        .send_video(video_frame(converted, video_ts))
        .expect("send_video");

    let video = session
        .recv_video_timeout(Duration::from_secs(3))
        .expect("decoded video");
    assert_eq!(video.width, 32);
    assert_eq!(video.height, 32);
    assert_eq!(video.timestamp, video_ts);

    session.disconnect();

    let session2 = ReceiverSession::connect(
        url,
        ReceiverConfig {
            frame_types: FrameType::VIDEO,
            connect_timeout: Duration::from_secs(5),
            auto_reconnect: false,
            ..ReceiverConfig::default()
        },
    )
    .expect("reconnect");
    wait_for_subscribe(&mut sender);

    let converted = solid_bgra(32, 1, 2, 3, 255);
    sender
        .send_video(video_frame(converted, 2_000_000))
        .expect("send after reconnect");
    let video = session2
        .recv_video_timeout(Duration::from_secs(3))
        .expect("video after reconnect");
    assert_eq!(video.timestamp, 2_000_000);
    session2.disconnect();
}
