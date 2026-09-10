# STT evaluation corpus

`corpus.csv` defines a privacy-safe 50-sample recording plan. Record each prompt
locally as mono little-endian PCM16 under `benchmarks/stt/audio/`; that directory
is ignored and raw recordings must not be committed.

Run one sample with the same sample rate used to record it:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --example stt_benchmark -- \
  --sample-id short-01 \
  --path benchmarks/stt/audio/short-01.pcm \
  --sample-rate 16000 \
  --expected "Hello, this is a test of TrueFlow."
```

The harness streams 20 ms chunks in real time and prints connection latency,
time to first partial, release-to-final latency, total elapsed time, the final
transcript, and an exact normalized-match indicator. Save evaluation output
outside production analytics. Record provider-dashboard cost metadata manually.
The harness is an evaluation tool, not evidence that the unrecorded corpus has
been executed.
