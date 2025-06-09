use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// PushBusIter is an experiment to use array based queues per thread, instead of a shared Linked List.
/// Most CPU time is used reading the RP1210 adapter, so the Bus isn't a significant contributer to CPU usage.

#[derive(Clone, Default)]
pub struct PushBus<T> {
    iters: Arc<Mutex<Vec<PushBusIter<T>>>>,
}
impl<T> PushBus<T> {
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

const SLEEP_DURATION: Duration = Duration::from_millis(1);
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
        thread::sleep(SLEEP_DURATION);
        Some(None)
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
        let mut iters = self.iters.lock().unwrap();
        // remove closed iterators.
        iters.retain(|i| i.running.load(std::sync::atomic::Ordering::Relaxed));
        iters.iter_mut().for_each(|i| {
            let mut items = i.data.lock().unwrap();
            let len = items.len();
            if len > 1000 {
                eprintln!("pushbus too deep: {len}");
            }
            items.push_back(item.clone())
        });
    }
}

impl<T> Drop for PushBus<T> {
    fn drop(&mut self) {
        self.close();
    }
}
impl<T> Drop for PushBusIter<T> {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}
