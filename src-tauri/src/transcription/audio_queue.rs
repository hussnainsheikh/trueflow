use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use tokio::sync::Notify;

use super::AudioChunk;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioQueueError {
    Closed,
    CapacityExceeded { rejected_samples: usize },
}

#[derive(Debug)]
struct QueueState {
    chunks: VecDeque<AudioChunk>,
    queued_samples: usize,
    sample_capacity: usize,
    next_sequence: u64,
    rejected_samples: u64,
    closed: bool,
}

#[derive(Debug)]
pub struct BoundedAudioQueue {
    state: Mutex<QueueState>,
    available: Notify,
}

impl BoundedAudioQueue {
    pub fn new(sample_capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(QueueState {
                chunks: VecDeque::new(),
                queued_samples: 0,
                sample_capacity,
                next_sequence: 0,
                rejected_samples: 0,
                closed: false,
            }),
            available: Notify::new(),
        })
    }

    pub fn set_sample_capacity(&self, sample_capacity: usize) {
        if let Ok(mut state) = self.state.lock() {
            state.sample_capacity = sample_capacity;
        }
    }

    pub fn try_push(&self, samples: Arc<[i16]>) -> Result<u64, AudioQueueError> {
        let mut state = self.state.lock().map_err(|_| AudioQueueError::Closed)?;
        if state.closed {
            return Err(AudioQueueError::Closed);
        }
        if samples.len() > state.sample_capacity.saturating_sub(state.queued_samples) {
            state.rejected_samples = state
                .rejected_samples
                .saturating_add(u64::try_from(samples.len()).unwrap_or(u64::MAX));
            return Err(AudioQueueError::CapacityExceeded {
                rejected_samples: samples.len(),
            });
        }

        let sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.saturating_add(1);
        state.queued_samples = state.queued_samples.saturating_add(samples.len());
        state.chunks.push_back(AudioChunk { sequence, samples });
        drop(state);
        self.available.notify_one();
        Ok(sequence)
    }

    pub async fn pop(&self) -> Option<AudioChunk> {
        loop {
            let notified = self.available.notified();
            if let Ok(mut state) = self.state.lock() {
                if let Some(chunk) = state.chunks.pop_front() {
                    state.queued_samples = state.queued_samples.saturating_sub(chunk.samples.len());
                    return Some(chunk);
                }
                if state.closed {
                    return None;
                }
            } else {
                return None;
            }
            notified.await;
        }
    }

    pub fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.closed = true;
        }
        self.available.notify_waiters();
    }

    pub fn clear(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.chunks.clear();
            state.queued_samples = 0;
            state.closed = true;
        }
        self.available.notify_waiters();
    }

    #[cfg(test)]
    pub fn metrics(&self) -> (usize, u64) {
        self.state
            .lock()
            .map(|state| (state.queued_samples, state.rejected_samples))
            .unwrap_or((0, 0))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{AudioQueueError, BoundedAudioQueue};

    #[tokio::test]
    async fn preserves_chunk_order_and_sequence() {
        let queue = BoundedAudioQueue::new(8);
        queue.try_push(Arc::from([1_i16, 2])).expect("first chunk");
        queue.try_push(Arc::from([3_i16])).expect("second chunk");

        let first = queue.pop().await.expect("first queued chunk");
        let second = queue.pop().await.expect("second queued chunk");
        assert_eq!(first.sequence, 0);
        assert_eq!(&*first.samples, &[1, 2]);
        assert_eq!(second.sequence, 1);
        assert_eq!(&*second.samples, &[3]);
    }

    #[test]
    fn rejects_new_audio_instead_of_evicting_speech() {
        let queue = BoundedAudioQueue::new(3);
        queue.try_push(Arc::from([1_i16, 2])).expect("first chunk");

        assert_eq!(
            queue.try_push(Arc::from([3_i16, 4])),
            Err(AudioQueueError::CapacityExceeded {
                rejected_samples: 2
            })
        );
        assert_eq!(queue.metrics(), (2, 2));
    }

    #[tokio::test]
    async fn close_drains_existing_audio_then_ends() {
        let queue = BoundedAudioQueue::new(3);
        queue.try_push(Arc::from([1_i16])).expect("audio");
        queue.close();

        assert!(queue.pop().await.is_some());
        assert!(queue.pop().await.is_none());
        assert_eq!(
            queue.try_push(Arc::from([2_i16])),
            Err(AudioQueueError::Closed)
        );
    }
}
