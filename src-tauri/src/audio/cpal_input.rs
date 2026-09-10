use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use cpal::{
    Device, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use super::{
    AudioFormat, AudioInput, CaptureEventHandler, CaptureSummary, RecordingSession,
    buffer::BoundedAudioBuffer,
};
use crate::dictation::DictationError;

const BUFFER_SECONDS: usize = 30;
const LEVEL_INTERVAL: Duration = Duration::from_millis(33);

pub struct CpalAudioInput;

impl AudioInput for CpalAudioInput {
    fn start(
        &self,
        handler: Arc<dyn CaptureEventHandler>,
    ) -> Result<Box<dyn RecordingSession>, DictationError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(DictationError::NoInputDevice)?;
        let supported = device
            .default_input_config()
            .map_err(|error| DictationError::UnsupportedAudioConfiguration(error.to_string()))?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels();
        let sample_capacity = usize::try_from(sample_rate)
            .unwrap_or(usize::MAX)
            .saturating_mul(BUFFER_SECONDS);
        let buffer = Arc::new(Mutex::new(BoundedAudioBuffer::new(sample_capacity)));
        let signals = Arc::new(CaptureSignals::default());
        let config: StreamConfig = supported.into();

        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_stream::<f32>(
                &device,
                &config,
                channels,
                buffer.clone(),
                signals.clone(),
                handler.clone(),
            ),
            SampleFormat::I16 => build_stream::<i16>(
                &device,
                &config,
                channels,
                buffer.clone(),
                signals.clone(),
                handler.clone(),
            ),
            SampleFormat::U16 => build_stream::<u16>(
                &device,
                &config,
                channels,
                buffer.clone(),
                signals.clone(),
                handler.clone(),
            ),
            format => Err(DictationError::UnsupportedAudioConfiguration(
                format.to_string(),
            )),
        }?;

        let running = Arc::new(AtomicBool::new(true));
        let worker = spawn_event_worker(running.clone(), signals.clone(), handler)?;
        if let Err(error) = stream.play() {
            running.store(false, Ordering::Release);
            let _ = worker.join();
            return Err(DictationError::CaptureStartFailed(error.to_string()));
        }

        Ok(Box::new(CpalRecordingSession {
            stream: Some(stream),
            running,
            worker: Some(worker),
            buffer,
            signals: signals.clone(),
            sample_rate,
            channels,
        }))
    }
}

#[derive(Default)]
struct CaptureSignals {
    first_frame_seen: AtomicBool,
    first_frame_pending: AtomicBool,
    latest_level_bits: AtomicU32,
    stream_error: Mutex<Option<DictationError>>,
    local_buffer_dropped_samples: std::sync::atomic::AtomicU64,
}

fn build_stream<T>(
    device: &Device,
    config: &StreamConfig,
    channels: u16,
    buffer: Arc<Mutex<BoundedAudioBuffer>>,
    signals: Arc<CaptureSignals>,
    handler: Arc<dyn CaptureEventHandler>,
) -> Result<Stream, DictationError>
where
    T: SizedSample + Copy,
    f32: FromSample<T>,
{
    let data_signals = signals.clone();
    let error_signals = signals;
    device
        .build_input_stream(
            *config,
            move |data: &[T], _| {
                process_input(data, channels, &buffer, &data_signals, handler.as_ref())
            },
            move |error| {
                if let Ok(mut slot) = error_signals.stream_error.try_lock()
                    && slot.is_none()
                {
                    *slot = Some(DictationError::AudioDeviceLost(error.to_string()));
                }
            },
            None,
        )
        .map_err(|error| DictationError::CaptureStartFailed(error.to_string()))
}

fn process_input<T>(
    input: &[T],
    channels: u16,
    buffer: &Mutex<BoundedAudioBuffer>,
    signals: &CaptureSignals,
    handler: &dyn CaptureEventHandler,
) where
    T: Sample + Copy,
    f32: FromSample<T>,
{
    if input.is_empty() || channels == 0 {
        return;
    }

    let channels = usize::from(channels);
    let mut mono = Vec::with_capacity(input.len() / channels);
    let mut square_sum = 0.0_f32;
    for frame in input.chunks_exact(channels) {
        let sum = frame
            .iter()
            .fold(0.0_f32, |total, sample| total + f32::from_sample(*sample));
        let sample = (sum / channels as f32).clamp(-1.0, 1.0);
        square_sum += sample * sample;
        mono.push((sample * f32::from(i16::MAX)) as i16);
    }

    if mono.is_empty() {
        return;
    }

    let rms = (square_sum / mono.len() as f32).sqrt();
    let display_level = (rms * 4.0).clamp(0.0, 1.0);
    signals
        .latest_level_bits
        .store(display_level.to_bits(), Ordering::Relaxed);
    if !signals.first_frame_seen.swap(true, Ordering::AcqRel) {
        signals.first_frame_pending.store(true, Ordering::Release);
    }

    let mono: Arc<[i16]> = mono.into();
    if let Ok(mut buffer) = buffer.try_lock() {
        buffer.push(mono.clone());
    } else {
        signals.local_buffer_dropped_samples.fetch_add(
            u64::try_from(mono.len()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }
    if let Err(error) = handler.audio_chunk(mono)
        && let Ok(mut slot) = signals.stream_error.try_lock()
        && slot.is_none()
    {
        *slot = Some(error);
    }
}

fn spawn_event_worker(
    running: Arc<AtomicBool>,
    signals: Arc<CaptureSignals>,
    handler: Arc<dyn CaptureEventHandler>,
) -> Result<JoinHandle<()>, DictationError> {
    thread::Builder::new()
        .name("trueflow-audio-events".to_owned())
        .spawn(move || {
            while running.load(Ordering::Acquire) {
                if signals.first_frame_pending.swap(false, Ordering::AcqRel) {
                    handler.first_audio_frame();
                }

                if let Ok(mut error) = signals.stream_error.try_lock()
                    && let Some(error) = error.take()
                {
                    handler.capture_error(error);
                    break;
                }

                let level = f32::from_bits(signals.latest_level_bits.load(Ordering::Relaxed));
                handler.audio_level(level);
                thread::sleep(LEVEL_INTERVAL);
            }
        })
        .map_err(|error| DictationError::CaptureStartFailed(error.to_string()))
}

struct CpalRecordingSession {
    stream: Option<Stream>,
    running: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    buffer: Arc<Mutex<BoundedAudioBuffer>>,
    signals: Arc<CaptureSignals>,
    sample_rate: u32,
    channels: u16,
}

impl RecordingSession for CpalRecordingSession {
    fn format(&self) -> AudioFormat {
        AudioFormat {
            sample_rate: self.sample_rate,
            channels: 1,
        }
    }

    fn stop(mut self: Box<Self>) -> Result<CaptureSummary, DictationError> {
        self.stream.take();
        self.running.store(false, Ordering::Release);
        if self
            .worker
            .take()
            .is_some_and(|worker| worker.join().is_err())
        {
            return Err(DictationError::CaptureCleanupFailed(
                "audio event worker panicked".to_owned(),
            ));
        }

        let buffer = self
            .buffer
            .lock()
            .map_err(|_| DictationError::CaptureCleanupFailed("audio buffer lock".to_owned()))?;
        Ok(CaptureSummary {
            buffered_samples: buffer.sample_count(),
            dropped_samples: buffer.dropped_samples().saturating_add(
                self.signals
                    .local_buffer_dropped_samples
                    .load(Ordering::Relaxed),
            ),
            sample_rate: self.sample_rate,
            channels: self.channels,
        })
    }
}
