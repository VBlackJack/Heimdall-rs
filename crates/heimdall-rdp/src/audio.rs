/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! The server's sound played on this computer, as the C# Heimdall's "Local playback".
//!
//! One format is offered, PCM at 44.1 kHz, 16 bits, stereo, which every Windows server
//! offers: `ironrdp-rdpsnd` sends the formats both sides share in an order of its own, and
//! the server names the one it sends by its place in that list; with one, it is always 0.
//! Servers speaking the audio channel before its version 8 (xrdp) send waves `ironrdp-rdpsnd`
//! does not read yet: they stay silent.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use ironrdp::rdpsnd::client::RdpsndClientHandler;
use ironrdp::rdpsnd::pdu::{AudioFormat, PitchPdu, VolumePdu, WaveFormat};

/// Channels of the format offered.
pub const CHANNELS: u16 = 2;
/// Samples per second, per channel, of the format offered.
pub const SAMPLE_RATE: u32 = 44_100;
/// Bits per sample of the format offered.
const BITS_PER_SAMPLE: u16 = 16;
/// Bytes per sample.
const BYTES_PER_SAMPLE: u16 = BITS_PER_SAMPLE / 8;
/// The loudest volume the server can ask, per channel.
const FULL_VOLUME: f32 = 65_535.0;

/// Where the samples go: the sound card, or a test's record.
pub trait AudioSink: Send + std::fmt::Debug {
    /// Plays `samples`, interleaved left and right, after those given before.
    fn play(&mut self, samples: &[i16]);
    /// The volume of each channel, from 0 (silent) to 1 (as sent).
    fn set_volume(&mut self, left: f32, right: f32);
    /// The server closed the sound: what is waiting is dropped.
    fn stop(&mut self);
}

/// The format offered to the server.
#[must_use]
pub fn offered_format() -> AudioFormat {
    let block_align = CHANNELS * BYTES_PER_SAMPLE;
    AudioFormat {
        format: WaveFormat::PCM,
        n_channels: CHANNELS,
        n_samples_per_sec: SAMPLE_RATE,
        n_avg_bytes_per_sec: SAMPLE_RATE * u32::from(block_align),
        n_block_align: block_align,
        bits_per_sample: BITS_PER_SAMPLE,
        data: None,
    }
}

/// The audio channel's end on this computer: the waves the server sends, played by a sink.
#[derive(Debug)]
pub struct SoundBackend<S> {
    sink: S,
    formats: [AudioFormat; 1],
}

impl<S: AudioSink> SoundBackend<S> {
    /// Plays through `sink`.
    #[must_use]
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            formats: [offered_format()],
        }
    }
}

impl<S: AudioSink> RdpsndClientHandler for SoundBackend<S> {
    fn get_formats(&self) -> &[AudioFormat] {
        &self.formats
    }

    fn wave(&mut self, format_no: usize, _ts: u32, data: Cow<'_, [u8]>) {
        // Only the one format offered is played; another number is a server's mistake.
        if format_no != 0 {
            return;
        }
        let samples: Vec<i16> = data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|bytes| i16::from_le_bytes(*bytes))
            .collect();
        self.sink.play(&samples);
    }

    fn set_volume(&mut self, volume: VolumePdu) {
        self.sink.set_volume(
            f32::from(volume.volume_left) / FULL_VOLUME,
            f32::from(volume.volume_right) / FULL_VOLUME,
        );
    }

    fn set_pitch(&mut self, _pitch: PitchPdu) {
        // Windows servers always send the normal pitch.
    }

    fn close(&mut self) {
        self.sink.stop();
    }
}

/// Samples waiting to be played, and the volume to play them at: what the sound card's
/// thread takes from.
#[derive(Debug, Default)]
pub struct Waiting {
    samples: VecDeque<i16>,
    volume: (f32, f32),
}

/// At most this many samples wait, two seconds: a sound card too slow drops the oldest
/// rather than the memory growing.
const MOST_WAITING: usize = SAMPLE_RATE as usize * CHANNELS as usize * 2;

/// A sink that queues the samples for the sound card's thread to take.
#[derive(Debug, Clone, Default)]
pub struct QueueSink {
    waiting: Arc<Mutex<Waiting>>,
}

