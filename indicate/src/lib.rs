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
#![feature(is_some_and)]
use std::{
    cell::RefCell,
    collections::BTreeMap,
    error::Error,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};

use cargo_metadata::{Metadata, MetadataCommand};
use cargo_toml;
use errors::ManifestPathError;
use glob::glob;
use once_cell::sync::Lazy;
use query::FullQuery;
use tokio::runtime::Runtime;
use trustfall::{execute_query as trustfall_execute_query, FieldValue, Schema};

pub mod adapter;
pub mod advisory;
pub mod errors;
pub mod geiger;
pub mod query;
pub mod repo;
pub mod util;
mod vertex;

/// Features to create metadata with
pub use cargo_metadata::CargoOpt;
pub use rustsec::advisory::Severity;
/// Valid platforms that can be provided to queries
pub use rustsec::platforms;

pub use crate::adapter::adapter_builder::IndicateAdapterBuilder;
pub use crate::adapter::IndicateAdapter;

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

/// The absolute path to a `Cargo.toml` file for a valid Rust package,
/// used to extract metadata and the like
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestPath(PathBuf);

impl ManifestPath {
    /// Attempts to create an absolute path to a Rust package `Cargo.toml` file
    fn absolute_manifest_path_from(
        path: &Path,
    ) -> Result<PathBuf, Box<dyn Error>> {
        let mut manifest_path = path.to_path_buf();

        if manifest_path.is_dir() && !manifest_path.ends_with("Cargo.toml") {
            manifest_path.push("Cargo.toml")
        }

        manifest_path = if !manifest_path.is_absolute() {
            fs::canonicalize(manifest_path)?
        } else {
            manifest_path
        };

        if !manifest_path.exists() {
            Err(Box::new(ManifestPathError::CouldNotCreateValidPath(
                manifest_path.to_string_lossy().into_owned(),
            )))
        } else {
            Ok(manifest_path)
        }
    }

    /// Checks if two package names is equal, using `crates.io` behaviour
    fn equal_package_names(s1: &str, s2: &str) -> bool {
        s1.replace('-', "_").to_lowercase()
            == s2.replace('-', "_").to_lowercase()
    }

    /// Creates a new, guaranteed valid, path to a `Cargo.toml` manifest
    ///
    /// If the path is not an absolute path to a `Cargo.toml` file, it will be
    /// attempted to be converted to it. If a directory is passed, it will be
    /// assumed to contain a `Cargo.toml` file
    pub fn new(path: PathBuf) -> Self {
        let manifest_path = Self::absolute_manifest_path_from(&path)
            .unwrap_or_else(|e| {
                let current_dir = std::env::current_dir()
                    .map(|p| p.to_string_lossy().into())
                    .unwrap_or(String::from("unknown"));
                panic!(
                    "path {} to package could not be resolved due to error: {e} (current dir is {})",
                    path.to_string_lossy(),
                    current_dir
                )
            });
        Self(manifest_path)
    }

