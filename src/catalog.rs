use anyhow::Context;
use itertools::Itertools;
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    algo::Algorithm,
    checksum_entry,
    reporting::EntryStatus,
    utils::{CanonicalPath, Canonicalizeable},
};

/// Name of the optional ignore file at the root of a signed directory.
pub(crate) const IGNORE_FILE_NAME: &str = ".ratify-ignore";

/// Returns whether the given directory-relative path is excluded by the matcher.
///
/// Uses `matched_path_or_any_parents` (rather than `matched`) so directory-style
/// patterns (e.g. `/build/`) still exclude files beneath them, since directories
/// are filtered out of the walk before reaching this check.
pub(crate) fn is_ignored(matcher: &ignore::gitignore::Gitignore, relpath: &str) -> bool {
    matcher
        .matched_path_or_any_parents(Path::new(relpath), false)
        .is_ignore()
}

pub struct Directory {
    path: CanonicalPath<PathBuf>,
    catalog_path: Option<CanonicalPath<PathBuf>>,
}

impl Directory {
    pub fn from_params(path: impl Into<PathBuf>, catalog_file: Option<PathBuf>) -> anyhow::Result<Self> {
        if let Some(catalog_file) = catalog_file {
            Self::with_catalog_file(path, catalog_file)
        } else {
            Self::new(path)
        }
    }

    pub fn new(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = path.into();
        let path = std::fs::canonicalize(&path)
            .with_context(|| format!("Failed resolve path {path:?}"))?
            .assume_canonical();

        Ok(Self {
            path,
            catalog_path: None,
        })
    }

    pub fn with_catalog_file(
        path: impl Into<PathBuf>,
        catalog_file: impl Into<PathBuf>,
    ) -> anyhow::Result<Self> {
        let path = path.into();
        let path = std::fs::canonicalize(&path)
            .with_context(|| format!("Failed resolve path {path:?}"))?
            .assume_canonical();

        let catalog_file = catalog_file.into();
        let catalog_file = if catalog_file.is_absolute() {
            catalog_file.assume_canonical()
        } else {
            path.as_path().join(catalog_file).assume_canonical()
        };

        Ok(Self {
            path,
            catalog_path: Some(catalog_file),
        })
    }

    pub fn load(self, algo: Option<Algorithm>) -> anyhow::Result<Catalog> {
        let (algo, filename) = if let Some(custom_file) = &self.catalog_path {
            let algo = algo
                .or_else(|| Algorithm::try_deduce_from_file(custom_file.as_path()))
                .ok_or_else(|| anyhow::format_err!(
                    "Failed to detect algorithm from catalog file {custom_file:?}. Please specify algorithm explicitly using --algo"
                ))?;
            (algo, custom_file.clone())
        } else if let Some(algo) = algo {
            let path = if let Some((_, legacy_path)) = Algorithm::try_deduce_from_path(self.path.as_path())
                .filter(|(detected_algo, _)| *detected_algo == algo)
            {
                legacy_path.assume_canonical()
            } else {
                self.signature_file_path(algo)
            };
            (algo, path)
        } else if let Some((algo, path)) = Algorithm::try_deduce_from_path(self.path.as_path()) {
            (algo, path.assume_canonical())
        } else {
            anyhow::bail!("Failed to detect signature file");
        };

        log::debug!("Opening signature file {filename:?}...");
        let file = std::io::BufReader::new(
            std::fs::File::open(filename.as_path())
                .with_context(|| format!("Failed opening {:?}", filename))?,
        );
        let mut entries = BTreeMap::new();

        for (lineno, line) in file.lines().enumerate().map(|(lineno, l)| (lineno + 1, l)) {
            let line = line.context("Cannot read file")?;
            let (hash, entry_path) = line.split_once(" *").ok_or_else(|| {
                anyhow::anyhow!("Syntax error at line {} of {:?}", lineno, filename)
            })?;

            let entry = hex::decode(hash)
                .map_err(|_| anyhow::anyhow!("Failed parsing line {lineno}: invalid hash"))?;

            let prev = entries.insert(entry_path.to_string(), entry);
            if prev.is_some() {
                anyhow::bail!(
                    "Entry {entry_path:?} appears multiple times in {:?}",
                    filename
                );
            }
        }
        let metadata = Arc::new(self.catalog_metadata_with_file(algo, filename));

        Ok(Catalog {
            metadata,
            directory: self,
            entries,
        })
    }

