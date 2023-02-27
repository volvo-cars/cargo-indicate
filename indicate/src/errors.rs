use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum QueryParseError {
    #[error(
        "file extension `{ext:?}` is not supported for file path `{path:?}`"
    )]
    UnsupportedFileExtension { ext: String, path: String },

    #[error("could not determine file extension of file path `{0}`")]
    UnknownFileExtension(String),
}

#[derive(Error, Debug, Clone)]
pub enum MetadataParseError {
    #[error("could not extract file path from path `{0}`")]
    NotFound(String),
}
