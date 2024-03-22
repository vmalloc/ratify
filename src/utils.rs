use std::path::{Path, PathBuf};

use anyhow::Context;

#[derive(PartialEq, Eq, Hash)]
pub(crate) struct CanonicalPath<T>(T);

impl<T: Clone> Clone for CanonicalPath<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: AsRef<Path> + std::fmt::Debug + Clone> std::fmt::Debug for CanonicalPath<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_path().fmt(f)
    }
}

impl<T: AsRef<Path>> CanonicalPath<T> {
    pub(crate) fn as_path(&self) -> &Path {
        self.0.as_ref()
    }
}

pub(crate) trait Canonicalizeable: Sized {
    fn try_canonicalize(&self) -> anyhow::Result<CanonicalPath<PathBuf>>;

    fn assume_canonical(self) -> CanonicalPath<Self> {
        CanonicalPath(self)
    }
}

impl<T: AsRef<Path>> Canonicalizeable for T {
    fn try_canonicalize(&self) -> anyhow::Result<CanonicalPath<PathBuf>> {
        let p = self.as_ref();

        Ok(p.canonicalize()
            .with_context(|| format!("Failed to canonicalize path {p:?}"))?
            .assume_canonical())
    }
}
