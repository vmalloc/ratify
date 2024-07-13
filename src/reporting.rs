use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use termcolor::{Color, ColorSpec};

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

    pub fn processed_size(&self) -> u64 {
        self.processed_size
    }
}

pub trait ReportFormatter {
    fn format(
        &mut self,
        report: &VerificationReport,
        start_time: std::time::Instant,
        writer: &mut dyn termcolor::WriteColor,
    ) -> anyhow::Result<()>;
}

fn output_short_status_line(
    writer: &mut dyn termcolor::WriteColor,
    entry: &ReportEntry,
    short_status: &str,
    color_spec: Option<&ColorSpec>,
) -> anyhow::Result<()> {
    write!(writer, "[")?;

    if let Some(spec) = &color_spec {
        writer.set_color(spec)?;
    }

    write!(writer, "{}", &short_status[..4])?;
    writer.reset()?;
    writeln!(writer, "] {:?}", entry.path())?;
    Ok(())
}

pub struct PlainFormatter;

impl ReportFormatter for PlainFormatter {
    fn format(
        &mut self,
        report: &VerificationReport,
        start_time: std::time::Instant,
        writer: &mut dyn termcolor::WriteColor,
    ) -> anyhow::Result<()> {
        let mut num_ok = 0;
        let mut num_failed = 0;
        let mut num_missing = 0;
        let mut num_unknown = 0;
        let mut total_bytes = 0;
        for entry in report.entries.iter() {
            total_bytes += entry.processed_size;
            match entry.status {
                EntryStatus::Ok => {
                    num_ok += 1;
                }
                EntryStatus::VerificationError => {
                    output_short_status_line(
                        writer,
                        entry,
                        "FAILED ",
                        Some(
                            ColorSpec::new()
                                .set_fg(Some(Color::Black))
                                .set_bg(Some(Color::Red)),
                        ),
                    )?;
                    num_failed += 1
                }
                EntryStatus::Missing => {
                    output_short_status_line(
                        writer,
                        entry,
                        "MISSING",
                        Some(ColorSpec::new().set_fg(Some(Color::Red))),
                    )?;
                    num_missing += 1
                }
                EntryStatus::Unknown => {
                    output_short_status_line(
                        writer,
                        entry,
                        "UNKNOWN",
                        Some(ColorSpec::new().set_fg(Some(Color::Yellow))),
                    )?;
                    num_unknown += 1;
                }
            }
        }
        writeln!(writer, "{} entries checked", report.entries.len())?;
        writeln!(writer, "{num_ok} OK")?;
        writer.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
        if num_failed > 0 {
            writeln!(writer, "{num_failed} Failed verification")?;
        }
        if num_missing > 0 {
            writeln!(writer, "{num_missing} Missing")?;
        }

        writer.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
        if num_unknown > 0 {
            writeln!(writer, "{num_unknown} Unknown")?;
        }

        writer.reset()?;
        let end = std::time::Instant::now();
        let duration = end.duration_since(start_time);
        let mut bw = (total_bytes as f64) / duration.as_secs_f64();
        if bw.is_infinite() {
            bw = 0.0;
        }

        writer.set_color(
            ColorSpec::new()
                .set_fg(Some(Color::Black))
                .set_intense(true),
        )?;
        writeln!(
            writer,
            "{} done in {duration:?} ({:.02} MB/sec)",
            human_bytes::human_bytes(total_bytes as f64),
            bw / 1_000_000.0,
        )?;

        writer.reset()?;
        writeln!(writer)?;

        Ok(())
    }
}

pub struct JsonFormatter;

impl ReportFormatter for JsonFormatter {
    fn format(
        &mut self,
        report: &VerificationReport,
        _start_time: std::time::Instant,
        writer: &mut dyn termcolor::WriteColor,
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
                    anyhow::bail!(crate::errors::Error::Failed);
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
            anyhow::bail!(crate::errors::Error::Missing);
        } else if has_unknown {
            anyhow::bail!(crate::errors::Error::Unknown);
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
