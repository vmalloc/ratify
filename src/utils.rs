use std::path::{Path, PathBuf};

use anyhow::Context;

#[derive(PartialEq, Eq, Hash, Clone)]
pub(crate) struct CanonicalPath<T>(T);

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

/// Ensures the terminal leaves raw mode even if reading input errors or panics.
struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> anyhow::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

pub(crate) fn read_single_char() -> anyhow::Result<char> {
    use crossterm::event::{self, Event, KeyCode};

    let _guard = RawModeGuard::enable()?;

    loop {
        match event::read()? {
            Event::Key(key_event) => {
                let ch = match key_event.code {
                    KeyCode::Char(c) => c,
                    _ => continue,
                };
                break Ok(ch);
            }
            _ => continue,
        }
    }
}
