use std::collections::VecDeque;
use std::sync::Mutex;

use tokio::sync::Notify;

/// A bounded queue between a fast producer and a slow consumer that only cares about the newest:
/// pushing onto a full queue drops the oldest item.
pub struct FrameQueue<T> {
    state: Mutex<State<T>>,
    wake: Notify,
    capacity: usize,
}

struct State<T> {
    items: VecDeque<T>,
    closed: bool,
}

impl<T> FrameQueue<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            state: Mutex::new(State {
                items: VecDeque::new(),
                closed: false,
            }),
            wake: Notify::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn push(&self, item: T) {
        {
            let mut state = self.state.lock().expect("queue lock");
            if state.items.len() >= self.capacity {
                state.items.pop_front();
            }
            state.items.push_back(item);
        }
        self.wake.notify_one();
    }

    /// No more items will come; `pop` ends once the queue is drained.
    pub fn close(&self) {
        self.state.lock().expect("queue lock").closed = true;
        self.wake.notify_one();
    }

    /// The oldest item still there, waiting for one; `None` when closed and drained.
    pub async fn pop(&self) -> Option<T> {
        loop {
            {
                let mut state = self.state.lock().expect("queue lock");
                if let Some(item) = state.items.pop_front() {
                    return Some(item);
                }
                if state.closed {
                    return None;
                }
            }
            self.wake.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_full_queue_drops_the_oldest() {
        let queue = FrameQueue::new(2);
        for n in 1..=5 {
            queue.push(n);
        }
        queue.close();
        assert_eq!(queue.pop().await, Some(4));
        assert_eq!(queue.pop().await, Some(5));
        assert_eq!(queue.pop().await, None);
    }
}