impl QueueSink {
    /// An empty queue, at full volume.
    #[must_use]
    pub fn new() -> Self {
        let sink = Self::default();
        sink.lock().volume = (1.0, 1.0);
        sink
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Waiting> {
        self.waiting.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Fills `out`, interleaved as it waits, with what waits at the volume set; silence
    /// where nothing does.
    pub fn take(&self, out: &mut [f32]) {
        let mut waiting = self.lock();
        let (left, right) = waiting.volume;
        for (index, slot) in out.iter_mut().enumerate() {
            let volume = if index % usize::from(CHANNELS) == 0 {
                left
            } else {
                right
            };
            *slot = waiting.samples.pop_front().map_or(0.0, |sample| {
                f32::from(sample) / f32::from(i16::MAX) * volume
            });
        }
    }
}

impl AudioSink for QueueSink {
    fn play(&mut self, samples: &[i16]) {
        let mut waiting = self.lock();
        waiting.samples.extend(samples);
        let over = waiting.samples.len().saturating_sub(MOST_WAITING);
        // Whole frames only: dropping one sample alone would swap left and right.
        let over = over + over % usize::from(CHANNELS);
        let over = over.min(waiting.samples.len());
        waiting.samples.drain(..over);
    }

    fn set_volume(&mut self, left: f32, right: f32) {
        self.lock().volume = (left.clamp(0.0, 1.0), right.clamp(0.0, 1.0));
    }

    fn stop(&mut self) {
        self.lock().samples.clear();
    }
}

/// The server's sound played on the default output of this computer, from a thread of its
/// own that holds the sound card's stream. `None` when there is no sound card to play on:
/// the session goes on silent.
#[must_use]
pub fn local_speakers() -> Option<SoundBackend<QueueSink>> {
    let sink = QueueSink::new();
    let feed = sink.clone();
    let (started, opened) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("rdp-sound".to_owned())
        .spawn(move || speakers(&feed, &started))
        .ok()?;
    opened.recv().ok()?.then(|| SoundBackend::new(sink))
}

/// Opens the default output at the format offered and plays `feed` until it is the last
/// holder of the queue; says on `started` whether it could open.
fn speakers(feed: &QueueSink, started: &std::sync::mpsc::Sender<bool>) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let stream = host.default_output_device().and_then(|device| {
        let config = cpal::StreamConfig {
            channels: CHANNELS,
            sample_rate: SAMPLE_RATE,
            buffer_size: cpal::BufferSize::Default,
        };
        let data = feed.clone();
        device
            .build_output_stream(
                &config,
                move |out: &mut [f32], _| data.take(out),
                |error| log::warn!("sound output failed: {error}"),
                None,
            )
            .ok()
    });
    let Some(stream) = stream.filter(|stream| stream.play().is_ok()) else {
        let _ = started.send(false);
        return;
    };
    let _ = started.send(true);
    // Played until the session drops its end of the queue.
    while Arc::strong_count(&feed.waiting) > 2 {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    drop(stream);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a test's sink was given.
    #[derive(Debug, Default)]
    struct Record {
        played: Vec<i16>,
        volume: Option<(f32, f32)>,
        stopped: bool,
    }

    impl AudioSink for Record {
        fn play(&mut self, samples: &[i16]) {
            self.played.extend(samples);
        }
        fn set_volume(&mut self, left: f32, right: f32) {
            self.volume = Some((left, right));
        }
        fn stop(&mut self) {
            self.stopped = true;
        }
    }

    #[test]
    fn one_format_is_offered_pcm_44_1_khz_16_bits_stereo() {
        let backend = SoundBackend::new(Record::default());
        let [format] = backend.get_formats() else {
            panic!("one format");
        };
        assert_eq!(format.format, WaveFormat::PCM);
        assert_eq!(
            (
                format.n_channels,
                format.n_samples_per_sec,
                format.bits_per_sample,
                format.n_block_align,
                format.n_avg_bytes_per_sec
            ),
            (2, 44_100, 16, 4, 176_400)
        );
    }

    #[test]
    fn a_wave_is_played_as_little_endian_samples_and_another_format_not_at_all() {
        let mut backend = SoundBackend::new(Record::default());
        backend.wave(0, 0, Cow::Borrowed(&[0x01, 0x00, 0xFF, 0x7F, 0x00, 0x80]));
        backend.wave(1, 0, Cow::Borrowed(&[0x05, 0x00]));
        assert_eq!(backend.sink.played, [1, i16::MAX, i16::MIN]);
    }

    #[test]
    fn the_volume_asked_is_a_share_of_the_loudest_and_close_stops() {
        let mut backend = SoundBackend::new(Record::default());
        backend.set_volume(VolumePdu {
            volume_left: 0xFFFF,
            volume_right: 0,
        });
        assert_eq!(backend.sink.volume, Some((1.0, 0.0)));
        backend.close();
        assert!(backend.sink.stopped);
    }

    #[test]
    #[expect(clippy::float_cmp, reason = "halves of exact values are exact")]
    fn the_queue_plays_in_order_at_the_volume_set_then_silence() {
        let mut sink = QueueSink::new();
        sink.play(&[i16::MAX, i16::MAX, 0, i16::MAX]);
        sink.set_volume(0.5, 1.0);
        let mut out = [9.0_f32; 6];
        sink.take(&mut out);
        assert_eq!(out, [0.5, 1.0, 0.0, 1.0, 0.0, 0.0]);
        sink.play(&[1, 2]);
        sink.stop();
        let mut out = [9.0_f32; 2];
        sink.take(&mut out);
        assert_eq!(out, [0.0, 0.0], "dropped when the server closes");
    }

    #[test]
    fn a_queue_too_long_drops_its_oldest_whole_frames() {
        let mut sink = QueueSink::new();
        let too_many: Vec<i16> = (0..MOST_WAITING + 3)
            .map(|index| i16::try_from(index % 2).expect("0 or 1"))
            .collect();
        sink.play(&too_many);
        let waiting = sink.lock();
        // Three over, rounded up to two frames: four dropped, a left sample first.
        assert_eq!(waiting.samples.len(), MOST_WAITING - 1);
        assert_eq!(waiting.samples.front(), Some(&0), "a left sample first");
    }
}
