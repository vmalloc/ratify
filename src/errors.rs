#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum Error {
    #[error("Failed entries found")]
    Failed,

    #[error("Missing entries found")]
    Missing,

    #[error("Unknown entries found")]
    Unknown,
}
