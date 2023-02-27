//! Library for `cargo-indicate`, providing a way to query dependencies across
//! different sources of information such as crates.io metadata, GitHub etc.
//!
//! Queries are written using [`trustfall`], a query engine for writing queries
//! across data sources. Currently only GraphQL-like schemas are available. The
//! following is the schema used that can be used to construct queries. Note
//! that only the directives provided here can be used.
//!
//! # Schema
//! _The following code is automatically included from the
//! `src/schema.trustfall.graphql` file_
//! ```graphql
#![doc = include_str!("schema.trustfall.graphql")]
//! ```
#![deny(unsafe_code)]
use std::{
    cell::RefCell,
    collections::BTreeMap,
    error::Error,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use adapter::IndicateAdapter;
use cargo_metadata::{Metadata, MetadataCommand};
use errors::FileParseError;
use once_cell::sync::Lazy;
use serde::Deserialize;
use tokio::runtime::Runtime;
use trustfall::{
    execute_query as trustfall_execute_query, FieldValue, Schema,
    TransparentValue,
};

mod adapter;
pub mod errors;
mod repo;
mod vertex;

const RAW_SCHEMA: &str = include_str!("schema.trustfall.graphql");

static SCHEMA: Lazy<Schema> =
    Lazy::new(|| Schema::parse(RAW_SCHEMA).expect("Could not parse schema!"));

/// async tokio runtime to be able to resolve `async` API client libraries
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("could not create tokio runtime")
});

/// Type representing a thread-safe JSON object, like
/// ```json
/// {
///     "name": "hello",
///     "value": true,
/// }
/// ```
type ObjectMap = BTreeMap<Arc<str>, FieldValue>;

#[derive(Debug, Clone, Deserialize)]
pub struct Query {
    pub query: String,
    pub args: ObjectMap,
}

impl Query {
    /// Extracts a query from a file
    pub fn from_path(path: &Path) -> Result<Query, Box<dyn Error>> {
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
                    let q = ron::from_str::<Query>(&raw_query)?;
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

/// Transform a result from [`execute_query`] to one where the fields can easily be
/// serialized to JSON using [`TransparentValue`].
pub fn transparent_results(
    res: Vec<BTreeMap<Arc<str>, FieldValue>>,
) -> Vec<BTreeMap<Arc<str>, TransparentValue>> {
    res.into_iter()
        .map(|entry| entry.into_iter().map(|(k, v)| (k, v.into())).collect())
        .collect()
}

/// Executes a Trustfall query at a defined path, using the schema
/// provided by `indicate`.
pub fn execute_query(
    query: &Query,
    metadata: &Metadata,
) -> Vec<BTreeMap<Arc<str>, FieldValue>> {
    let adapter = Rc::new(RefCell::new(IndicateAdapter::new(metadata)));
    let res = match trustfall_execute_query(
        &SCHEMA,
        adapter,
        query.query.as_str(),
        query.args.clone(),
    ) {
        Ok(res) => res.collect(),
        Err(e) => panic!("Could not execute query due to error: {:#?}", e),
    };
    res
}

/// Extracts metadata from a `Cargo.toml` file by its direct path, or the path
/// of its directory
pub fn extract_metadata_from_path(
    path: &Path,
) -> Result<Metadata, Box<dyn Error>> {
    if path.is_file() {
        let m = MetadataCommand::new().manifest_path(path).exec()?;
        Ok(m)
    } else if path.is_dir() {
        let mut assumed_path = PathBuf::from(path);
        assumed_path.push("Cargo.toml");
        let m = MetadataCommand::new().manifest_path(assumed_path).exec()?;
        Ok(m)
    } else {
        Err(Box::new(FileParseError::NotFound(
            path.to_string_lossy().to_string(),
        )))
    }
}

#[cfg(test)]
mod test {
    // use lazy_static::lazy_static;
    use core::panic;
    use std::{fs, path::Path};
    use test_case::test_case;

    use crate::{
        execute_query, extract_metadata_from_path, transparent_results, Query,
    };

    /// File that may never exist, to ensure some test work
    const NONEXISTENT_FILE: &'static str = "test_data/notafile";

    #[test]
    fn non_existant_file() {
        assert!(!Path::new(NONEXISTENT_FILE).exists());
    }

    #[test_case("direct_dependencies", "direct_dependencies" ; "direct dependencies as listed in Cargo.toml")]
    #[test_case("direct_dependencies", "no_deps_all_fields" ; "retrieving all fields of root package, but not dependencies")]
    #[test_case("direct_dependencies", "dependency_package_info" ; "information about root package direct dependencies")]
    #[test_case("direct_dependencies", "recursive_dependency" ; "retrieve recursive dependency information")]
    #[test_case("direct_dependencies", "count_dependencies" ; "count the number of dependencies used by each dependency")]
    #[test_case("direct_dependencies", "github_simple" => ignore["don't use GitHub API rate limits in tests"]; "simple GitHub repository query")]
    #[test_case("direct_dependencies", "github_owner" => ignore["don't use GitHub API rate limits in tests"]; "retrieve the owner of a GitHub repository")]
    fn query_tests(fake_crate: &str, query_name: &str) {
        let raw_cargo_toml_path =
            format!("test_data/fake_crates/{fake_crate}/Cargo.toml");
        let cargo_toml_path = Path::new(&raw_cargo_toml_path);

        let raw_query_path = format!("test_data/queries/{query_name}.in.ron");
        let query_path = Path::new(&raw_query_path);

        let raw_expected_result_path =
            format!("test_data/queries/{query_name}.expected.json");
        let expected_result_name = Path::new(&raw_expected_result_path);

        // We use `TransparentValue for neater JSON serialization
        let res = transparent_results(execute_query(
            &Query::from_path(query_path).unwrap(),
            &extract_metadata_from_path(cargo_toml_path).unwrap(),
        ));
        let res_json_string = serde_json::to_string_pretty(&res)
            .expect("Could not convert result to string");

        let expected_result_string = fs::read_to_string(expected_result_name)
            .unwrap_or_else(|_| {
                panic!(
                    "Could not read expected file '{}'",
                    expected_result_name.to_string_lossy()
                );
            });

        assert_eq!(
            res_json_string.trim(),
            expected_result_string.trim(),
            "\nfailing query result:\n{}\n but expected:\n{}\n",
            res_json_string,
            expected_result_string
        );
    }

    #[test_case("test_data/fake_crates/direct_dependencies" ; "extract from directory")]
    #[test_case("test_data/fake_crates/direct_dependencies/Cargo.toml" ; "extract from direct path")]
    #[test_case(NONEXISTENT_FILE => panics "does not exist" ; "extract from directory without Cargo.toml")]
    fn extract_metadata(path_str: &str) {
        let m = extract_metadata_from_path(Path::new(path_str));
        match m {
            Ok(_) => return,
            Err(b) => panic!("{}", b),
        }
    }

    #[test_case("test_data/queries/no_deps_all_fields.in.ron" ; "extract ron file")]
    #[test_case(NONEXISTENT_FILE => panics "does not exist" ; "extracting nonexistent file")]
    fn extract_query(path_str: &str) {
        let q = Query::from_path(Path::new(path_str));
        match q {
            Ok(_) => return,
            Err(b) => panic!("{}", b),
        }
    }
}
