use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;

use crate::utils::CanonicalPath;

#[derive(PartialEq, Eq)]
pub enum EntryStatus {
    Ok,
    VerificationError,
    Missing,
    Unknown,
}

pub struct ReportEntry {
    path: Arc<CanonicalPath<PathBuf>>,
    processed_size: u64,
    status: EntryStatus,
}

impl ReportEntry {
    pub fn new(
        path: Arc<CanonicalPath<PathBuf>>,
        processed_size: u64,
        status: EntryStatus,
    ) -> Self {
        Self {
            path,
            processed_size,
            status,
        }
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn canonical_path(&self) -> &CanonicalPath<PathBuf> {
        &self.path
    }
}

pub trait ReportFormatter {
    fn format(
        &mut self,
        report: &VerificationReport,
        start_time: std::time::Instant,
        writer: &mut dyn Write,
    ) -> anyhow::Result<()>;
}

pub struct PlainFormatter;

impl ReportFormatter for PlainFormatter {
    fn format(
        &mut self,
        report: &VerificationReport,
        start_time: std::time::Instant,
        writer: &mut dyn Write,
    ) -> anyhow::Result<()> {
        let mut num_ok = 0;
        let mut num_failed = 0;
        let mut num_missing = 0;
        let mut total_bytes = 0;
        for entry in report.entries.iter() {
            total_bytes += entry.processed_size;
            match entry.status {
                EntryStatus::Ok => {
                    num_ok += 1;
                }
                EntryStatus::VerificationError => {
                    writeln!(writer, "* {:?} failed verification", entry.path())?;
                    num_failed += 1
                }
                EntryStatus::Missing => {
                    writeln!(writer, "* {:?} missing", entry.path())?;
                    num_missing += 1
                }
                EntryStatus::Unknown => {
                    writeln!(writer, "* {:?} is unknown", entry.path())?;
                }
            }
        }
        writeln!(writer, "{} entries checked", report.entries.len())?;
        writeln!(writer, "{num_ok} OK")?;
        if num_failed > 0 {
            writeln!(writer, "{num_failed} entries failed verification")?;
        }
        if num_missing > 0 {
            writeln!(writer, "{num_missing} entries failed verification")?;
        }
        let end = std::time::Instant::now();
        let duration = end.duration_since(start_time);
        let mut bw = (total_bytes as f64) / duration.as_secs_f64();
        if bw.is_infinite() {
            bw = 0.0;
        }

        writeln!(
            writer,
            "{} done in {duration:?} ({:.02} MB/sec)",
            human_bytes::human_bytes(total_bytes as f64),
            bw / 1_000_000.0,
        )?;
        Ok(())
    }
}

pub struct JsonFormatter;

impl ReportFormatter for JsonFormatter {
    fn format(
        &mut self,
        report: &VerificationReport,
        _start_time: std::time::Instant,
        writer: &mut dyn Write,
    ) -> anyhow::Result<()> {
        let mut failed = Vec::new();
        for entry in report.entries.iter() {
            match entry.status {
                EntryStatus::Ok => {}
                EntryStatus::VerificationError => failed.push(serde_json::json!({
                    "path": entry.path(),
                    "status": "fail"
                })),
                EntryStatus::Unknown => failed.push(serde_json::json!({
                    "path": entry.path(),
                    "status": "unknown"
                })),
                EntryStatus::Missing => failed.push(serde_json::json!({
                    "path": entry.path(),
                    "status": "missing"
                })),
            }
        }
        let json_report = serde_json::json!({
            "processed": report.entries.len(),
            "total_size": report.total_size,
            "failed": failed,
        });
        serde_json::to_writer(writer, &json_report).context("Failed serializing report")
    }
}

pub struct VerificationReport {
    total_size: u64,
    entries: Vec<ReportEntry>,
}

impl VerificationReport {
    pub(crate) fn update_unknown(&mut self, mut all_paths: HashSet<CanonicalPath<PathBuf>>) {
        for entry in self.entries.iter() {
            all_paths.remove(entry.canonical_path());
        }

        for path in all_paths {
            self.entries.push(ReportEntry {
                path: path.into(),
                processed_size: 0,
                status: EntryStatus::Unknown,
            });
        }
    }

    pub(crate) fn result(&self) -> Result<(), anyhow::Error> {
        let mut has_unknown = false;
        let mut has_missing = false;

        for entry in self.entries.iter() {
            match &entry.status {
                EntryStatus::VerificationError => {
                    anyhow::bail!("Failed entries found");
                }
                EntryStatus::Missing => {
                    has_missing = true;
                }
                EntryStatus::Unknown => {
                    has_unknown = true;
                }
                _ => (),
            }
        }
        if has_missing {
            anyhow::bail!("Missing entries found");
        } else if has_unknown {
            anyhow::bail!("Unknown entries found");
        }
        Ok(())
    }
}

impl FromIterator<ReportEntry> for VerificationReport {
    fn from_iter<T: IntoIterator<Item = ReportEntry>>(iter: T) -> Self {
        let mut total_size = 0;
        let entries = iter
            .into_iter()
            .inspect(|entry| total_size += entry.processed_size)
            .collect();
        Self {
            entries,
            total_size,
        }
    }
}
