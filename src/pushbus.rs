use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// PushBusIter is an experiment to use array based queues per thread, instead of a shared Linked List.
/// Most CPU time is used reading the RP1210 adapter, so the Bus isn't a significant contributer to CPU usage.

pub struct PushBus<T> {
    iters: Arc<Mutex<Vec<PushBusIter<T>>>>,
    name: String,
}

impl<T: Clone> Clone for PushBus<T> {
    fn clone(&self) -> Self {
        Self {
            iters: self.iters.clone(),
            name: self.name.clone(),
        }
    }
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
    sleep: bool,
}
impl<T> PushBus<T> {
    pub fn new(name: &str) -> Self {
        Self {
            iters: Default::default(),
            name: name.to_string(),
        }
    }
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
            self.sleep = false;
            return v;
        }
        // this means there was an empty response from poll()
        // sleep to avoid busy spinning
        if self.sleep {
            thread::sleep(SLEEP_DURATION);
        }
        self.sleep = true;
        Some(None)
    }
}

impl<T: Send + Sync + 'static + Clone> PushBus<T> {
    pub fn iter(&self) -> Box<dyn Iterator<Item = Option<T>> + Send + Sync> {
        let x = PushBusIter {
            data: Arc::new(Mutex::new(VecDeque::new())),
            running: Arc::new(AtomicBool::new(true)),
            sleep: false,
        };
        self.iters.lock().unwrap().push(x.clone());
        Box::new(x)
    }

    pub fn push(&self, item: Option<T>) {
        let mut iters = self.iters.lock().unwrap();
        // remove closed iterators.
        iters.retain(|i| i.running.load(std::sync::atomic::Ordering::Relaxed));
        iters.iter_mut().for_each(|i| {
            let mut items = i.data.lock().unwrap();
            let name = self.name.as_str();
            let len = items.len();
            if len > 10_000 {
                eprintln!("{name} pushbus too deep: {len}");
            }
            items.push_back(item.clone())
        });
    }
}

// This breaks the usefullness of clone()
// impl<T> Drop for PushBus<T> {
//     fn drop(&mut self) {
//         self.close();
//     }
// }
impl<T> Drop for PushBusIter<T> {
    fn drop(&mut self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {

    use crate::pushbus::PushBus;
    #[test]
    fn test_clone() {
        let mut pb1 = PushBus::new("test");
        let pb2 = pb1.clone();

        let mut i1 = pb1.iter();
        let mut i2 = pb2.iter();
        pb1.push(Some(1));
        pb1.push(Some(2));
        assert_eq!(Some(1), i1.next().unwrap());
        assert_eq!(Some(1), i2.next().unwrap());
        assert_eq!(Some(2), i1.next().unwrap());
        assert_eq!(Some(2), i2.next().unwrap());
        assert_eq!(None, i1.next().unwrap());
        assert_eq!(None, i2.next().unwrap());

        let mut i1 = pb1.iter();
        let mut i2 = pb2.iter();
        assert_eq!(None, i1.next().unwrap());
        assert_eq!(None, i2.next().unwrap());
    }
}
