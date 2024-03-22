pub(crate) fn for_each<I, F, R, T>(iterator: I, handler: F) -> impl Iterator<Item = R>
where
    I: Iterator<Item = T> + Send,
    R: Send,
    F: Fn(T) -> R + Send + Clone,
    T: Send,
{
    let (entries_sender, entries_receiver) = crossbeam_channel::bounded(num_cpus::get() * 4);

    let (results_sender, results_receiver) = crossbeam_channel::unbounded();

    std::thread::scope(|s| {
        let _producer = s.spawn(move || {
            for entry in iterator {
                if entries_sender.send(entry).is_err() {
                    log::debug!("Entries sender channel closed. Closing producer");
                    break;
                }
            }
        });

        for _ in 0..(num_cpus::get() * 2) {
            let receiver = entries_receiver.clone();
            let sender = results_sender.clone();
            let handler = handler.clone();
            s.spawn(move || {
                for entry in receiver {
                    if sender.send(handler(entry)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(results_sender);
        results_receiver.into_iter()
    })
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
