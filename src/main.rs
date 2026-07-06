use algo::Algorithm;
use anyhow::Context;
use clap::{Parser, Subcommand};
use itertools::Itertools;
use std::{collections::HashSet, io::Write, path::PathBuf};
use strum::{EnumString, IntoEnumIterator};
use termcolor::{Color, ColorSpec, StandardStream, WriteColor};
use tracing_subscriber::{filter::LevelFilter, EnvFilter};
use utils::{read_single_char, Canonicalizeable};

mod algo;
mod catalog;
mod config;
mod errors;
mod parallel;
mod progress;
mod reporting;
mod utils;

use reporting::ReportFormatter;

#[derive(Parser, Debug)]
struct SignParams {
    /// algorithm to use for hashing (run list-algos to view available algorithms)
    #[clap(short = 'a', long)]
    algo: Option<Algorithm>,
    /// path to the catalog file to create/use instead of the default location
    #[clap(long)]
    catalog_file: Option<PathBuf>,
    /// automatically overwrite existing catalog file without prompting
    #[clap(long)]
    overwrite: bool,
    #[arg(short)]
    recursive: bool,
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

    /// path to the catalog file to use instead of the default location
    #[clap(long)]
    catalog_file: Option<PathBuf>,

    #[clap(default_value = ".")]
    path: PathBuf,
}

#[derive(Parser)]
struct UpdateParams {
    /// algorithm to use
    #[clap(short = 'a', long)]
    algo: Option<Algorithm>,

    /// path to the catalog file to use instead of the default location
    #[clap(long)]
    catalog_file: Option<PathBuf>,

    /// automatically confirm all updates without prompting
    #[clap(long)]
    confirm: bool,

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

    /// Verifies an existing signature catalog against the actual directory contents
    Test {
        #[clap(flatten)]
        params: TestParams,
    },
    /// Interactively updates entries with verification discrepancies
    Update {
        #[clap(flatten)]
        params: UpdateParams,
    },
    /// Lists available signature (hashing) algorithms
    ListAlgos,
}

#[derive(Parser)]
#[command(version = env!("CARGO_PKG_VERSION"))]
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

struct Verification {
    report: reporting::VerificationReport,
    algo: Algorithm,
}

fn load_and_verify_catalog(
    directory: catalog::Directory,
    algo_param: Option<Algorithm>,
    path_param: &PathBuf,
) -> anyhow::Result<Verification> {
    let iterator = walkdir::WalkDir::new(directory.path());
    let ignore_matcher = directory.load_ignore_matcher()?;
    let root_path = directory.path().to_owned();
    let all_paths_thread = std::thread::spawn(move || {
        iterator
            .into_iter()
            .filter_ok(|entry| !entry.path().is_dir())
            .filter_ok(move |entry| {
                match pathdiff::diff_paths(entry.path(), &root_path) {
                    Some(relpath) => {
                        !catalog::is_ignored(&ignore_matcher, &relpath.to_string_lossy())
                    }
                    None => true,
                }
            })
            .map(|entry| {
                entry
                    .context("Failed reading directory")
                    .and_then(|e| e.path().try_canonicalize())
            })
            .collect::<Result<HashSet<_>, _>>()
    });

    let catalog = directory.load(algo_param)?;
    let catalog_filename = catalog.metadata().signature_file_path().clone();
    let algo = catalog.metadata().algo();

    let bar = crate::progress::ProgressBar::new_with_description(Some(catalog.len()), "Verifying");

    let mut report = crate::parallel::for_each(catalog.into_iter(), move |entry| {
        let res = entry
            .verify(algo)
            .inspect_err(|e| log::info!("Failed checksum for {:?}: {e:?}", entry.path()));
        bar.notify_record_processed(res.as_ref().map(|r| r.processed_size()).ok());
        res
    })
    .collect::<anyhow::Result<reporting::VerificationReport>>()?;

    let mut all_paths = all_paths_thread
        .join()
        .unwrap()
        .context("Failed listing all files in directory")?;

    all_paths.remove(&catalog_filename);
    all_paths.remove(&path_param.try_canonicalize()?);
    report.update_unknown(all_paths);

    Ok(Verification { report, algo })
}

