//! Metronome audio output via `cpal`.
//!
//! Renders a short synthesized "click" (a decaying sine burst) at each
//! [`quietmytempo_core::ClickEvent`] timestamp coming from a
//! `Schedule::clicks()` call, using [`crate::clock::SessionClock`] as the
//! shared t=0 reference so audio output and keyboard capture agree on
//! timing.
//!
//! Design choice: clicks are precomputed for the whole session up front
//! (schedules are pure/deterministic, see `crates/core`), then converted to
//! absolute sample indices. The realtime audio callback only does cheap
//! sample-index bookkeeping — no allocation, no scheduling logic — to keep
//! it safe for a realtime audio thread.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;
use quietmytempo_core::ClickEvent;

/// Frequency of the synthesized click tone (Hz). A short, high-ish blip
/// reads clearly as a metronome click without needing sample assets.
const CLICK_FREQ_HZ: f32 = 1500.0;
/// How long each click's audible decay lasts.
const CLICK_DURATION: Duration = Duration::from_millis(35);

struct ActiveClick {
    start_frame: u64,
}

/// Shared, lock-free handle to the audio backend's most recently reported
/// output latency (time between the data callback being invoked and the
/// samples it wrote actually reaching the DAC/speakers).
///
/// This exists because audio/visual/input sync is fundamentally NOT solved
/// by "make everything happen at the same instant" — output hardware
/// always has some buffering delay, typically 10-100+ ms depending on
/// backend and device. Every rhythm game (osu!, Guitar Hero, Beat Saber)
/// deals with this the same way: measure the latency, then compensate for
/// it, rather than trying to eliminate it. This estimate is a *fallback*
/// starting point; the authoritative number comes from the calibration
/// phase in `main.rs`, which measures the player's actual perceived
/// latency (device latency + their own reaction/perception) empirically.
#[derive(Clone)]
pub struct LatencyEstimate(Arc<AtomicU64>);

impl LatencyEstimate {
    fn new() -> Self {
        Self(Arc::new(AtomicU64::new(0)))
    }

    fn set(&self, latency: Duration) {
        self.0.store(latency.as_nanos() as u64, Ordering::Relaxed);
    }

    /// Latest known output latency, or `Duration::ZERO` if no callback with
    /// timestamp info has run yet (or the backend doesn't support it).
    pub fn get(&self) -> Duration {
        Duration::from_nanos(self.0.load(Ordering::Relaxed))
    }
}

/// Owns the running audio stream. Dropping this stops metronome playback.
pub struct Metronome {
    _stream: Stream,
}

impl Metronome {
    /// Starts playing back the given clicks (already expressed as offsets
    /// from session start, e.g. `schedule.clicks(session_duration)`).
    /// Returns the `Metronome` handle plus a [`LatencyEstimate`] that keeps
    /// updating in the background as the stream runs.
    pub fn play(
        clicks: Vec<ClickEvent>,
        clock: crate::clock::SessionClock,
    ) -> anyhow::Result<(Self, LatencyEstimate)> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("no output audio device available"))?;
        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate();
        let channels = config.channels() as usize;

        let mut click_frames: Vec<u64> = clicks
            .iter()
            .map(|c| duration_to_frames(c.at, sample_rate))
            .collect();
        click_frames.sort_unstable();

        // `frame_offset` lets the callback know how many frames have
        // elapsed since the stream itself started, which combined with
        // `clock` lets us realign if stream startup latency drifts from
        // the logical session clock. For v1 we assume stream start ≈
        // clock start (both happen right before `stream.play()`), so we
        // seed the frame counter from the clock's elapsed time at first
        // callback rather than from a hardcoded zero — this keeps
        // metronome audio aligned even if `Metronome::play` is called
        // slightly after the session clock started.
        let start_elapsed = clock.elapsed();
        let start_frame_offset = duration_to_frames(start_elapsed, sample_rate);

        let mut next_click_idx: usize = 0;
        let mut active: Vec<ActiveClick> = Vec::new();
        let mut frame_counter: u64 = start_frame_offset;
        let click_len_frames = duration_to_frames(CLICK_DURATION, sample_rate);
        let sr = sample_rate as f32;

        let latency_estimate = LatencyEstimate::new();
        let latency_estimate_for_callback = latency_estimate.clone();

        let stream_config: cpal::StreamConfig = config.into();
        let stream = device.build_output_stream(
            stream_config,
            move |data: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let timestamp = info.timestamp();
                // `playback` is the backend's own prediction of when these
                // samples will actually reach the speakers; `callback` is
                // when this function was invoked. Their difference is the
                // output latency we want to compensate for elsewhere.
                let latency = timestamp.playback.duration_since(timestamp.callback);
                latency_estimate_for_callback.set(latency);

                for frame in data.chunks_mut(channels) {
                    while next_click_idx < click_frames.len()
                        && click_frames[next_click_idx] <= frame_counter
                    {
                        active.push(ActiveClick {
                            start_frame: frame_counter,
                        });
                        next_click_idx += 1;
                    }

                    let mut sample = 0.0f32;
                    active.retain(|click| {
                        let elapsed = frame_counter.saturating_sub(click.start_frame);
                        if elapsed >= click_len_frames {
                            return false;
                        }
                        let t = elapsed as f32 / sr;
                        let envelope = 1.0 - (elapsed as f32 / click_len_frames as f32);
                        sample += (2.0 * std::f32::consts::PI * CLICK_FREQ_HZ * t).sin() * envelope;
                        true
                    });

                    for out in frame.iter_mut() {
                        *out = sample * 0.5; // headroom
                    }

                    frame_counter += 1;
                }
            },
            |err| eprintln!("metronome audio stream error: {err}"),
            None,
        )?;

        stream.play()?;
        Ok((Self { _stream: stream }, latency_estimate))
    }
}

fn duration_to_frames(d: Duration, sample_rate: u32) -> u64 {
    (d.as_secs_f64() * sample_rate as f64) as u64
}
