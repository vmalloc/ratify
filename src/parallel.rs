use std::thread::JoinHandle;

pub(crate) fn for_each<I, F, R, T>(iterator: I, handler: F) -> impl Iterator<Item = R>
where
    I: Iterator<Item = T> + Send + 'static,
    R: Send + 'static,
    F: Fn(T) -> R + Send + Clone + 'static,
    T: Send + 'static,
{
    let (entries_sender, entries_receiver) = crossbeam_channel::bounded(num_cpus::get() * 4);

    let (results_sender, results_receiver) = crossbeam_channel::unbounded();

    let producer = std::thread::spawn(move || {
        let mut size = 0;
        for entry in iterator {
            size += 1;
            if entries_sender.send(entry).is_err() {
                log::debug!("Entries sender channel closed. Closing producer");
                break;
            }
        }
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

    ResultsIterator::new(producer, results_receiver)
}

pub struct ResultsIterator<R> {
    join_handle: Option<JoinHandle<usize>>,
    total: Option<usize>,
    receiver: crossbeam_channel::Receiver<R>,
    received: usize,
}
impl<R> ResultsIterator<R> {
    fn new(join_handle: JoinHandle<usize>, receiver: crossbeam_channel::Receiver<R>) -> Self {
        Self {
            join_handle: Some(join_handle),
            total: None,
            receiver,
            received: 0,
        }
    }
}

impl<R> Iterator for ResultsIterator<R> {
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(handle) = self.join_handle.take() {
            if handle.is_finished() {
                self.total.replace(handle.join().unwrap());
            }
        }

        if let Some(total) = self.total {
            if self.received >= total {
                return None;
            }
        }

        // we either didn't finish, or we are still missing our total
        self.receiver.recv().ok()
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