    pub fn signature_file_path(&self, algo: Algorithm) -> CanonicalPath<PathBuf> {
        self.path
            .as_path()
            .join(self.signature_filename(algo))
            .assume_canonical()
    }

    fn signature_filename(&self, algo: Algorithm) -> String {
        format!("ratify-catalog.{algo}")
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Builds a gitignore-style matcher rooted at this directory, loading an
    /// optional `.ratify-ignore` file from the directory root. The ignore file
    /// itself is always excluded so it is never signed or reported as unknown.
    pub(crate) fn load_ignore_matcher(&self) -> anyhow::Result<Arc<ignore::gitignore::Gitignore>> {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(self.path());

        builder
            .add_line(None, IGNORE_FILE_NAME)
            .context("Failed adding self-exclusion for ignore file")?;

        let ignore_file = self.path().join(IGNORE_FILE_NAME);
        if ignore_file.exists() {
            if let Some(err) = builder.add(&ignore_file) {
                return Err(err)
                    .with_context(|| format!("Failed parsing ignore file {ignore_file:?}"));
            }
        }

        let matcher = builder.build().context("Failed building ignore matcher")?;

        Ok(Arc::new(matcher))
    }

    pub fn get_catalog_file_path(&self, algo: Algorithm) -> CanonicalPath<PathBuf> {
        if let Some(custom_file) = &self.catalog_path {
            custom_file.clone()
        } else {
            self.signature_file_path(algo)
        }
    }

    pub(crate) fn catalog_metadata(&self, algo: Algorithm) -> CatalogMetadata {
        let signature_filename = self.signature_filename(algo);
        let signature_file_path = self
            .path
            .as_path()
            .join(signature_filename)
            .assume_canonical();
        CatalogMetadata {
            algo,
            signature_file_path,
        }
    }

    pub(crate) fn catalog_metadata_with_file(
        &self,
        algo: Algorithm,
        file_path: CanonicalPath<PathBuf>,
    ) -> CatalogMetadata {
        CatalogMetadata {
            algo,
            signature_file_path: file_path,
        }
    }

    pub(crate) fn empty_catalog(self, algo: Algorithm) -> Catalog {
        let metadata = if let Some(custom_file) = &self.catalog_path {
            Arc::new(self.catalog_metadata_with_file(algo, custom_file.clone()))
        } else {
            Arc::new(self.catalog_metadata(algo))
        };
        Catalog {
            directory: self,
            entries: Default::default(),
            metadata,
        }
    }
}

pub(crate) struct CatalogMetadata {
    algo: Algorithm,
    signature_file_path: CanonicalPath<PathBuf>,
}

impl CatalogMetadata {
    pub(crate) fn signature_file_path(&self) -> &CanonicalPath<PathBuf> {
        &self.signature_file_path
    }

    pub(crate) fn algo(&self) -> Algorithm {
        self.algo
    }
}

pub struct Catalog {
    directory: Directory,

