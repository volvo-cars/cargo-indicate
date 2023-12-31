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
#![forbid(unsafe_code)]
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

use cargo_metadata::Package;
use once_cell::sync::Lazy;
use query::FullQuery;
use rustsec::Version;
use serde::Deserialize;
use tokio::runtime::Runtime;
use trustfall::{execute_query as trustfall_execute_query, FieldValue, Schema};

pub mod adapter;
pub mod advisory;
pub mod code_stats;
pub mod crates_io;
pub mod errors;
pub mod geiger;
pub mod manifest;
pub mod query;
pub mod repo;
pub mod util;
mod vertex;

/// Features to create metadata with
pub use cargo_metadata::CargoOpt;
pub use rustsec::advisory::Severity;
/// Valid platforms that can be provided to queries
pub use rustsec::platforms;
pub use tokei;

pub use crate::adapter::adapter_builder::IndicateAdapterBuilder;
pub use crate::adapter::IndicateAdapter;
pub use crate::manifest::ManifestPath;

pub const RAW_SCHEMA: &str = include_str!("schema.trustfall.graphql");

/// Schema used for queries
/// ```graphql
#[doc = include_str!("schema.trustfall.graphql")]
/// ```
static SCHEMA: Lazy<Schema> = Lazy::new(|| {
    Schema::parse(RAW_SCHEMA)
        .unwrap_or_else(|e| panic!("Could not parse schema due to error: {e}"))
});

/// async tokio runtime to be able to resolve `async` API client libraries
static RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("could not create tokio runtime")
});

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
pub struct NameVersion {
    pub name: String,
    pub version: Version,
    // Other fields ignored, assume crates.io registry
}

impl NameVersion {
    #[must_use]
    pub fn new(name: String, version: Version) -> Self {
        Self { name, version }
    }
}

impl<T> From<T> for NameVersion
where
    T: AsRef<Package>,
{
    fn from(value: T) -> Self {
        fn inner(package: &Package) -> NameVersion {
            NameVersion {
                name: package.name.clone(),
                version: package.version.clone(),
            }
        }
        inner(value.as_ref())
    }
}

/// Executes a Trustfall query at a defined path, using the schema
/// provided by `indicate`
///
/// Will assume sane defaults for the adapter, such as enabling default features
/// when resolving metadata.
///
/// If multiple queries are to be resolved using the same adapter,
/// [`execute_query_with_adapter`] can be used instead.
#[must_use]
pub fn execute_query(
    query: &FullQuery,
    manifest_path: ManifestPath,
    max_results: Option<usize>,
) -> Vec<BTreeMap<Arc<str>, FieldValue>> {
    let adapter = IndicateAdapter::new(manifest_path);
    execute_query_with_adapter(query, Rc::new(adapter), max_results)
}

/// Executes a Trustfall query with a dedicated [`IndicateAdapter`], that may
/// be reused
///
/// Use when the default configuration does not provide enough control.
///
/// # Panics
///
/// Panics if the query could not be executed.
pub fn execute_query_with_adapter(
    query: &FullQuery,
    adapter: Rc<IndicateAdapter>,
    max_results: Option<usize>,
) -> Vec<BTreeMap<Arc<str>, FieldValue>> {
    let res = match trustfall_execute_query(
        &SCHEMA,
        adapter,
        query.query.as_str(),
        query.args.clone(),
    ) {
        Ok(res) => res.take(max_results.unwrap_or(usize::MAX)).collect(),
        Err(e) => panic!(
            "Could not execute query due to error: {e:#?}, query was: {query:#?}"
        ),
    };
    res
}

#[cfg(test)]
mod test {
    // use lazy_static::lazy_static;
    use cargo_metadata::CargoOpt;
    use core::panic;
    use std::{
        collections::BTreeMap,
        fs,
        path::{Path, PathBuf},
        rc::Rc,
        sync::Arc,
    };
    use test_case::test_case;
    use trustfall::TransparentValue;

