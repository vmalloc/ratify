use std::sync::atomic::AtomicU64;

use indicatif::{HumanBytes, ProgressDrawTarget, ProgressStyle};

const SIZE_UPDATE_FREQ: u64 = 100;

pub struct ProgressBar {
    bar: indicatif::ProgressBar,
    size: AtomicU64,
    count: AtomicU64,
    start_time: std::time::Instant,
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
        Self {
            bar,
            size: AtomicU64::default(),
            count: AtomicU64::default(),
            start_time: std::time::Instant::now(),
        }
    }

    pub fn notify_record_processed(&self, record_size: Option<u64>) {
        self.bar.inc(1);
        let record_size = record_size.unwrap_or_default();
        let size = self
            .size
            .fetch_add(record_size, std::sync::atomic::Ordering::Relaxed)
            + record_size;

        let count = self
            .count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;

        if count % SIZE_UPDATE_FREQ == 0 {
            self.bar.set_message(format!(
                "{}/s",
                HumanBytes(
                    size.checked_div(
                        std::time::Instant::now()
                            .duration_since(self.start_time)
                            .as_secs()
                    )
                    .unwrap_or_default()
                )
            ));
        }
    }
}
