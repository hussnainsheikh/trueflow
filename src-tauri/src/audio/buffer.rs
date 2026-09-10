use std::{collections::VecDeque, sync::Arc};

#[derive(Debug)]
pub struct BoundedAudioBuffer {
    chunks: VecDeque<Arc<[i16]>>,
    sample_capacity: usize,
    sample_count: usize,
    dropped_samples: u64,
}

impl BoundedAudioBuffer {
    pub fn new(sample_capacity: usize) -> Self {
        Self {
            chunks: VecDeque::new(),
            sample_capacity,
            sample_count: 0,
            dropped_samples: 0,
        }
    }

    pub fn push(&mut self, samples: Arc<[i16]>) {
        if self.sample_capacity == 0 {
            self.dropped_samples = self
                .dropped_samples
                .saturating_add(u64::try_from(samples.len()).unwrap_or(u64::MAX));
            return;
        }

        let samples = if samples.len() > self.sample_capacity {
            let remove = samples.len() - self.sample_capacity;
            self.dropped_samples = self
                .dropped_samples
                .saturating_add(u64::try_from(remove).unwrap_or(u64::MAX));
            Arc::from(&samples[remove..])
        } else {
            samples
        };

        while self.sample_count.saturating_add(samples.len()) > self.sample_capacity {
            if let Some(removed) = self.chunks.pop_front() {
                self.sample_count = self.sample_count.saturating_sub(removed.len());
                self.dropped_samples = self
                    .dropped_samples
                    .saturating_add(u64::try_from(removed.len()).unwrap_or(u64::MAX));
            } else {
                break;
            }
        }

        self.sample_count = self.sample_count.saturating_add(samples.len());
        self.chunks.push_back(samples);
    }

    pub const fn sample_count(&self) -> usize {
        self.sample_count
    }

    pub const fn dropped_samples(&self) -> u64 {
        self.dropped_samples
    }

    #[cfg(test)]
    fn clear(&mut self) {
        self.chunks.clear();
        self.sample_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::BoundedAudioBuffer;

    #[test]
    fn drops_oldest_complete_chunks_on_overflow() {
        let mut buffer = BoundedAudioBuffer::new(5);
        buffer.push(Arc::from([1, 2, 3]));
        buffer.push(Arc::from([4, 5, 6]));

        assert_eq!(buffer.sample_count(), 3);
        assert_eq!(buffer.dropped_samples(), 3);
        assert_eq!(
            buffer.chunks.front().map(AsRef::as_ref),
            Some(&[4, 5, 6][..])
        );
    }

    #[test]
    fn oversized_chunk_keeps_only_most_recent_samples() {
        let mut buffer = BoundedAudioBuffer::new(3);
        buffer.push(Arc::from([1, 2, 3, 4, 5]));

        assert_eq!(buffer.sample_count(), 3);
        assert_eq!(buffer.dropped_samples(), 2);
        assert_eq!(
            buffer.chunks.front().map(AsRef::as_ref),
            Some(&[3, 4, 5][..])
        );
    }

    #[test]
    fn clear_releases_all_buffered_audio() {
        let mut buffer = BoundedAudioBuffer::new(10);
        buffer.push(Arc::from([1, 2, 3]));
        buffer.clear();

        assert_eq!(buffer.sample_count(), 0);
        assert!(buffer.chunks.is_empty());
    }
}
