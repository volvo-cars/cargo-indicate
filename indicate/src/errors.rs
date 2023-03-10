use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum FileParseError {
    #[error(
        "file extension `{ext:?}` is not supported for file path `{path:?}`"
    )]
    UnsupportedFileExtension { ext: String, path: String },

    #[error("could not determine file extension of file path `{0}`")]
    UnknownFileExtension(String),

    #[error(
        "could not extract file path from path `{0}`, file does not exist"
    )]
    NotFound(String),
}

#[derive(Error, Debug, Clone)]
pub enum GeigerError {
    #[error("geiger status code was not OK ({0}), stderr was: `{1}`")]
    NonZeroStatus(i32, String),

    #[error(
        "could not parse geiger output due to error `{0}`, stdout was: `{1}`"
    )]
    UnexpectedOutput(String, String),
}
