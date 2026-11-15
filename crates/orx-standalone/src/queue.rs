//! A bounded single-producer single-consumer queue of `Copy` values:
//! wait-free on both sides, no allocation after creation. The control
//! thread pushes, the audio thread pops.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Ring<T> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>]>,
    /// Next slot to read; written by the consumer only
    head: AtomicUsize,
    /// Next slot to write; written by the producer only
    tail: AtomicUsize,
}

// SAFETY: a slot is written by the producer before `tail` is published
// (Release) and read by the consumer after it observes `tail` (Acquire),
// and the producer reuses a slot only after observing `head` past it, so
// no slot is accessed by both sides at once. T: Send crosses threads.
unsafe impl<T: Send> Sync for Ring<T> {}

pub struct Producer<T> {
    ring: Arc<Ring<T>>,
}

pub struct Consumer<T> {
    ring: Arc<Ring<T>>,
}

/// A queue holding up to `capacity` values
pub fn queue<T: Copy + Send>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    let slots = (0..capacity + 1).map(|_| UnsafeCell::new(MaybeUninit::uninit())).collect();
    let ring = Arc::new(Ring { slots, head: AtomicUsize::new(0), tail: AtomicUsize::new(0) });
    (Producer { ring: ring.clone() }, Consumer { ring })
}

impl<T: Copy> Producer<T> {
    /// Append `value`; false (and nothing queued) when full
    pub fn push(&mut self, value: T) -> bool {
        let r = &*self.ring;
        let tail = r.tail.load(Ordering::Relaxed);
        let next = (tail + 1) % r.slots.len();
        if next == r.head.load(Ordering::Acquire) {
            return false;
        }
        // SAFETY: slot `tail` is not readable by the consumer until `tail`
        // moves past it below, and it finished reading it (head moved past)
        unsafe { (*r.slots[tail].get()).write(value) };
        r.tail.store(next, Ordering::Release);
        true
    }
}

impl<T: Copy> Consumer<T> {
    /// The oldest value, if any
    pub fn pop(&mut self) -> Option<T> {
        let r = &*self.ring;
        let head = r.head.load(Ordering::Relaxed);
        if head == r.tail.load(Ordering::Acquire) {
            return None;
        }
        // SAFETY: the producer wrote slot `head` before publishing a tail
        // past it (Acquire above), and will not rewrite it until head moves
        let value = unsafe { (*r.slots[head].get()).assume_init() };
        r.head.store((head + 1) % r.slots.len(), Ordering::Release);
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_and_drains_in_order() {
        let (mut p, mut c) = queue::<u32>(3);
        assert!(p.push(1) && p.push(2) && p.push(3));
        assert!(!p.push(4));
        assert_eq!(c.pop(), Some(1));
        assert!(p.push(4));
        assert_eq!([c.pop(), c.pop(), c.pop(), c.pop()], [Some(2), Some(3), Some(4), None]);
    }

    #[test]
    fn crosses_threads_without_loss() {
        let (mut p, mut c) = queue::<u64>(64);
        let n = 200_000u64;
        let producer = std::thread::spawn(move || {
            for i in 0..n {
                while !p.push(i) {
                    std::hint::spin_loop();
                }
            }
        });
        let mut expected = 0;
        while expected < n {
            if let Some(v) = c.pop() {
                assert_eq!(v, expected);
                expected += 1;
            }
        }
        producer.join().map_err(|_| "producer panicked").unwrap_or_else(|e| panic!("{e}"));
    }
}
