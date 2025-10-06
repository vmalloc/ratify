use std::sync::Arc;
use std::thread::JoinHandle;

pub(crate) fn for_each<I, F, R, T>(iterator: I, handler: F) -> ResultsIterator<R>
where
    I: Iterator<Item = T> + Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> R + Send + Clone + 'static,
    T: Send + 'static,
{
    for_each_with_discovery_callback(iterator, handler, None)
}

pub(crate) fn for_each_with_discovery_callback<I, F, R, T>(
    iterator: I,
    handler: F,
    discovery_callback: Option<Box<dyn Fn() + Send + Sync>>,
) -> ResultsIterator<R>
where
    I: Iterator<Item = T> + Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> R + Send + Clone + 'static,
    T: Send + 'static,
{
    let (entries_sender, entries_receiver) = crossbeam_channel::unbounded();
    let (results_sender, results_receiver) = crossbeam_channel::unbounded();
    let (total_sender, total_receiver) = crossbeam_channel::bounded(1);

    let discovery_callback = discovery_callback.map(Arc::new);

    let producer = std::thread::spawn(move || {
        let mut size = 0;
        for entry in iterator {
            size += 1;
            if let Some(ref callback) = discovery_callback {
                callback();
            }
            if entries_sender.send(entry).is_err() {
                log::debug!("Entries sender channel closed. Closing producer");
                break;
            }
        }
        let _ = total_sender.send(size);
        size
    });

    for _ in 0..(num_cpus::get() * 2) {
        let receiver = entries_receiver.clone();
        let sender = results_sender.clone();
        let handler = handler.clone();
        std::thread::spawn(move || {
            for entry in receiver {
                if sender.send(handler(entry)).is_err() {
                    break;
                }
            }
        });
    }
    drop(results_sender);

    ResultsIterator::new(producer, results_receiver, total_receiver)
}

pub struct ResultsIterator<R> {
    total: Option<usize>,
    receiver: crossbeam_channel::Receiver<R>,
    total_receiver: crossbeam_channel::Receiver<usize>,
    received: usize,
    on_total_discovered: Option<Box<dyn FnOnce(usize) + Send>>,
}
impl<R> ResultsIterator<R> {
    fn new(
        _join_handle: JoinHandle<usize>,
        receiver: crossbeam_channel::Receiver<R>,
        total_receiver: crossbeam_channel::Receiver<usize>,
    ) -> Self {
        Self {
            total: None,
            receiver,
            total_receiver,
            received: 0,
            on_total_discovered: None,
        }
    }

    pub fn with_total_callback<F>(mut self, callback: F) -> Self
    where
        F: FnOnce(usize) + Send + 'static,
    {
        self.on_total_discovered = Some(Box::new(callback));
        self
    }
}

impl<R> Iterator for ResultsIterator<R> {
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if we received the total count
        if self.total.is_none() {
            if let Ok(total) = self.total_receiver.try_recv() {
                self.total.replace(total);
                if let Some(callback) = self.on_total_discovered.take() {
                    callback(total);
                }
            }
        }

        if let Some(total) = self.total {
            if self.received >= total {
                return None;
            }
        }

        // Get the next result
        match self.receiver.recv().ok() {
            Some(item) => {
                self.received += 1;
                // Check again for total after receiving an item
                if self.total.is_none() {
                    if let Ok(total) = self.total_receiver.try_recv() {
                        self.total.replace(total);
                        if let Some(callback) = self.on_total_discovered.take() {
                            callback(total);
                        }
                    }
                }
                Some(item)
            }
            None => {
                // No more results, but check one last time for total
                if self.total.is_none() {
                    if let Ok(total) = self.total_receiver.try_recv() {
                        self.total.replace(total);
                        if let Some(callback) = self.on_total_discovered.take() {
                            callback(total);
                        }
                    }
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use pretty_assertions::assert_eq;

    #[test]
    fn test_for_each() {
        const N: usize = 1000;
        let iter = 0..N;

        let res = super::for_each(iter, |num| num * 2).collect::<HashSet<_>>();

        assert_eq!(res, (0..N).map(|num| num * 2).collect());
    }
}
