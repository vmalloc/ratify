#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum Error {
    #[error("Failed entries found")]
    FailedEntriesFound,

    #[error("Missing entries found")]
    MissingEntriesFound,

    #[error("Unknown entries found")]
    UnknownEntriesFound,
}
