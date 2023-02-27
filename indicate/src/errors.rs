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
        "could not extract file path from path `{0}`: File does not exist"
    )]
    NotFound(String),
}
