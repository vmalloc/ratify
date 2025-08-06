use anyhow::Context;
use blake3::Hasher as Blake3;
use digest::Digest;
use md5::Md5;
use sha1::Sha1;
use sha2::Sha256;
use sha2::Sha512;
use std::path::Path;
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

#[derive(IntoStaticStr, EnumString, EnumIter, strum::Display, Clone, Copy)]
#[strum(serialize_all = "snake_case")]
pub enum Algorithm {
    Blake3,
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl Algorithm {
    pub fn try_deduce_from_path(root: &Path) -> Option<Self> {
        let root_dir_name = root.file_name()?.to_string_lossy();
        Self::iter().find(|variant| {
            let path = root.join(format!("{root_dir_name}.{variant}"));
            log::debug!("Searching for {path:?}...");
            path.exists()
        })
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
