use std::{env, fs, sync::Arc, time::Duration};

use serde_json::json;
use tokio::time::{Instant, sleep, timeout};
use trueflow_lib::transcription::{
    AudioChunk, AudioEncoding, DeepgramProvider, SessionConfig, TranscriptAccumulator,
    TranscriptionEvent, TranscriptionProvider,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Arguments::parse()?;
    let api_key = env::var("TRUEFLOW_DEEPGRAM_API_KEY")
        .map_err(|_| "TRUEFLOW_DEEPGRAM_API_KEY is required")?;
    let pcm = read_pcm16(&args.path)?;
    let provider = DeepgramProvider::new(Some(api_key), Duration::from_secs(5));
    let started = Instant::now();
    let mut session = provider
        .start_session(SessionConfig {
            operation_id: format!("benchmark-{}", args.sample_id),
            sample_rate: args.sample_rate,
            channels: 1,
            encoding: AudioEncoding::Linear16,
            language: "en-US".to_owned(),
            model: "nova-3".to_owned(),
            vocabulary: Default::default(),
        })
        .await?;
    let connected = Instant::now();
    let mut transcript = TranscriptAccumulator::default();
    let mut first_partial_ms = None;
    let samples_per_chunk = usize::try_from(args.sample_rate / 50)?.max(1);

    for (sequence, samples) in pcm.chunks(samples_per_chunk).enumerate() {
        session
            .input
            .send_audio(AudioChunk {
                sequence: u64::try_from(sequence)?,
                samples: Arc::from(samples),
            })
            .await?;
        drain_ready_events(
            &mut session.events,
            &mut transcript,
            &mut first_partial_ms,
            started,
        )
        .await?;
        sleep(Duration::from_millis(20)).await;
    }

    let released = Instant::now();
    session.input.finish().await?;
    timeout(Duration::from_secs(8), async {
        while !transcript.is_operation_final() {
            let event = session.events.next_event().await?;
            apply_event(event, &mut transcript, &mut first_partial_ms, started)?;
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    })
    .await
    .map_err(|_| "benchmark finalization timed out")??;
    let finalized = Instant::now();
    session.input.close().await?;

    let final_text = transcript.best_known_text();
    let exact_match = args
        .expected
        .as_ref()
        .map(|expected| normalize(expected) == normalize(&final_text));
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "sample_id": args.sample_id,
            "provider": "deepgram",
            "model": "nova-3",
            "sample_rate": args.sample_rate,
            "audio_duration_ms": pcm.len().saturating_mul(1000) / usize::try_from(args.sample_rate)?,
            "connection_latency_ms": connected.duration_since(started).as_millis(),
            "time_to_first_partial_ms": first_partial_ms,
            "release_to_final_ms": finalized.duration_since(released).as_millis(),
            "total_elapsed_ms": finalized.duration_since(started).as_millis(),
            "exact_normalized_match": exact_match,
            "final_transcript": final_text,
            "cost": "record from the provider usage dashboard for this evaluation run"
        }))?
    );
    Ok(())
}

async fn drain_ready_events(
    events: &mut Box<dyn trueflow_lib::transcription::TranscriptionEvents>,
    transcript: &mut TranscriptAccumulator,
    first_partial_ms: &mut Option<u128>,
    started: Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    while let Ok(result) = timeout(Duration::from_millis(1), events.next_event()).await {
        apply_event(result?, transcript, first_partial_ms, started)?;
    }
    Ok(())
}

fn apply_event(
    event: TranscriptionEvent,
    transcript: &mut TranscriptAccumulator,
    first_partial_ms: &mut Option<u128>,
    started: Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    match event {
        TranscriptionEvent::PartialTranscript { text } => {
            first_partial_ms.get_or_insert(started.elapsed().as_millis());
            transcript.replace_partial(text);
        }
        TranscriptionEvent::FinalTranscript {
            text,
            operation_final,
        } => transcript.add_final_segment(text, operation_final),
        TranscriptionEvent::ProviderError(error) => return Err(Box::new(error)),
        TranscriptionEvent::SessionClosed if !transcript.is_operation_final() => {
            return Err("provider closed before final transcript".into());
        }
        TranscriptionEvent::SessionStarted { .. }
        | TranscriptionEvent::SpeechStarted
        | TranscriptionEvent::SpeechEnded
        | TranscriptionEvent::ProviderWarning { .. }
        | TranscriptionEvent::SessionClosed => {}
    }
    Ok(())
}

fn read_pcm16(path: &str) -> Result<Vec<i16>, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    if bytes.len() % 2 != 0 {
        return Err("PCM16 input must contain complete little-endian samples".into());
    }
    Ok(bytes
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect())
}

fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

struct Arguments {
    sample_id: String,
    path: String,
    sample_rate: u32,
    expected: Option<String>,
}

impl Arguments {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut values = env::args().skip(1);
        let mut sample_id = None;
        let mut path = None;
        let mut sample_rate = None;
        let mut expected = None;
        while let Some(flag) = values.next() {
            let value = values
                .next()
                .ok_or_else(|| format!("missing value for {flag}"))?;
            match flag.as_str() {
                "--sample-id" => sample_id = Some(value),
                "--path" => path = Some(value),
                "--sample-rate" => sample_rate = Some(value.parse()?),
                "--expected" => expected = Some(value),
                _ => return Err(format!("unknown argument: {flag}").into()),
            }
        }
        Ok(Self {
            sample_id: sample_id.ok_or("--sample-id is required")?,
            path: path.ok_or("--path is required")?,
            sample_rate: sample_rate.ok_or("--sample-rate is required")?,
            expected,
        })
    }
}
