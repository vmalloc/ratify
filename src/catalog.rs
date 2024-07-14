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

pub struct Directory {
    path: CanonicalPath<PathBuf>,
}

impl Directory {
    pub fn new(path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = std::fs::canonicalize(path.into())
            .context("Failed canonicalizing path")?
            .assume_canonical();

        Ok(Self { path })
    }

    pub fn load(self, algo: Option<Algorithm>) -> anyhow::Result<Catalog> {
        let algo = algo
            .or_else(|| Algorithm::try_deduce_from_path(self.path.as_path()))
            .ok_or_else(|| anyhow::format_err!("Failed to detect signature file"))?;
        let filename = self.signature_file_path(algo);

        log::debug!("Opening signature file {filename:?}...");
        let file = std::io::BufReader::new(
            std::fs::File::open(filename.as_path())
                .with_context(|| format!("Failed opening {:?}", self.path))?,
        );
        let mut entries = BTreeMap::new();

        for (lineno, line) in file.lines().enumerate().map(|(lineno, l)| (lineno + 1, l)) {
            let line = line.context("Cannot read file")?;
            let (hash, entry_path) = line.split_once(" *").ok_or_else(|| {
                anyhow::anyhow!("Syntax error at line {} of {:?}", lineno, self.path)
            })?;

            let entry = hex::decode(hash)
                .map_err(|_| anyhow::anyhow!("Failed parsing line {lineno}: invalid hash"))?;

            let prev = entries.insert(entry_path.to_string(), entry);
            if prev.is_some() {
                anyhow::bail!(
                    "Entry {entry_path:?} appears multiple times in {:?}",
                    self.path
                );
            }
        }
        let metadata = Arc::new(self.catalog_metadata(algo));

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
        let abs_root = &self.path;
        let file_name = abs_root
            .as_path()
            .file_name()
            .map(|x| x.to_string_lossy())
            .unwrap_or_else(|| "signatures".into());
        format!("{}.{}", file_name, algo)
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
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

    pub(crate) fn empty_catalog(self, algo: Algorithm) -> Catalog {
        let metadata = Arc::new(self.catalog_metadata(algo));
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
    pub fn populate(&mut self) -> anyhow::Result<()> {
        let mut new_entries = BTreeMap::new();
        let mut old_entries = Arc::new(std::mem::take(&mut self.entries));

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
                let old_entries = old_entries.clone();
                move |(_, relpath)| !old_entries.contains_key(relpath)
            });
        let metadata = self.metadata.clone();
        for result in crate::parallel::for_each(iterator, move |res| {
            res.and_then(|(entry, relative_path)| {
                checksum_entry(metadata.algo, entry, relative_path)
            })
        }) {
            let (entry, relative_filename, signature) = result?;
            let prev = new_entries.insert(relative_filename, signature);
            assert!(prev.is_none(), "Entry {:?} was already in catalog!", entry)
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
        let mut sigfile = std::fs::OpenOptions::new()
            .create_new(!allow_existing)
            .write(true)
            .truncate(false)
            .open(self.metadata.signature_file_path.as_path())
            .with_context(|| {
                format!(
                    "Failed opening signature file {:?} for writing",
                    self.metadata.signature_file_path.as_path()
                )
            })?;

        for (subpath, sig) in self.entries.iter() {
            let encoded = hex::encode(sig);
            writeln!(&mut sigfile, "{} *{}", encoded, subpath)?;
        }

        Ok(())
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.entries.len()
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

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

impl Catalog {
    pub(crate) fn metadata(&self) -> &CatalogMetadata {
        &self.metadata
    }

    pub(crate) fn into_iter(self) -> impl Iterator<Item = Entry> {
        let root_path = self.directory.path;

        self.entries.into_iter().map(move |(subpath, hash)| Entry {
            path: Arc::new(root_path.as_path().join(subpath).assume_canonical()),
            hash,
        })
    }
}