    entries: BTreeMap<String, Vec<u8>>,
    metadata: Arc<CatalogMetadata>,
}

impl Catalog {
    pub fn populate_with_progress(
        &mut self,
        progress_bar: Option<crate::progress::ProgressBar>,
    ) -> anyhow::Result<()> {
        let mut new_entries = BTreeMap::new();
        let mut old_entries = Arc::new(std::mem::take(&mut self.entries));

        let ignore_matcher = self.directory.load_ignore_matcher()?;

        let iterator = walkdir::WalkDir::new(self.directory.path())
            .into_iter()
            .filter_ok(|entry| !entry.path().is_dir())
            .filter_ok({
                let metadata = self.metadata.clone();
                move |entry| entry.path() != metadata.signature_file_path.as_path()
            })
            .map({
                let directory_path = self.directory.path().to_owned();
                move |maybe_entry| {
                    maybe_entry.context("Failed reading entry").and_then({
                        |entry| {
                            let relative_path = pathdiff::diff_paths(entry.path(), &directory_path)
                                .ok_or_else(|| {
                                    anyhow::format_err!(
                                        "Unable to get relative path for {:?}",
                                        entry.path()
                                    )
                                })?;

                            Ok((entry, relative_path.to_string_lossy().to_string()))
                        }
                    })
                }
            })
            .filter_ok({
                let ignore_matcher = ignore_matcher.clone();
                move |(_, relpath)| !is_ignored(&ignore_matcher, relpath)
            })
            .filter_ok({
                let old_entries = old_entries.clone();
                move |(_, relpath)| !old_entries.contains_key(relpath)
            });
        let metadata = self.metadata.clone();

        let results_iter = if let Some(ref bar) = progress_bar {
            let discovery_bar = bar.clone();
            let progress_bar = progress_bar.clone();
            crate::parallel::for_each_with_discovery_callback(
                iterator,
                move |res| {
                    let result = res.and_then(|(entry, relative_path)| {
                        checksum_entry(metadata.algo, entry, relative_path)
                    });
                    if let Some(ref bar) = progress_bar {
                        bar.notify_record_processed(result.as_ref().map(|(size, _, _)| *size).ok());
                    }
                    result
                },
                Some(Box::new(move || {
                    discovery_bar.notify_file_discovered();
                })),
            )
        } else {
            crate::parallel::for_each(iterator, move |res| {
                res.and_then(|(entry, relative_path)| {
                    checksum_entry(metadata.algo, entry, relative_path)
                })
            })
        };

        let results_iter = if let Some(ref bar) = progress_bar {
            let bar_clone = bar.clone();
            results_iter.with_total_callback(move |total| {
                bar_clone.set_length(total);
            })
        } else {
            results_iter
        };

        for result in results_iter {
            let (_, relative_filename, signature) = result?;
            let prev = new_entries.insert(relative_filename, signature);
            assert!(prev.is_none(), "Entry was already in catalog!")
        }

        assert!(self.entries.is_empty());
        assert_eq!(Arc::strong_count(&old_entries), 1);
        std::mem::swap(Arc::make_mut(&mut old_entries), &mut self.entries);
        // now self.entries is back to what it used to be

        if self.entries.is_empty() {
            std::mem::swap(&mut self.entries, &mut new_entries);
        } else {
            self.entries.extend(new_entries);
        }

        Ok(())
    }

    pub fn write_signature_file(&self, allow_existing: bool) -> anyhow::Result<()> {
        let mut open_options = std::fs::OpenOptions::new();
        if allow_existing {
            open_options.create(true).write(true).truncate(true);
        } else {
            open_options.create_new(true).write(true);
        };
        let mut sigfile = open_options
            .open(self.metadata.signature_file_path.as_path())
            .with_context(|| {
                format!(
                    "Failed opening signature file {:?} for writing",
                    self.metadata.signature_file_path.as_path()
                )
            })?;

        for (subpath, sig) in self.entries.iter() {
            let encoded = hex::encode(sig);
            writeln!(&mut sigfile, "{encoded} *{subpath}")?;
        }

        Ok(())
    }
}

pub struct Entry {
    hash: Vec<u8>,
    path: Arc<CanonicalPath<PathBuf>>,
}

impl Entry {
    pub(crate) fn verify(&self, algo: Algorithm) -> anyhow::Result<crate::reporting::ReportEntry> {
        let (size, hash) = match algo
            .hash_file(self.path.as_path())
            .with_context(|| format!("Failed hashing {:?}", self.path))
        {
            Ok(x) => x,
            Err(e) => {
                if let Some(e) = e.downcast_ref::<std::io::Error>() {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        log::info!("{:?} is missing!", self.path);
                        return Ok(crate::reporting::ReportEntry::new(
                            self.path.clone(),
                            0,
                            EntryStatus::Missing,
                        ));
                    }
                }
                return Err(e);
            }
        };

        Ok(crate::reporting::ReportEntry::new(
            self.path.clone(),
            size,
            if hash == self.hash {
                EntryStatus::Ok
            } else {
                EntryStatus::VerificationError
            },
        ))
    }

