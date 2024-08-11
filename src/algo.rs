use anyhow::Context;
use digest::Digest;
use sha3;
use blake3;
use std::path::Path;
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

#[derive(IntoStaticStr, EnumString, EnumIter, strum::Display, Clone, Copy)]
#[strum(serialize_all = "snake_case")]
pub enum Algorithm {
    Sha256,
    Sha512,
    Blake3,
}

impl Algorithm {
    pub fn try_deduce_from_path(root: &Path) -> Option<Self> {
        let root_dir_name = root.file_name()?.to_string_lossy();
        Self::iter().find(|variant| {
            let path = root.join(format!("{}.{}", root_dir_name, variant));
            log::debug!("Searching for {path:?}...");
            path.exists()
        })
    }
}

impl Algorithm {
    pub fn hash_file(&self, p: impl AsRef<Path>) -> anyhow::Result<(u64, Vec<u8>)> {
        let path = p.as_ref();
        let mut file =
            std::fs::File::open(path).with_context(|| format!("{path:?}: Failed opening file"))?;

        match self {
            Algorithm::Sha256 => {
                let mut hasher = sha3::Sha3_256::new();
                let res = std::io::copy(&mut file, &mut hasher)?;
                return Ok((res, Vec::from(hasher.finalize().as_slice())))
            }
            Algorithm::Sha512 => {
                let mut hasher = sha3::Sha3_512::new();
                let res = std::io::copy(&mut file, &mut hasher)?;
                return Ok((res, Vec::from(hasher.finalize().as_slice())))
            }
            Algorithm::Blake3 => {
                let mut hasher = blake3::Hasher::new();
                let res = std::io::copy(&mut file, &mut hasher)?;
                return Ok((res, Vec::from(hasher.finalize().as_bytes())))
            }
        };
    }
}