    /// Creates a new, guaranteed valid, path to a `Cargo.toml` manifest
    /// where the package name _must_ match the provided name (handling `-` and
    /// `_` as the same character)
    ///
    /// Used when there is a possibility that the provided path contains a
    /// workspace `Cargo.toml` file. In this case, the path will be changed
    /// to point to the correct `Cargo.toml` file.
    ///
    /// The motivation for `_` and `-` handling is that they are considered the
    /// same character by `crates.io` and `cargo`, except in presentation.
    ///
    /// This requires `Metadata` to be parsed (twice), so only use
    /// when it is unsure if the target is a workspace. Otherwise use
    /// [`ManifestPath::new`].
    pub fn with_package_name(path: PathBuf, name: String) -> Self {
        let mut s = Self::new(path);

        let ctf = cargo_toml::Manifest::from_path(&s.0).unwrap_or_else(|e| {
            panic!(
                "could not parse manifest file {} due to error {e}",
                s.0.to_string_lossy()
            )
        });

        if ctf.package.is_none()
            || ctf
                .package
                .is_some_and(|p| !Self::equal_package_names(&p.name(), &name))
        {
            // It is probably a workspace, we'll have to find a `Cargo.toml`
            // file with matching name

            // Remove `Cargo.toml`
            s.0.pop();

            // All directories in the `member` part of workspace
            // are potential targets
            let member_globs = ctf
                .workspace
                .unwrap_or_else(|| {
                    panic!(
                        "{} is neither workspace nor root package!",
                        s.0.to_string_lossy()
                    );
                })
                .members;

            // Create paths to all member `Cargo.toml` files
            let mut member_manifest_paths = Vec::new();
            for member_glob in member_globs {
                // We need to prepend the glob with the path of the workspace
                let mut new_glob = s.0.clone();
                new_glob.push(member_glob);
                let new_glob = new_glob.to_string_lossy();

                // Find all paths for each glob (granted it is valid)
                if let Ok(entries) = glob(&new_glob) {
                    for entry in entries {
                        // Create a path to each `Cargo.toml` for each path
                        // created by glob
                        if let Ok(mut pb) = entry {
                            if pb.is_dir() {
                                pb.push("Cargo.toml");
                            }
                            member_manifest_paths.push(pb);
                        }
                    }
                }
            }

            for manifest_path in member_manifest_paths {
                // Read the file, parse as toml, and see if package.name mathces
                let ct = cargo_toml::Manifest::from_path(&manifest_path);
                match ct {
                    Ok(parsed_config_toml)
                        if parsed_config_toml.package.is_some() =>
                    {
                        if Self::equal_package_names(
                            &parsed_config_toml.package.unwrap().name(),
                            &name,
                        ) {
                            return Self::new(manifest_path);
                        }
                    }
                    Ok(_) => {
                        continue;
                    }
                    Err(_) => {
                        // Might not be a manifest file at all
                        continue;
                    }
                }
            }

            panic!("did not manage to find a `Cargo.toml` manifest file matching the package name {name}");
        } else {
            s
        }
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Extracts metadata from a `Cargo.toml` file, using the features provided.
    ///
    /// Optionally provide a list of features to be used when creating the metadata,
    /// however some combinations may not be viable (see [`CargoOpt`]).
    ///
    /// May return a failure if the features provided are not of a possible
    /// combination (such as `AllFeatures` with `NoDefaultFeatures`).
    pub fn metadata(
        &self,
        features: Vec<CargoOpt>,
    ) -> Result<Metadata, Box<dyn Error>> {
        let mut m = MetadataCommand::new();
        m.manifest_path(self.as_path());

        for feature in features {
            m.features(feature);
        }

        let res = m.exec()?;
        Ok(res)
    }
}

impl From<&'_ str> for ManifestPath {
    fn from(value: &'_ str) -> Self {
        let mut pb = PathBuf::new();
        pb.push(value);
        ManifestPath::new(pb)
    }
}

impl From<String> for ManifestPath {
    /// Attempts to create a valid [`ManifestPath`] from a String representation
    /// of a path, using the same coresions as [`ManifestPath::new`]
    fn from(value: String) -> Self {
        ManifestPath::from(value.as_str())
    }
}

impl From<&String> for ManifestPath {
    fn from(value: &String) -> Self {
        ManifestPath::from(value.as_str())
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
pub fn execute_query(
    query: &FullQuery,
    manifest_path: ManifestPath,
    max_results: Option<usize>,
) -> Vec<BTreeMap<Arc<str>, FieldValue>> {
    let adapter = IndicateAdapter::new(manifest_path);
    execute_query_with_adapter(
        query,
        Rc::new(RefCell::new(adapter)),
        max_results,
    )
}

/// Executes a Trustfall query with a dedicated [`IndicateAdapter`], that may
/// be reused
///
/// Use when the default configuration does not provide enough control.
pub fn execute_query_with_adapter(
    query: &FullQuery,
    adapter: Rc<RefCell<IndicateAdapter>>,
    max_results: Option<usize>,
) -> Vec<BTreeMap<Arc<str>, FieldValue>> {
    let res = match trustfall_execute_query(
        &SCHEMA,
        adapter,
        query.query.as_str(),
        query.args.clone(),
    ) {
        Ok(res) => res.take(max_results.unwrap_or(usize::MAX)).collect(),
        Err(e) => panic!("Could not execute query due to error: {:#?}", e),
    };
    res
}

#[cfg(test)]
mod test {
    // use lazy_static::lazy_static;
    use cargo_metadata::CargoOpt;
    use core::panic;
    use std::{
        cell::RefCell,
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
    ) -> Rc<RefCell<IndicateAdapter>> {
        let mut b = IndicateAdapterBuilder::new(manifest_path).advisory_client(
            AdvisoryClient::from_default_path()
                .unwrap_or_else(|_| AdvisoryClient::new().unwrap()),
        );

        if let Some(f) = features {
            b = b.features(f);
        }

        Rc::new(RefCell::new(b.build()))
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
        let manifest_path = ManifestPath::new(cargo_toml_path);
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
    fn query_test(fake_crate_name: &str, query_name: &str) {
        let (cargo_toml_path, query_path) =
            get_paths(fake_crate_name, query_name);
        let raw_expected_result_name =
            format!("test_data/queries_expected/{query_name}.expected.json");
        let expected_result_path = Path::new(&raw_expected_result_name);

        // We use `TransparentValue for neater JSON serialization
        let res = transparent_results(execute_query_with_adapter(
            &FullQuery::from_path(query_path.as_path()).unwrap(),
            test_adapter(ManifestPath::new(cargo_toml_path), None),
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

        let manifest_path = ManifestPath::new(cargo_toml_path);

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
