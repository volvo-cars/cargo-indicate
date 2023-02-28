use std::{
    collections::BTreeMap, error::Error, ffi::OsStr, fs, path::Path, sync::Arc,
};

use serde::Deserialize;
use trustfall::FieldValue;

use crate::errors::FileParseError;

/// Type representing a thread-safe JSON object, like
/// ```json
/// {
///     "name": "hello",
///     "value": true,
/// }
/// ```
type QueryArgs = BTreeMap<Arc<str>, FieldValue>;

#[derive(Debug, Clone, Deserialize)]
pub struct FullQuery {
    pub query: String,
    pub args: QueryArgs,
}

impl FullQuery {
    /// Extracts a query from a file
    pub fn from_path(path: &Path) -> Result<FullQuery, Box<dyn Error>> {
        if !path.exists() {
            Err(Box::new(FileParseError::NotFound(
                path.to_string_lossy().to_string(),
            )))
        } else {
            let raw_query = fs::read_to_string(path)?;
            match path.extension().and_then(OsStr::to_str) {
                // TODO: Add support for other file types
                // Some("json") => {
                //     let q: Query = serde_json::from_str::<Query>(&raw_query)?;
                //     Ok(q)
                // }
                Some("ron") => {
                    let q = ron::from_str::<FullQuery>(&raw_query)?;
                    Ok(q)
                }
                Some(ext) => {
                    Err(Box::new(FileParseError::UnsupportedFileExtension {
                        ext: String::from(ext),
                        path: path.to_string_lossy().to_string(),
                    }))
                }
                None => Err(Box::new(FileParseError::UnknownFileExtension(
                    path.to_string_lossy().to_string(),
                ))),
            }
        }
    }
}

pub struct FullQueryBuilder {
    query: String,
    args: Option<QueryArgs>,
}

impl FullQueryBuilder {
    pub fn new(query: String) -> Self {
        Self { query, args: None }
    }

    pub fn query(mut self, query: String) -> Self {
        self.query = query;
        self
    }

    pub fn args(mut self, args: QueryArgs) -> Self {
        self.args = Some(args);
        self
    }

    pub fn build(self) -> FullQuery {
        FullQuery {
            query: self.query,
            args: self.args.unwrap_or(BTreeMap::new()),
        }
    }
}
