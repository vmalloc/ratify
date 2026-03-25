use anyhow::Context;
use blake3::Hasher as Blake3;
use digest::Digest;
use md5::Md5;
use sha1::Sha1;
use sha2::Sha256;
use sha2::Sha512;
use std::path::{Path, PathBuf};
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

#[derive(
    Debug,
    PartialEq,
    Eq,
    IntoStaticStr,
    EnumString,
    EnumIter,
    strum::Display,
    Clone,
    Copy,
    serde::Serialize,
    serde::Deserialize,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    Blake3,
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    pub fn try_deduce_from_path(root: &Path) -> Option<(Self, PathBuf)> {
        let root_dir_name = root.file_name()?.to_string_lossy();
        for variant in Self::iter() {
            let ratify_path = root.join(format!("ratify-catalog.{variant}"));
            log::debug!("Searching for {ratify_path:?}...");
            if ratify_path.exists() {
                return Some((variant, ratify_path));
            }
            let old_ratify_path = root.join(format!("ratify.{variant}"));
            log::debug!("Searching for {old_ratify_path:?}...");
            if old_ratify_path.exists() {
                return Some((variant, old_ratify_path));
            }
            let legacy_path = root.join(format!("{root_dir_name}.{variant}"));
            log::debug!("Searching for {legacy_path:?}...");
            if legacy_path.exists() {
                return Some((variant, legacy_path));
            }
        }
        None
    }

    pub fn try_deduce_from_file(file_path: &Path) -> Option<Self> {
        let file_name = file_path.file_name()?.to_string_lossy();
        if let Some(extension) = file_name.split('.').next_back() {
            Self::iter().find(|variant| {
                extension.eq_ignore_ascii_case(&variant.to_string())
            })
        } else {
            None
        }
    }
}

macro_rules! hash_impl {
    ($self:expr, $file:expr, $($algo:tt),*) => {
        match $self {
            $(Self::$algo => {

                let mut hasher = $algo::new();
                let res = std::io::copy($file, &mut hasher)?;
                std::io::Result::Ok((res, Vec::from(hasher.finalize().as_slice())))
            }),*
        }
    };
}

impl Algorithm {
    pub fn hash_file(&self, p: impl AsRef<Path>) -> anyhow::Result<(u64, Vec<u8>)> {
        let path = p.as_ref();
        let mut file =
            std::fs::File::open(path).with_context(|| format!("{path:?}: Failed opening file"))?;

        let (size, data) = hash_impl!(self, &mut file, Blake3, Md5, Sha1, Sha256, Sha512)?;
        Ok((size, data))
    }
}
