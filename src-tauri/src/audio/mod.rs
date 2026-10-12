mod buffer;
mod cpal_input;
mod selection;

use std::sync::Arc;

use serde::Serialize;

pub use cpal_input::{CpalAudioInput, list_input_devices};
pub use selection::{InputDeviceInfo, MicrophoneOptions, MicrophoneSelection, valid_device_field};

use crate::dictation::DictationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MicrophonePermissionState {
    NotDetermined,
    Granted,
    Denied,
    Restricted,
    Unavailable,
}

pub trait MicrophonePermission: Send + Sync {
    fn status(&self) -> MicrophonePermissionState;
    fn request(&self) -> Result<MicrophonePermissionState, DictationError>;
}

pub trait CaptureEventHandler: Send + Sync {
    fn first_audio_frame(&self);
    fn audio_chunk(&self, samples: Arc<[i16]>) -> Result<(), DictationError>;
    fn audio_level(&self, level: f32);
    fn capture_error(&self, error: DictationError);
}

pub trait AudioInput: Send + Sync {
    fn start(
        &self,
        handler: Arc<dyn CaptureEventHandler>,
    ) -> Result<Box<dyn RecordingSession>, DictationError>;
}

pub trait RecordingSession: Send {
    fn format(&self) -> AudioFormat;
    fn stop(self: Box<Self>) -> Result<CaptureSummary, DictationError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureSummary {
    pub buffered_samples: usize,
    pub dropped_samples: u64,
    pub sample_rate: u32,
    pub channels: u16,
}
