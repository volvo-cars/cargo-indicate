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
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use adapter::IndicateAdapter;
use cargo_metadata::{CargoOpt, Metadata, MetadataCommand};
use errors::FileParseError;
use once_cell::sync::Lazy;
use query::FullQuery;
use tokio::runtime::Runtime;
use trustfall::{execute_query as trustfall_execute_query, FieldValue, Schema};

mod adapter;
mod advisory;
pub mod errors;
pub mod query;
mod repo;
pub mod util;
mod vertex;

pub use adapter::adapter_builder;
pub use rustsec::advisory::Severity;
/// Valid platforms that can be provided to queries
pub use rustsec::platforms;

pub const RAW_SCHEMA: &str = include_str!("schema.trustfall.graphql");

/// Schema used for queries
/// ```graphql
#[doc = include_str!("schema.trustfall.graphql")]
/// ```
static SCHEMA: Lazy<Schema> =
    Lazy::new(|| Schema::parse(RAW_SCHEMA).expect("Could not parse schema!"));

/// async tokio runtime to be able to resolve `async` API client libraries
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("could not create tokio runtime")
});

/// Executes a Trustfall query at a defined path, using the schema
/// provided by `indicate`.
pub fn execute_query(
    query: &FullQuery,
    metadata: Metadata,
    max_results: Option<usize>,
) -> Vec<BTreeMap<Arc<str>, FieldValue>> {
    let adapter = IndicateAdapter::new(metadata);
    execute_query_with_adapter(query, adapter, max_results)
}

/// Executes a Trustfall query with a dedicated [`IndicateAdapter`]
///
/// Use when the default configuration does not provide enough control
pub fn execute_query_with_adapter(
    query: &FullQuery,
    adapter: IndicateAdapter,
    max_results: Option<usize>,
) -> Vec<BTreeMap<Arc<str>, FieldValue>> {
    let res = match trustfall_execute_query(
        &SCHEMA,
        Rc::new(RefCell::new(adapter)),
        query.query.as_str(),
        query.args.clone(),
    ) {
        Ok(res) => res.take(max_results.unwrap_or(usize::MAX)).collect(),
        Err(e) => panic!("Could not execute query due to error: {:#?}", e),
    };
    res
}

/// Extracts metadata from a `Cargo.toml` file by its direct path, or the path
/// of its directory
///
/// Optionally provide a list of features to be used when creating the metadata,
/// and if default features are to be included or not.
pub fn extract_metadata_from_path(
    path: &Path,
    default_features: bool,
    features: Option<Vec<String>>,
) -> Result<Metadata, Box<dyn Error>> {
    let mut m = MetadataCommand::new();
    if path.is_file() {
        m.manifest_path(path);
    } else if path.is_dir() {
        let mut assumed_path = PathBuf::from(path);
        assumed_path.push("Cargo.toml");
        m.manifest_path(assumed_path);
    } else {
        return Err(Box::new(FileParseError::NotFound(
            path.to_string_lossy().to_string(),
        )));
    };

    if !default_features {
        m.features(CargoOpt::NoDefaultFeatures);
    }

    if let Some(f) = features {
        m.features(CargoOpt::SomeFeatures(f));
    }

    let res = m.exec()?;
    Ok(res)
}

#[cfg(test)]
mod test {
    // use lazy_static::lazy_static;
    use core::panic;
    use std::{
        collections::BTreeMap,
        fs,
        path::{Path, PathBuf},
        sync::Arc,
    };
    use test_case::test_case;
    use trustfall::TransparentValue;

    use crate::{
        execute_query, extract_metadata_from_path, query::FullQuery,
        repo::github::GH_API_CALL_COUNTER, util::transparent_results,
    };

    /// File that may never exist, to ensure some test work
    const NONEXISTENT_FILE: &'static str = "test_data/notafile";

    /// Retrieve paths for the crate and query names provided, relative to
    /// `indicate` crate root
    fn get_paths<'a, 'b>(
        fake_crate_name: &'a str,
        query_name: &'b str,
    ) -> (PathBuf, PathBuf) {
        let raw_cargo_toml_path =
            format!("test_data/fake_crates/{fake_crate_name}/Cargo.toml");
        let cargo_toml_path = PathBuf::from(&raw_cargo_toml_path);

        let raw_query_path = format!("test_data/queries/{query_name}.in.ron");
        let query_path = PathBuf::from(&raw_query_path);

        (cargo_toml_path, query_path)
    }

    #[test]
    fn non_existant_file() {
        assert!(!Path::new(NONEXISTENT_FILE).exists());
    }

