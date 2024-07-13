use std::sync::{atomic::AtomicU64, Arc};

use indicatif::{HumanBytes, ProgressDrawTarget, ProgressStyle};

const SIZE_UPDATE_FREQ: std::time::Duration = std::time::Duration::from_secs(3);

pub struct ProgressBar {
    bar: Arc<indicatif::ProgressBar>,
    size: Arc<AtomicU64>,
}

impl ProgressBar {
    pub fn new(len: usize) -> Self {
        let bar = indicatif::ProgressBar::new(len.try_into().unwrap());
        bar.set_draw_target(ProgressDrawTarget::stderr_with_hz(5));
        bar.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
            )
            .unwrap(),
        );

        let bar = Arc::new(bar);
        let size = Arc::new(AtomicU64::default());

        let bar_weak = Arc::downgrade(&bar);
        let size_weak = Arc::downgrade(&size);

        std::thread::spawn(move || {
            let mut last_update_time = std::time::Instant::now();
            let mut last_size = 0;
            loop {
                std::thread::sleep(SIZE_UPDATE_FREQ);

                let Some(bar) = bar_weak.upgrade() else { break };
                let Some(size) = size_weak.upgrade() else {
                    break;
                };

                let now = std::time::Instant::now();
                let current_size = size.load(std::sync::atomic::Ordering::Relaxed);
                let size_diff = current_size - last_size;
                let time_diff = now.duration_since(last_update_time).as_secs();

                bar.set_message(format!(
                    "{}/s",
                    HumanBytes(size_diff.checked_div(time_diff).unwrap_or_default())
                ));
                last_update_time = now;
                last_size = current_size;
            }
        });

        Self { bar, size }
    }

    pub fn notify_record_processed(&self, record_size: Option<u64>) {
        self.bar.inc(1);
        let record_size = record_size.unwrap_or_default();
        self.size
            .fetch_add(record_size, std::sync::atomic::Ordering::Relaxed);
    }
}