    /// Checks only whether the entry's file exists on disk, without reading or
    /// hashing its contents. Yields `Ok` when a regular file is present and
    /// `Missing` otherwise (including when the path was replaced by a directory
    /// or other non-file). Metadata is resolved through symlinks, matching how
    /// the hashing path opens the file.
    pub(crate) fn verify_existence(&self) -> anyhow::Result<crate::reporting::ReportEntry> {
        let status = match std::fs::metadata(self.path.as_path()) {
            Ok(metadata) if metadata.is_file() => EntryStatus::Ok,
            Ok(_) => {
                log::info!("{:?} exists but is not a regular file!", self.path);
                EntryStatus::Missing
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::info!("{:?} is missing!", self.path);
                EntryStatus::Missing
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("Failed checking existence of {:?}", self.path)
                });
            }
        };

        Ok(crate::reporting::ReportEntry::new(
            self.path.clone(),
            0,
            status,
        ))
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Catalog {
    pub(crate) fn metadata(&self) -> &CatalogMetadata {
        &self.metadata
    }

    pub fn update_entry(&mut self, relative_path: &str, hash: Vec<u8>) {
        self.entries.insert(relative_path.to_string(), hash);
    }

    pub fn remove_entry(&mut self, relative_path: &str) {
        self.entries.remove(relative_path);
    }

    pub fn directory(&self) -> &Directory {
        &self.directory
    }

}

impl IntoIterator for Catalog {
    type Item = Entry;
    type IntoIter = Box<dyn Iterator<Item = Entry> + Send>;

    fn into_iter(self) -> Self::IntoIter {
        let root_path = self.directory.path;

        Box::new(self.entries.into_iter().map(move |(subpath, hash)| Entry {
            path: Arc::new(root_path.as_path().join(subpath).assume_canonical()),
            hash,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{is_ignored, Directory, IGNORE_FILE_NAME};
    use assert_fs::prelude::*;

    /// Builds a matcher from an ignore file written into a fresh temp directory.
    /// The ignore file is read eagerly during `load_ignore_matcher`, and
    /// matching relative paths never touches the filesystem, so the temp
    /// directory can be dropped as soon as the matcher is built.
    fn matcher_from(contents: &str) -> ignore::gitignore::Gitignore {
        let temp = assert_fs::TempDir::new().unwrap();
        temp.child(IGNORE_FILE_NAME).write_str(contents).unwrap();
        let directory = Directory::new(temp.path()).unwrap();
        let matcher = directory.load_ignore_matcher().unwrap();
        std::sync::Arc::try_unwrap(matcher).unwrap_or_else(|arc| (*arc).clone())
    }

    #[test]
    fn bare_name_matches_any_depth() {
        let matcher = matcher_from("1\n");
        assert!(is_ignored(&matcher, "1"));
        assert!(is_ignored(&matcher, "a/1"));
        assert!(!is_ignored(&matcher, "2"));
        assert!(!is_ignored(&matcher, "a/2"));
    }

    #[test]
    fn glob_matches_any_depth() {
        let matcher = matcher_from("*.txt\n");
        assert!(is_ignored(&matcher, "x.txt"));
        assert!(is_ignored(&matcher, "a/x.txt"));
        assert!(!is_ignored(&matcher, "x.log"));
        assert!(!is_ignored(&matcher, "a/x.log"));
    }

    #[test]
    fn leading_slash_anchors_to_root_and_is_not_absolute() {
        // "/a/1" is anchored to the directory root, NOT the filesystem root.
        let matcher = matcher_from("/a/1\n");
        assert!(is_ignored(&matcher, "a/1"));
        // A same-named file deeper in the tree must NOT be excluded.
        assert!(!is_ignored(&matcher, "sub/a/1"));
        // The relative path is matched as-is (relative), never as an absolute
        // "/a/1" against the real filesystem.
        assert!(!is_ignored(&matcher, "b/1"));
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let matcher = matcher_from("# a comment\n\n2\n");
        assert!(is_ignored(&matcher, "2"));
        // "# a comment" must not be treated as a pattern.
        assert!(!is_ignored(&matcher, "# a comment"));
        assert!(!is_ignored(&matcher, "1"));
    }

    #[test]
    fn ignore_file_is_self_excluded_even_when_empty() {
        let matcher = matcher_from("");
        assert!(is_ignored(&matcher, IGNORE_FILE_NAME));
    }

    #[test]
    fn missing_ignore_file_excludes_nothing_but_itself() {
        let temp = assert_fs::TempDir::new().unwrap();
        let directory = Directory::new(temp.path()).unwrap();
        let matcher = directory.load_ignore_matcher().unwrap();
        assert!(is_ignored(&matcher, IGNORE_FILE_NAME));
        assert!(!is_ignored(&matcher, "a/1"));
        assert!(!is_ignored(&matcher, "c"));
    }
}