    /// Assert that a query results matches the results provided in a file
    fn assert_query_res(
        res: Vec<BTreeMap<Arc<str>, TransparentValue>>,
        expected_result_path: &Path,
    ) {
        let res_json_string = serde_json::to_string_pretty(&res)
            .expect("Could not convert result to string");

        let expected_result_string = fs::read_to_string(expected_result_path)
            .unwrap_or_else(|_| {
                panic!(
                    "Could not read expected file '{}'",
                    expected_result_path.to_string_lossy()
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

    /// Test that the queries complete (or panic), but do not check their results
    ///
    /// Used for results that may change over time.
    #[test_case("known_advisory_deps", "advisory_db_simple" ; "simple advisory db does not panic")]
    #[test_case("known_advisory_deps", "advisory_db_affected_funcs" ; "advisory db with affected functions does not panic")]
    #[test_case("known_advisory_deps", "advisory_db_no_include_withdrawn" => panics ; "advisory db without includeWithin panics")]
    #[test_case("known_advisory_deps", "advisory_db_with_parameters" ; "advisory db with parameters does not panic")]
    #[test_case("simple_deps", "github_simple" => ignore["don't use GitHub API rate limits in tests"]; "simple GitHub repository query")]
    #[test_case("simple_deps", "github_owner" => ignore["don't use GitHub API rate limits in tests"]; "retrieve the owner of a GitHub repository")]
    fn query_sanity_check(fake_crate_name: &str, query_name: &str) {
        let (cargo_toml_path, query_path) =
            get_paths(fake_crate_name, query_name);
        execute_query(
            &FullQuery::from_path(query_path.as_path()).unwrap(),
            extract_metadata_from_path(cargo_toml_path.as_path(), true, None)
                .unwrap(),
            None,
        );
    }

    #[test_case("simple_deps", "direct_dependencies" ; "direct dependencies as listed in Cargo.toml")]
    #[test_case("simple_deps", "no_deps_all_fields" ; "retrieving all fields of root package, but not dependencies")]
    #[test_case("simple_deps", "dependency_package_info" ; "information about root package direct dependencies")]
    #[test_case("simple_deps", "recursive_dependency" ; "retrieve recursive dependency information")]
    #[test_case("simple_deps", "count_dependencies" ; "count the number of dependencies used by each dependency")]
    fn query_test(fake_crate_name: &str, query_name: &str) {
        let (cargo_toml_path, query_path) =
            get_paths(fake_crate_name, query_name);
        let raw_expected_result_name =
            format!("test_data/queries/{query_name}.expected.json");
        let expected_result_path = Path::new(&raw_expected_result_name);

        // We use `TransparentValue for neater JSON serialization
        let res = transparent_results(execute_query(
            &FullQuery::from_path(query_path.as_path()).unwrap(),
            extract_metadata_from_path(cargo_toml_path.as_path(), true, None)
                .unwrap(),
            None,
        ));

        assert_query_res(res, expected_result_path);
    }

    /// Test dependencies based on the features used
    ///
    /// Relies on a naming scheme where the expected ends with
    /// `-<default_features>-<feat1>-<feat2>.expected.json` (features are sorted
    /// alphabetically, and not included if empty)
    #[test_case(true, vec![] ; "default features enabled")]
    #[test_case(false, vec![] ; "no features no dependencies")]
    #[test_case(false, vec!["a", "b"] ; "default features manually enabled")]
    #[test_case(false, vec!["c"] ; "no default features single dep")]
    #[test_case(false, vec!["d"] ; "no default features single dep via other dep")]
    #[test_case(true, vec!["a", "b"] ; "default features enabled together with manual")]
    #[test_case(false, vec!["a", "b", "c", "d"] ; "no default features all deps")]
    fn feature_query_test(default_features: bool, features: Vec<&'static str>) {
        let query_name = "list_direct_dependencies";
        let (cargo_toml_path, query_path) =
            get_paths("feature_deps", query_name);

        let mut sorted_features = features.to_owned();
        sorted_features.sort();

        let metadata = extract_metadata_from_path(
            cargo_toml_path.as_path(),
            default_features,
            Some(features.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
        )
        .expect("should be able to parse metadata");

        let mut raw_expected_result_name =
            format!("test_data/queries/{query_name}-{default_features}");

        if !sorted_features.is_empty() {
            raw_expected_result_name = format!(
                "{raw_expected_result_name}-{}",
                sorted_features.join("-")
            );
        }

        raw_expected_result_name.push_str(".expected.json");

        let expected_result_path = Path::new(&raw_expected_result_name);

        let res = transparent_results(execute_query(
            &FullQuery::from_path(&query_path).unwrap(),
            metadata,
            None,
        ));

        assert_query_res(res, expected_result_path);
    }

    #[test_case("test_data/fake_crates/simple_deps" ; "extract from directory")]
    #[test_case("test_data/fake_crates/simple_deps/Cargo.toml" ; "extract from direct path")]
    #[test_case(NONEXISTENT_FILE => panics "does not exist" ; "extract from directory without Cargo.toml")]
    fn extract_metadata(path_str: &str) {
        let m = extract_metadata_from_path(Path::new(path_str), true, None);
        match m {
            Ok(_) => return,
            Err(b) => panic!("{}", b),
        }
    }

    #[test_case("test_data/queries/no_deps_all_fields.in.ron" ; "extract ron file")]
    #[test_case(NONEXISTENT_FILE => panics "does not exist" ; "extracting nonexistent file")]
    fn extract_query(path_str: &str) {
        let q = FullQuery::from_path(Path::new(path_str));
        match q {
            Ok(_) => return,
            Err(b) => panic!("{}", b),
        }
    }

    #[test]
    #[ignore = "run in isolation"]
    fn max_results_limits_api_calls() {
        let q = FullQuery::from_path(Path::new(
            "test_data/queries/github_simple.in.ron",
        ))
        .unwrap();
        let metadata = extract_metadata_from_path(
            Path::new("test_data/fake_crates/direct_dependencies"),
            true,
            None,
        )
        .unwrap();
        let res = execute_query(&q, metadata, Some(1));
        assert_eq!(res.len(), GH_API_CALL_COUNTER.get())
    }
}
