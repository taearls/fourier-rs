//! Lock-free SPSC ring buffer for inter-thread audio communication.
//!
//! The audio callback thread (real-time) writes to one end; the processing
//! thread reads from the other. No mutexes, no allocations.

use ringbuf::{
    traits::{Consumer, Observer, Producer, Split},
    HeapRb,
};

/// A lock-free single-producer single-consumer audio ring buffer.
pub struct AudioRingBuffer;

/// Producer half — lives on the writing thread.
pub struct RingProducer {
    inner: ringbuf::HeapProd<f32>,
}

/// Consumer half — lives on the reading thread.
pub struct RingConsumer {
    inner: ringbuf::HeapCons<f32>,
}

impl AudioRingBuffer {
    /// Create a new ring buffer with the given capacity in samples.
    ///
    /// Returns (producer, consumer) halves that can be sent to different threads.
    /// Named `create` instead of `new` because it returns a tuple of halves
    /// rather than `Self`.
    pub fn create(capacity: usize) -> (RingProducer, RingConsumer) {
        let rb = HeapRb::<f32>::new(capacity);
        let (prod, cons) = rb.split();
        (RingProducer { inner: prod }, RingConsumer { inner: cons })
    }
}

impl RingProducer {
    /// Push samples into the ring buffer. Returns the number of samples
    /// actually written (may be less than `samples.len()` if buffer is full).
    pub fn push_slice(&mut self, samples: &[f32]) -> usize {
        self.inner.push_slice(samples)
    }

    /// Number of free slots available for writing.
    pub fn available(&self) -> usize {
        self.inner.vacant_len()
    }

    /// Check if the buffer is full.
    pub fn is_full(&self) -> bool {
        self.inner.is_full()
    }
}

impl RingConsumer {
    /// Pop samples from the ring buffer into `output`. Returns the number
    /// of samples actually read.
    pub fn pop_slice(&mut self, output: &mut [f32]) -> usize {
        self.inner.pop_slice(output)
    }

    /// Number of samples available for reading.
    pub fn available(&self) -> usize {
        self.inner.occupied_len()
    }

    /// Check if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

// SAFETY: `RingProducer` wraps `ringbuf::HeapProd<f32>`, which is
// `CachingProd<Arc<HeapRb<f32>>>`. The inner `Arc<HeapRb<f32>>` is `Send + Sync`
// (ringbuf's `Heap<T>` implements `Send` when `T: Send`, and `f32: Send`).
// The `Caching` wrapper uses only atomic operations for index synchronization
// and holds no thread-local or non-Send state. The ringbuf crate omits the
// blanket `Send` impl on `Caching` because it is generic over `R: RbRef`
// without a `Send` bound, but for our concrete type (`Arc<HeapRb<f32>>`),
// sending across threads is sound.
#[allow(unsafe_code)]
unsafe impl Send for RingProducer {}

// SAFETY: Same reasoning as `RingProducer` above — `RingConsumer` wraps
// `ringbuf::HeapCons<f32>` (`CachingCons<Arc<HeapRb<f32>>>`), which is
// safe to send across threads for the same reasons.
#[allow(unsafe_code)]
unsafe impl Send for RingConsumer {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_push_pop() {
        let (mut prod, mut cons) = AudioRingBuffer::create(1024);
        let input = vec![1.0, 2.0, 3.0, 4.0];
        assert_eq!(prod.push_slice(&input), 4);

        let mut output = vec![0.0; 4];
        assert_eq!(cons.pop_slice(&mut output), 4);
        assert_eq!(output, input);
    }

    #[test]
    fn overflow_returns_partial_count() {
        let (mut prod, _cons) = AudioRingBuffer::create(4);
        let input = vec![1.0; 8];
        let written = prod.push_slice(&input);
        assert!(written <= 4);
    }
}
