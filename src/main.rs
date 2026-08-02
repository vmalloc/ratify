use algo::Algorithm;
use anyhow::Context;
use clap::{Parser, Subcommand};
use itertools::Itertools;
use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
};
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

    /// only verify that cataloged files exist, without reading/hashing their
    /// contents (useful for quickly finding missing or new files)
    #[clap(long)]
    existence_only: bool,

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

    /// automatically apply all updates without prompting
    #[clap(long)]
    yes: bool,

    /// only add new (unknown) files to the catalog; skip modified or missing entries
    #[clap(long)]
    new_only: bool,

    #[clap(default_value = ".")]
    path: PathBuf,
}

#[derive(Parser)]
struct AppendParams {
    /// algorithm to use
    #[clap(short = 'a', long)]
    algo: Option<Algorithm>,

    /// path to the catalog file to use instead of the default location
    #[clap(long)]
    catalog_file: Option<PathBuf>,

    /// automatically add all new files without prompting
    #[clap(long)]
    yes: bool,

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
    /// Adds new (unknown) files to the catalog without modifying existing entries
    Append {
        #[clap(flatten)]
        params: AppendParams,
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

    tracing::debug!("Checksumming {path:?}...");

    let result = algo
        .hash_file(path)
        .with_context(|| format!("Failed hashing {path:?}"))?;

    tracing::debug!("Checksumming {path:?} complete: {:?}", result.hash);
    Ok((result.size, relative_filename, result.hash))
}

struct Verification {
    report: reporting::VerificationReport,
    catalog: catalog::Catalog,
}

/// Whether a cataloged entry should still be considered part of the catalog,
/// or has since been excluded by a `.ratify-ignore` rule.
fn should_keep_entry(
    entry_path: &Path,
    root_path: &Path,
    ignore_matcher: &ignore::gitignore::Gitignore,
) -> bool {
    match pathdiff::diff_paths(entry_path, root_path) {
        Some(relpath) => {
            let ignored = catalog::is_ignored(ignore_matcher, &relpath.to_string_lossy());
            tracing::debug!(
                "Ignore check: entry={entry_path:?} root={root_path:?} relpath={relpath:?} ignored={ignored}"
            );
            !ignored
        }
        None => {
            tracing::debug!(
                "Ignore check: entry={entry_path:?} root={root_path:?} diff_paths returned None; keeping"
            );
            true
        }
    }
}

fn load_and_verify_catalog(
    directory: catalog::Directory,
    algo_param: Option<Algorithm>,
    path_param: &Path,
    existence_only: bool,
) -> anyhow::Result<Verification> {
    let iterator = walkdir::WalkDir::new(directory.path());
    let ignore_matcher = directory.load_ignore_matcher()?;
    let root_path = directory.path().to_owned();

    let walk_matcher = ignore_matcher.clone();
    let walk_root = root_path.clone();
    let all_paths_thread = std::thread::spawn(move || {
        iterator
            .into_iter()
            .filter_ok(|entry| !entry.path().is_dir())
            .filter_ok(move |entry| match pathdiff::diff_paths(entry.path(), &walk_root) {
                Some(relpath) => !catalog::is_ignored(&walk_matcher, &relpath.to_string_lossy()),
                None => true,
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

    // Partition out any catalog entries that now match a .ratify-ignore rule.
    // These were signed before the rule was added; skipping them (with a
    // warning) keeps ignore semantics consistent -- an ignored file is never
    // verified and never flagged as unknown -- and points the user at
    // re-signing.
    let (entries, ignored_entries): (Vec<_>, Vec<_>) = catalog
        .iter_entries()
        .partition(|entry| should_keep_entry(entry.path(), &root_path, &ignore_matcher));

    if !ignored_entries.is_empty() {
        let mut writer = StandardStream::stderr(termcolor::ColorChoice::Auto);
        writer.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)))?;
        for entry in &ignored_entries {
            writeln!(
                writer,
                "Warning: catalog entry {:?} matches a .ratify-ignore rule; skipping it (re-sign to remove it from the catalog)",
                entry.path()
            )?;
        }
        writer.reset()?;
    }

    let description = if existence_only {
        "Checking"
    } else {
        "Verifying"
    };
    let bar = crate::progress::ProgressBar::new_with_description(Some(entries.len()), description);

    let mut report = crate::parallel::for_each(entries.into_iter(), move |entry| {
        let res = if existence_only {
            entry.verify_existence().inspect_err(|e| {
                tracing::info!("Failed existence check for {:?}: {e:?}", entry.path())
            })
        } else {
            entry
                .verify(algo)
                .inspect_err(|e| tracing::info!("Failed checksum for {:?}: {e:?}", entry.path()))
        };
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

    Ok(Verification { report, catalog })
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

    let report =
        load_and_verify_catalog(directory, params.algo, &params.path, params.existence_only)?
            .report;

    let mut report_writer: Box<dyn WriteColor> = if let Some(path) = &params.report_filename {
        tracing::debug!("Opening report file {path:?} for writing...");
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

/// Prints `msg` in `color` (bold when `intense`), then resets. Collapses the
/// repeated `set_color` -> `println!` -> `reset` ceremony used throughout the
/// interactive update/append flows.
fn print_colored(
    writer: &mut dyn WriteColor,
    color: Color,
    intense: bool,
    msg: &str,
) -> anyhow::Result<()> {
    writer.set_color(ColorSpec::new().set_fg(Some(color)).set_intense(intense))?;
    println!("{msg}");
    writer.reset()?;
    Ok(())
}

/// Hashes (or, if missing, removes) each of `paths` in the catalog and writes
/// it back to disk. `verb` labels the per-file success message (e.g.
/// "Updated" or "Added"); a path that's gone missing is always reported as
/// "Removed" regardless of `verb`.
fn apply_entries(
    catalog: &mut catalog::Catalog,
    algo: Algorithm,
    paths: &HashSet<&Path>,
    verb: &str,
    writer: &mut dyn WriteColor,
) -> anyhow::Result<()> {
    for path in paths {
        let relative_path = pathdiff::diff_paths(path, catalog.directory().path())
            .ok_or_else(|| anyhow::format_err!("Unable to get relative path for {:?}", path))?;

        if path.exists() {
            let result = algo
                .hash_file(path)
                .with_context(|| format!("Failed hashing {path:?}"))?;
            catalog.update_entry(&relative_path.to_string_lossy(), result.hash);
            print_colored(writer, Color::Green, false, &format!("{verb}: {path:?}"))?;
        } else {
            catalog.remove_entry(&relative_path.to_string_lossy());
            print_colored(writer, Color::Red, false, &format!("Removed: {path:?}"))?;
        }
    }

    catalog.write_signature_file(true)?;
    print_colored(writer, Color::Green, true, "Catalog updated successfully.")
}

fn update_catalog(params: UpdateParams) -> anyhow::Result<()> {
    let directory = catalog::Directory::from_params(&params.path, params.catalog_file.clone())?;

    let verification = load_and_verify_catalog(directory, params.algo, &params.path, false)?;
    let report = verification.report;
    let mut catalog = verification.catalog;
    let algo = catalog.metadata().algo();

    let mut writer = StandardStream::stderr(termcolor::ColorChoice::Auto);

    let mut files_to_update = HashSet::new();

    if params.yes {
        for entry in report.entries() {
            if entry.status().is_actionable(params.new_only) {
                files_to_update.insert(entry.path());
            }
        }
    } else {
        let mut processed_directories = HashSet::new();

        for entry in report.entries() {
            if !entry.status().is_actionable(params.new_only) {
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
                    print_colored(
                        &mut writer,
                        Color::Red,
                        false,
                        "  Status: Checksum mismatch",
                    )?;
                }
                reporting::EntryStatus::Missing => {
                    print_colored(&mut writer, Color::Red, false, "  Status: File missing")?;
                }
                reporting::EntryStatus::Unknown => {
                    print_colored(&mut writer, Color::Yellow, false, "  Status: Unknown file")?;
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
                        if !other_entry.status().is_actionable(params.new_only) {
                            continue;
                        }

                        if current_dir == other_entry.path().parent() {
                            files_to_update.insert(other_entry.path());
                        }
                    }
                }
                UpdateAction::UpdateAll => {
                    for e in report.entries() {
                        if e.status().is_actionable(params.new_only) {
                            files_to_update.insert(e.path());
                        }
                    }
                    break;
                }
            }
        }
    }

    if !params.yes && !confirm_updates(&files_to_update, &mut writer)? {
        if !files_to_update.is_empty() {
            print_colored(&mut writer, Color::Yellow, false, "Update cancelled.")?;
        }
        return Ok(());
    }

    apply_entries(&mut catalog, algo, &files_to_update, "Updated", &mut writer)
}

fn append_catalog(params: AppendParams) -> anyhow::Result<()> {
    let directory = catalog::Directory::from_params(&params.path, params.catalog_file.clone())?;

    let verification = load_and_verify_catalog(directory, params.algo, &params.path, false)?;
    let report = verification.report;
    let mut catalog = verification.catalog;
    let algo = catalog.metadata().algo();

    let mut writer = StandardStream::stderr(termcolor::ColorChoice::Auto);

    let files_to_add: HashSet<_> = report
        .entries()
        .iter()
        .filter(|entry| matches!(entry.status(), reporting::EntryStatus::Unknown))
        .map(|entry| entry.path())
        .collect();

    if !params.yes && !confirm_updates(&files_to_add, &mut writer)? {
        if !files_to_add.is_empty() {
            print_colored(&mut writer, Color::Yellow, false, "Append cancelled.")?;
        }
        return Ok(());
    }

    apply_entries(&mut catalog, algo, &files_to_add, "Added", &mut writer)
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
        Command::Append { params } => append_catalog(params),
        Command::ListAlgos => {
            for algo in Algorithm::iter() {
                println!("{algo}");
            }
            Ok(())
        }
    }
}
