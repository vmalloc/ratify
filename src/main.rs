use algo::Algorithm;
use anyhow::Context;
use clap::{Parser, Subcommand};
use itertools::Itertools;
use std::{collections::HashSet, path::PathBuf};
use strum::{EnumString, IntoEnumIterator};
use termcolor::{StandardStream, WriteColor};
use tracing_subscriber::{filter::LevelFilter, EnvFilter};
use utils::Canonicalizeable;

mod algo;
mod catalog;
mod errors;
mod parallel;
mod progress;
mod reporting;
mod utils;

use reporting::ReportFormatter;

#[derive(Parser)]
struct SignParams {
    /// algorithm to use for hashing (run list-algos to view available algorithms)
    #[clap(short = 'a', long)]
    algo: Algorithm,
    #[arg(short)]
    recursive: bool,
    #[clap(default_value = ".")]
    path: PathBuf,
}

#[derive(Parser)]
struct AppendParams {
    #[clap(default_value = ".")]
    path: PathBuf,
}

#[derive(Parser)]
struct TestParams {
    /// Kind of report to generate (plain/json)
    #[clap(long = "report", default_value = "plain")]
    report_type: ReportType,

    /// Filename to write a summary report to (see also --report)
    #[clap(long)]
    report_filename: Option<PathBuf>,

    /// algorithm to use
    #[clap(short = 'a', long)]
    algo: Option<Algorithm>,

    #[clap(default_value = ".")]
    path: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    /// Creates a new signature catalog for this directory, signing its contents recursively
    Sign {
        #[clap(flatten)]
        params: SignParams,
    },
    /// Adds entries for unknown files to an already-existing catalog
    Append {
        #[clap(flatten)]
        params: AppendParams,
    },
    /// Verifies an existing signature catalog against the actual directory contents
    Test {
        #[clap(flatten)]
        params: TestParams,
    },
    /// Lists available signature (hashing) algorithms
    ListAlgos,
}

#[derive(Parser)]
struct Opts {
    #[arg(short, long="verbose", action=clap::ArgAction::Count)]
    verbosity: u8,

    #[clap(subcommand)]
    command: Command,
}

#[derive(EnumString, Clone)]
#[strum(serialize_all = "snake_case")]
enum ReportType {
    Json,
    Plain,
}

fn checksum_entry(
    algo: Algorithm,
    dir_entry: walkdir::DirEntry,
    relative_filename: String,
) -> anyhow::Result<(u64, String, Vec<u8>)> {
    let path = dir_entry.path();

    log::debug!("Checksumming {path:?}...");

    let (size, hash) = algo
        .hash_file(path)
        .with_context(|| format!("Failed hashing {path:?}"))?;

    log::debug!("Checksumming {path:?} complete: {hash:?}");
    Ok((size, relative_filename, hash))
}

fn create_catalog(params: SignParams) -> anyhow::Result<()> {
    let directory = catalog::Directory::new(&params.path)?;

    let mut catalog = directory.empty_catalog(params.algo);

    catalog.populate()?;

    catalog.write_signature_file(false)
}

fn test_catalog(params: TestParams) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let directory = catalog::Directory::new(&params.path)?;

    let iterator = walkdir::WalkDir::new(directory.path());
    let all_paths_thread = std::thread::spawn(move || {
        iterator
            .into_iter()
            .filter_ok(|entry| !entry.path().is_dir())
            .map(|entry| {
                entry
                    .context("Failed reading directory")
                    .and_then(|e| e.path().try_canonicalize())
            })
            .collect::<Result<HashSet<_>, _>>()
    });

    let catalog = directory
        .load(params.algo)
        .context("Failed loading directory")?;
    let catalog_filename = catalog.metadata().signature_file_path().clone();

    let algo = catalog.metadata().algo();

    let bar = crate::progress::ProgressBar::new(catalog.len());

    let mut report = crate::parallel::for_each(catalog.into_iter(), |entry| {
        let res = entry
            .verify(algo)
            .inspect_err(|e| log::info!("Failed checksum for {:?}: {e:?}", entry.path()));
        bar.notify_record_processed(res.as_ref().map(|r| r.processed_size()).ok());
        res
    })
    .collect::<anyhow::Result<reporting::VerificationReport>>()?;

    drop(bar);

    let mut all_paths = all_paths_thread
        .join()
        .unwrap()
        .context("Failed listing all files in directory")?;

    all_paths.remove(&catalog_filename);
    all_paths.remove(&params.path.try_canonicalize()?);
    report.update_unknown(all_paths);

    let mut report_writer: Box<dyn WriteColor> = if let Some(path) = &params.report_filename {
        log::debug!("Opening report file {:?} for writing...", path);
        Box::new(termcolor::NoColor::new(
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .truncate(true)
                .open(path)
                .context("Failed opening report file")?,
        ))
    } else {
        Box::new(StandardStream::stderr(termcolor::ColorChoice::Auto))
    };

    match params.report_type {
        ReportType::Json => {
            crate::reporting::JsonFormatter.format(&report, start, &mut report_writer)
        }
        ReportType::Plain => {
            crate::reporting::PlainFormatter.format(&report, start, &mut report_writer)
        }
    }?;

    report.result()
}

fn append_catalog(params: AppendParams) -> anyhow::Result<()> {
    let dir = crate::catalog::Directory::new(params.path)?;
    let mut catalog = dir.load(None)?;

    catalog.populate()?;

    catalog.write_signature_file(true)
}

fn main() {
    if let Err(e) = entry_point() {
        match e.downcast_ref() {
            Some(
                crate::errors::Error::Failed
                | crate::errors::Error::Missing
                | crate::errors::Error::Unknown,
            ) => {
                eprintln!("{}", e);
            }
            _ => {
                log::error!("Error occurred: {e:?}");
            }
        }

        std::process::exit(-1);
    }
}

fn entry_point() -> anyhow::Result<()> {
    let opts = Opts::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::default().add_directive(
                match opts.verbosity {
                    0 => LevelFilter::ERROR,
                    1 => LevelFilter::INFO,
                    _ => LevelFilter::DEBUG,
                }
                .into(),
            ),
        )
        .compact()
        .init();

    match opts.command {
        Command::Sign { params } => create_catalog(params),
        Command::Append { params } => append_catalog(params),
        Command::Test { params } => test_catalog(params),
        Command::ListAlgos => {
            for algo in Algorithm::iter() {
                println!("{algo}");
            }
            Ok(())
        }
    }
}