fn create_catalog(params: SignParams, config: &config::Config) -> anyhow::Result<()> {
    let directory = catalog::Directory::from_params(&params.path, params.catalog_file)?;

    let algo = params.algo.or(config.default_sign_algo).ok_or_else(|| {
        anyhow::format_err!(
            "No algorithm specified. Use --algo or set default_sign_algo in ~/.config/ratify.toml"
        )
    })?;

    let catalog_file_path = directory.get_catalog_file_path(algo);

    if catalog_file_path.as_path().exists()
        && !params.overwrite
        && !prompt_user_overwrite(catalog_file_path.as_path())?
    {
        anyhow::bail!(
            "Catalog file {:?} already exists. Use --overwrite to overwrite automatically.",
            catalog_file_path.as_path()
        );
    }

    let mut catalog = directory.empty_catalog(algo);

    let bar = crate::progress::ProgressBar::new_with_description(None, "Signing");

    catalog.populate_with_progress(Some(bar))?;

    let should_overwrite = catalog_file_path.as_path().exists();
    catalog.write_signature_file(should_overwrite)
}

fn test_catalog(params: TestParams) -> anyhow::Result<()> {
    let start = std::time::Instant::now();
    let directory = catalog::Directory::from_params(&params.path, params.catalog_file)?;

    let report = load_and_verify_catalog(directory, params.algo, &params.path)?.report;

    let mut report_writer: Box<dyn WriteColor> = if let Some(path) = &params.report_filename {
        log::debug!("Opening report file {path:?} for writing...");
        Box::new(termcolor::NoColor::new(
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
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

#[derive(Debug, Clone, PartialEq)]
enum UpdateAction {
    Skip,
    Update,
    UpdateSubdirectory,
    UpdateAll,
}

fn read_user_choice(writer: &mut dyn WriteColor) -> anyhow::Result<UpdateAction> {
    writer.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
    print!("[S]kip [U]pdate [D]irectory [A]ll (default: Skip): ");
    writer.reset()?;
    std::io::stdout().flush()?;

    let key = read_single_char()?.to_ascii_lowercase();
    println!("{key}");

    match key {
        'u' => Ok(UpdateAction::Update),
        'd' => Ok(UpdateAction::UpdateSubdirectory),
        'a' => Ok(UpdateAction::UpdateAll),
        _ => Ok(UpdateAction::Skip),
    }
}

fn prompt_user_overwrite(catalog_path: &std::path::Path) -> anyhow::Result<bool> {
    let mut writer = StandardStream::stderr(termcolor::ColorChoice::Auto);

    writer.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
    println!("Catalog file {:?} already exists.", catalog_path);
    writer.reset()?;

    writer.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
    print!("Overwrite existing catalog file? [y/N]: ");
    writer.reset()?;
    std::io::stdout().flush()?;

    let key = read_single_char()?.to_ascii_lowercase();
    println!("{key}");

    Ok(key == 'y')
}

fn confirm_updates(
    paths: &HashSet<&std::path::Path>,
    writer: &mut dyn WriteColor,
) -> anyhow::Result<bool> {
    if paths.is_empty() {
        writer.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
        println!("Nothing to do.");
        writer.reset()?;
        return Ok(false);
    }

    writer.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
    println!("\nThe following files will be updated:");
    writer.reset()?;

    for path in paths {
        writer.set_color(
            ColorSpec::new()
                .set_fg(Some(Color::White))
                .set_intense(true),
        )?;
        println!("  {path:?}");
        writer.reset()?;
    }

    writer.set_color(ColorSpec::new().set_fg(Some(Color::Cyan)))?;
    print!("\nProceed with updates? [y/N]: ");
    writer.reset()?;
    std::io::stdout().flush()?;

    let key = read_single_char()?.to_ascii_lowercase();
    println!("{key}");

    Ok(key == 'y')
}

fn update_catalog(params: UpdateParams) -> anyhow::Result<()> {
    let directory = catalog::Directory::from_params(&params.path, params.catalog_file.clone())?;

    let verification = load_and_verify_catalog(directory, params.algo, &params.path)?;
    let report = verification.report;
    let algo = verification.algo;

    let mut writer = StandardStream::stderr(termcolor::ColorChoice::Auto);

    let mut files_to_update = HashSet::new();

    if params.confirm {
        for entry in report.entries() {
            if !matches!(entry.status(), reporting::EntryStatus::Ok) {
                files_to_update.insert(entry.path());
            }
        }
    } else {
        let mut processed_directories = HashSet::new();

        for entry in report.entries() {
            if matches!(entry.status(), reporting::EntryStatus::Ok) {
                continue;
            }

            let current_dir = entry.path().parent();
            if let Some(dir) = current_dir {
                if processed_directories.contains(dir) {
                    files_to_update.insert(entry.path());
                    continue;
                }
            }

            println!();
            reporting::output_status_line(&mut writer, entry.path(), entry.status())?;

            match entry.status() {
                reporting::EntryStatus::VerificationError => {
                    writer.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
                    println!("  Status: Checksum mismatch");
                    writer.reset()?;
                }
                reporting::EntryStatus::Missing => {
                    writer.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
                    println!("  Status: File missing");
                    writer.reset()?;
                }
                reporting::EntryStatus::Unknown => {
                    writer.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
                    println!("  Status: Unknown file");
                    writer.reset()?;
                }
                reporting::EntryStatus::Ok => unreachable!(),
            }

            let action = read_user_choice(&mut writer)?;

            match action {
                UpdateAction::Skip => continue,
                UpdateAction::Update => {
                    files_to_update.insert(entry.path());
                }
                UpdateAction::UpdateSubdirectory => {
                    files_to_update.insert(entry.path());

                    if let Some(dir) = current_dir {
                        processed_directories.insert(dir);
                    }

                    for other_entry in report.entries() {
                        if matches!(other_entry.status(), reporting::EntryStatus::Ok) {
                            continue;
                        }

                        if current_dir == other_entry.path().parent() {
                            files_to_update.insert(other_entry.path());
                        }
                    }
                }
                UpdateAction::UpdateAll => {
                    for e in report.entries() {
                        if !matches!(e.status(), reporting::EntryStatus::Ok) {
                            files_to_update.insert(e.path());
                        }
                    }
                    break;
                }
            }
        }
    }

    if !params.confirm && !confirm_updates(&files_to_update, &mut writer)? {
        if !files_to_update.is_empty() {
            writer.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
            println!("Update cancelled.");
            writer.reset()?;
        }
        return Ok(());
    }

    let directory_for_update = catalog::Directory::from_params(&params.path, params.catalog_file)?;
    let mut catalog = directory_for_update.load(params.algo)?;

    for path in &files_to_update {
        let relative_path = pathdiff::diff_paths(path, catalog.directory().path())
            .ok_or_else(|| anyhow::format_err!("Unable to get relative path for {:?}", path))?;

        if path.exists() {
            let (_, new_hash) = algo
                .hash_file(path)
                .with_context(|| format!("Failed hashing {path:?}"))?;
            catalog.update_entry(&relative_path.to_string_lossy(), new_hash);
            writer.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
            println!("Updated: {path:?}");
            writer.reset()?;
        } else {
            catalog.remove_entry(&relative_path.to_string_lossy());
            writer.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
            println!("Removed: {path:?}");
            writer.reset()?;
        }
    }

    catalog.write_signature_file(true)?;
    writer.set_color(
        ColorSpec::new()
            .set_fg(Some(Color::Green))
            .set_intense(true),
    )?;
    println!("Catalog updated successfully.");
    writer.reset()?;
    Ok(())
}

fn main() {
    if let Err(e) = entry_point() {
        match e.downcast_ref() {
            Some(
                crate::errors::Error::Failed
                | crate::errors::Error::Missing
                | crate::errors::Error::Unknown,
            ) => {
                eprintln!("{e}");
            }
            _ => {
                eprintln!("ERROR: {e}");
            }
        }

        std::process::exit(-1);
    }
}

fn entry_point() -> anyhow::Result<()> {
    let opts = Opts::parse();
    let config = config::Config::load()?;

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
        Command::Sign { params } => create_catalog(params, &config),

        Command::Test { params } => test_catalog(params),
        Command::Update { params } => update_catalog(params),
        Command::ListAlgos => {
            for algo in Algorithm::iter() {
                println!("{algo}");
            }
            Ok(())
        }
    }
}