    use crate::{
        adapter::IndicateAdapter, advisory::AdvisoryClient,
        execute_query_with_adapter, query::FullQuery,
        repo::github::GH_API_CALL_COUNTER, util::transparent_results,
        IndicateAdapterBuilder, ManifestPath,
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

    /// Crates an [`IndicateAdapter`] that is usable in tests
    ///
    /// Passing `None` features will use default features.
    fn test_adapter(
        manifest_path: ManifestPath,
        features: Option<Vec<CargoOpt>>,
    ) -> Rc<IndicateAdapter> {
        let mut b = IndicateAdapterBuilder::new(manifest_path).advisory_client(
            AdvisoryClient::from_default_path()
                .unwrap_or_else(|_| AdvisoryClient::new().unwrap()),
        );

        if let Some(f) = features {
            b = b.features(f);
        }

        Rc::new(b.build())
    }

    #[test]
    fn non_existant_file() {
        assert!(!Path::new(NONEXISTENT_FILE).exists());
    }

    #[test_case("./")]
    #[test_case("./Cargo.toml")]
    fn manifest_path_smoke_test(path_str: &'static str) {
        let res = ManifestPath::from(path_str);
        assert!(res.as_path().ends_with("Cargo.toml"))
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
        let manifest_path = ManifestPath::new(&cargo_toml_path);
        execute_query_with_adapter(
            &FullQuery::from_path(query_path.as_path()).unwrap(),
            test_adapter(manifest_path, None),
            None,
        );
    }

    #[test_case("simple_deps", "direct_dependencies" ; "direct dependencies as listed in Cargo.toml")]
    #[test_case("simple_deps", "no_deps_all_fields" ; "retrieving all fields of root package, but not dependencies")]
    #[test_case("simple_deps", "dependency_package_info" ; "information about root package direct dependencies")]
    #[test_case("simple_deps", "recursive_dependency" ; "retrieve recursive dependency information")]
    #[test_case("simple_deps", "count_dependencies" ; "count the number of dependencies used by each dependency")]
    #[test_case("forbids_unsafe", "geiger_forbids_unsafe")]
    #[test_case("forbids_unsafe", "geiger_total_percentage")]
    #[test_case("unsafe_crate", "geiger_advanced" => inconclusive["cargo-geiger --features flag broken, see https://github.com/rust-secure-code/cargo-geiger/issues/379"])]
    #[test_case("simple_deps", "dependencies_all_fields" ; "retrieve all fields of all dependencies")]
    #[test_case("simple_deps", "dependencies_all_fields_include_root" ; "retrieve all fields of all dependencies including root package")]
    #[test_case("dev_deps", "dev_dependencies_excluded" ; "dev-dependencies excluded in dep resolution when using Dependencies entry point")]
    #[test_case("dev_deps", "dev_dependencies_excluded_w_root_package" ; "dev-dependencies excluded in dep resolution when using RootPackage entry point")]
    #[test_case("transitive_deps", "list_transitive_dependencies" ; "list only transitive dependencies")]
    #[test_case("simple_deps", "code_stats_simple")]
    #[test_case("simple_deps", "all_deps_code_stats")]
    #[test_case("simple_deps", "all_deps_code_stats_only_src")]
    fn query_test(fake_crate_name: &str, query_name: &str) {
        let (cargo_toml_path, query_path) =
            get_paths(fake_crate_name, query_name);
        let raw_expected_result_name =
            format!("test_data/queries_expected/{query_name}.expected.json");
        let expected_result_path = Path::new(&raw_expected_result_name);

        // We use `TransparentValue for neater JSON serialization
        let res = transparent_results(execute_query_with_adapter(
            &FullQuery::from_path(query_path.as_path()).unwrap(),
            test_adapter(ManifestPath::new(&cargo_toml_path), None),
            None,
        ));

        assert_query_res(res, expected_result_path);
    }

    /// Test dependencies based on the features used
    ///
    /// Relies on a naming scheme where the expected ends with
    /// `-<default_features>-<feat1>-<feat2>.expected.json` (features are sorted
    /// alphabetically, and not included if empty)
    #[test_case("feature_deps", "list_direct_dependencies", true, vec![] ; "default features enabled")]
    #[test_case("feature_deps", "list_direct_dependencies", false, vec![] ; "no features no dependencies")]
    #[test_case("feature_deps", "list_direct_dependencies", false, vec!["a", "b"] ; "default features manually enabled")]
    #[test_case("feature_deps", "list_direct_dependencies", false, vec!["c"] ; "no default features single dep")]
    #[test_case("feature_deps", "list_direct_dependencies", false, vec!["d"] ; "no default features single dep via other dep")]
    #[test_case("feature_deps", "list_direct_dependencies", true, vec!["a", "b"] ; "default features enabled together with manual")]
    #[test_case("feature_deps", "list_direct_dependencies", false, vec!["a", "b", "c", "d"] ; "no default features all deps")]
    #[test_case("unsafe_crate", "geiger_advanced", false, vec!["crazy_unsafe"] => inconclusive["cargo-geiger and libc disagrees, see https://github.com/rust-secure-code/cargo-geiger/issues/447"] ; "dangerous feature increases geiger unsafety")]
    fn feature_query_test(
        fake_crate_name: &str,
        query_name: &str,
        default_features: bool,
        features: Vec<&'static str>,
    ) {
        let (cargo_toml_path, query_path) =
            get_paths(fake_crate_name, query_name);

        let mut sorted_features = features.to_owned();
        sorted_features.sort();

        let mut features = vec![CargoOpt::SomeFeatures(
            features
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>(),
        )];

        if !default_features {
            features.push(CargoOpt::NoDefaultFeatures);
        }

        let manifest_path = ManifestPath::new(&cargo_toml_path);

        let mut raw_expected_result_name = format!(
            "test_data/queries_expected/{query_name}-{default_features}"
        );

        if !sorted_features.is_empty() {
            raw_expected_result_name = format!(
                "{raw_expected_result_name}-{}",
                sorted_features.join("-")
            );
        }

        raw_expected_result_name.push_str(".expected.json");

        let expected_result_path = Path::new(&raw_expected_result_name);

        let res = transparent_results(execute_query_with_adapter(
            &FullQuery::from_path(&query_path).unwrap(),
            test_adapter(manifest_path, Some(features)),
            None,
        ));

        assert_query_res(res, expected_result_path);
    }

    #[test_case("test_data/fake_crates/simple_deps" ; "extract from directory")]
    #[test_case("test_data/fake_crates/simple_deps/Cargo.toml" ; "extract from direct path")]
    #[test_case(NONEXISTENT_FILE => panics ; "extract from directory without Cargo.toml")]
    fn extract_metadata(path_str: &str) {
        let _ = ManifestPath::from(path_str);
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
        // TODO: Use cfg(test) bookkeeping fields on GitHubClient to avoid
        // global counter
        let q = FullQuery::from_path(Path::new(
            "test_data/queries/github_simple.in.ron",
        ))
        .unwrap();
        let adapter = test_adapter(
            ManifestPath::from("test_data/fake_crates/direct_dependencies"),
            None,
        );
        let res = execute_query_with_adapter(&q, adapter, Some(1));
        assert_eq!(res.len(), GH_API_CALL_COUNTER.get())
    }
}
