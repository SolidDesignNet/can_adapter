use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::thread;
use std::time::Duration;
use std::time::Instant;

/// represents the bus.  This is used by the adapter.  Currently is a custom multiqueue (multi headed linked list), but may use a publish subscribe sytem in the future.
pub(crate) trait Bus<T>: Send
where
    T: Clone,
{
    /// used to read packets from the bus for a duration (typically considered a response timeout).
    fn iter_for(&mut self, duration: Duration) -> Box<dyn Iterator<Item = T>>;
    fn push(&mut self, item: T);
    fn clone_bus(&self) -> Box<dyn Bus<T>>;
}

#[derive(Clone)]
pub(crate) struct MultiQueue<T: Clone> {
    // shared head that always points to the empty Arc<RwLock>
    // Yes, this seems like overkill, but we need to clone multiqueues to easily use them in threads, so this makes cloning work easily.
    head: Arc<RwLock<Arc<RwLock<Option<MqItem<T>>>>>>,
}

/// Iterator
struct MqIter<T> {
    head: Arc<RwLock<Option<MqItem<T>>>>,
    until: Instant,
}

/// Linked list data
struct MqItem<T> {
    data: T,
    next: Arc<RwLock<Option<MqItem<T>>>>,
}

impl<T> Iterator for MqIter<T>
where
    T: Clone + Sync + Send,
{
    type Item = T;
    fn next(&mut self) -> std::option::Option<<Self as std::iter::Iterator>::Item> {
        let mut o = None;
        while o.is_none() && Instant::now() < self.until {
            thread::sleep(Duration::from_millis(1));
            o = self
                .head
                .read()
                .unwrap()
                .as_ref()
                .map(|i| (i.data.clone(), i.next.clone()));
        }
        o.map(|clones| {
            self.head = clones.1;
            clones.0
        })
    }
}

#[allow(dead_code)]
impl<T> MultiQueue<T>
where
    T: Clone + Sync + Send,
{
    pub fn new() -> Box<dyn Bus<T>>
    where
        T: 'static + Clone + Sync + Send,
    {
        Box::new(MultiQueue {
            head: Arc::new(RwLock::new(Arc::new(RwLock::new(None)))),
        })
    }
}

impl<T> Bus<T> for MultiQueue<T>
where
    T: 'static + Clone + Sync + Send,
{
    fn iter_for(&mut self, duration: Duration) -> Box<dyn Iterator<Item = T>> {
        Box::new(MqIter {
            head: self.head.read().unwrap().clone(),
            until: Instant::now() + duration,
        })
    }

    fn push(&mut self, item: T) {
        let empty = Arc::new(RwLock::new(None));
        let mut head = self.head.write().unwrap();
        // add the new item.
        *head.write().unwrap() = Some(MqItem {
            data: item,
            next: empty.clone(),
        });
        // update head to point to the new empty item.
        *head = empty;
    }

    fn clone_bus(&self) -> Box<dyn Bus<T>> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let mut q = MultiQueue::new();
        q.push("one");
        let mut i = q.iter_for(Duration::from_millis(200));
        q.push("two");
        q.push("three");
        assert_eq!("two", i.next().unwrap());
        assert_eq!("three", i.next().unwrap());
        assert_eq!(std::option::Option::None, i.next());
    }
}

/// PushBusIter is an experiment to use array based queues per thread, instead of a shared Linked List.
/// Most CPU time is used reading the RP1210 adapter, so the Bus isn't a significant contributer to CPU usage.

#[derive(Clone)]
pub(crate) struct PushBus<T> {
    iters: Arc<Mutex<Vec<PushBusIter<T>>>>,
}
impl<T> PushBus<T> {
    pub fn new() -> Self {
        Self {
            iters: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[derive(Clone)]
struct PushBusIter<T> {
    data: Arc<Mutex<VecDeque<T>>>,
    iters: Arc<Mutex<Vec<PushBusIter<T>>>>,
    end: Instant,
}
impl<T> Drop for PushBusIter<T> {
    fn drop(&mut self) {
        self.end = Instant::now() - Duration::from_secs(1);
        self.iters.lock().unwrap().retain(|i| i.is_running());
    }
}
impl<T> Iterator for PushBusIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        while self.is_running() {
            let v = self.data.lock().unwrap().pop_front();
            if v.is_some() {
                return v;
            }
            thread::sleep(Duration::from_millis(1));
        }
        None
    }
}

impl<T> PushBusIter<T> {
    fn is_running(&self) -> bool {
        Instant::now() <= self.end
    }
}
impl<T: 'static + Send + Clone> Bus<T> for PushBus<T> {
    fn iter_for(&mut self, duration: Duration) -> Box<dyn Iterator<Item = T>> {
        let muti = PushBusIter {
            data: Arc::new(Mutex::new(VecDeque::new())),
            iters: self.iters.clone(),
            end: Instant::now() + duration,
        };
        self.iters.lock().unwrap().push(muti.clone());
        Box::new(muti)
    }

    fn push(&mut self, item: T) {
        self.iters
            .lock()
            .unwrap()
            .iter_mut()
            .for_each(|i| i.data.lock().unwrap().push_back(item.clone()));
    }

    fn clone_bus(&self) -> Box<dyn Bus<T>> {
        Box::new(self.clone())
    }
}

impl<T: Clone + 'static> Clone for Box<dyn Bus<T>> {
    fn clone(&self) -> Self {
        self.clone_bus()
    }
}
