use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// PushBusIter is an experiment to use array based queues per thread, instead of a shared Linked List.
/// Most CPU time is used reading the RP1210 adapter, so the Bus isn't a significant contributer to CPU usage.

#[derive(Clone)]
pub struct PushBus<T> {
    iters: Arc<Mutex<Vec<PushBusIter<T>>>>,
}
impl<T> PushBus<T> {
    pub fn new() -> Self {
        Self {
            iters: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn close(&mut self) {
        self.iters
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|i| i.running.store(false, std::sync::atomic::Ordering::Relaxed));
    }
}

#[derive(Clone)]
pub struct PushBusIter<T> {
    data: Arc<Mutex<VecDeque<Option<T>>>>,
    running: Arc<AtomicBool>,
}

impl<T> Iterator for PushBusIter<T> {
    /// That's right, `Option<Option<Packet>>`
    /// None is closed
    /// Some(None) is an empty poll() of the adapter
    /// Some(Packet) is a CAN packet
    type Item = Option<T>;
    fn next(&mut self) -> Option<Self::Item> {
        if !self.running.load(std::sync::atomic::Ordering::Relaxed) {
            // done
            return None;
        }
        let v = self.data.lock().unwrap().pop_front();
        if v.is_some() {
            return v;
        }
        // this means there was an empty response from poll()
        // sleep to avoid busy spinning
        thread::sleep(Duration::from_millis(1));
        return Some(None);
    }
}

impl<T: Send + Sync + 'static + Clone> PushBus<T> {
    pub fn iter(&self) -> Box<dyn Iterator<Item = Option<T>> + Send + Sync> {
        let x = PushBusIter {
            data: Arc::new(Mutex::new(VecDeque::new())),
            running: Arc::new(AtomicBool::new(true)),
        };
        self.iters.lock().unwrap().push(x.clone());
        Box::new(x)
    }

    pub fn push(&mut self, item: Option<T>) {
        self.iters
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|i| i.data.lock().unwrap().push_back(item.clone()));
    }
}

impl<T> Drop for PushBus<T> {
    fn drop(&mut self) {
        self.close();
    }
}
