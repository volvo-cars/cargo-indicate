use cargo::core::Workspace as CargoWorkspace;
use cargo::ops::load_pkg_lockfile as load_cargo_lockfile;
use cargo::util::config::Config as CargoConfig;
use cargo::util::hex;
use cargo_metadata::{CargoOpt, DependencyKind, Metadata, Package, PackageId};
use chrono::{NaiveDate, NaiveDateTime};
use git_url_parse::GitUrl;
use once_cell::unsync::OnceCell;
use std::path::PathBuf;
use std::{
    cell::RefCell, collections::HashMap, env, rc::Rc, str::FromStr, sync::Arc,
};
use trustfall::{
    provider::{
        accessor_property, field_property, resolve_neighbors_with,
        resolve_property_with, BasicAdapter, ContextIterator,
        ContextOutcomeIterator, EdgeParameters, VertexIterator,
    },
    FieldValue,
};

use crate::code_stats::{get_code_stats, CodeStats};
use crate::{
    advisory::AdvisoryClient,
    geiger::GeigerClient,
    repo::github::{GitHubClient, GitHubRepositoryId},
    vertex::Vertex,
    ManifestPath,
};
use crate::{IndicateAdapterBuilder, NameVersion};

pub mod adapter_builder;

/// Direct dependencies to a package, i.e. _not_ dependencies to dependencies
type DirectDependencyMap = HashMap<PackageId, Rc<Vec<PackageId>>>;
type PackageMap = HashMap<PackageId, Rc<Package>>;
/// Maps the (name, version) tuple of a dependency to its local path to source
/// code
type SourceMap = HashMap<NameVersion, PathBuf>;

/// Parse metadata to create maps over the packages and dependency
/// relations in it
///
/// `direct_dependencies` will only include 'normal' dependencies, i.e.
/// not build nor test deps.
pub fn parse_metadata(
    metadata: &Metadata,
) -> (PackageMap, DirectDependencyMap) {
    let mut packages = HashMap::with_capacity(metadata.packages.len());

    for p in &metadata.packages {
        let id = p.id.to_owned();
        let package = p.to_owned();
        packages.insert(id, Rc::new(package));
    }

    let mut direct_dependencies =
        HashMap::with_capacity(metadata.packages.len());

    for node in metadata
        .resolve
        .as_ref()
        .expect("No nodes found!")
        .nodes
        .iter()
    {
        let id = node.id.to_owned();

        // Filter out dependencies that are not normal
        let normal_deps = node
            .deps
            .iter()
            .filter_map(|nd| {
                if nd
                    .dep_kinds
                    .iter()
                    .any(|dki| dki.kind == DependencyKind::Normal)
                {
                    // A dependency can have many kinds; We only care if it is
                    // normal
                    Some(nd.pkg.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        direct_dependencies.insert(id, Rc::new(normal_deps));
    }

    (packages, direct_dependencies)
}

/// Resolves the path to where dependencies are stored, and map them to
/// dependency (name, version)
///
/// This is done since internally, `cargo` and `cargo_metadata` are not required
/// to use the same convention for `PackageId`.
pub fn resolve_cargo_dirs(manifest_path: &ManifestPath) -> SourceMap {
    // This code is based on `cargo-local`, licensed under MIT, which is in
    // turn based on how `cargo` resolves registries.
    // https://github.com/AndrewRadev/cargo-local/blob/0773ae40e7c6e293d95e087bb3e10c1b70d3c429/src/main.rs
    let cargo_config = CargoConfig::default()
        .expect("default cargo config could not be created");
    let workspace = CargoWorkspace::new(manifest_path.as_path(), &cargo_config)
        .expect("error when resolving workspace");

    let resolved = match load_cargo_lockfile(&workspace)
        .expect("could not load lockfile")
    {
        Some(r) => r,
        None => return HashMap::new(),
    };

    // Build registry_source_path the same way cargo's Config does as of
    // https://github.com/rust-lang/cargo/blob/176b5c17906cf43445888e83a4031e411f56e7dc/src/cargo/util/config.rs#L35-L80
    let cwd = env::current_dir().expect("could not retrieve current dir");
    let cargo_home = env::var_os("CARGO_HOME").map(|home| cwd.join(home));
    let user_home = ::dirs::home_dir()
        .map(|p| p.join(".cargo"))
        .expect("could not resolve user home directory");
    let home_path = cargo_home.unwrap_or(user_home);
    let registry_source_path = home_path.join("registry").join("src");

    let paths = resolved
        .iter()
        .flat_map(|pkgid| {
            // Build src_path the same way cargo's RegistrySource does as of
            // https://github.com/rust-lang/cargo/blob/176b5c17906cf43445888e83a4031e411f56e7dc/src/cargo/sources/registry.rs#L232-L238
            let hash = hex::short_hash(&pkgid.source_id());
            let ident = pkgid.source_id().url().host()?.to_string();
            let part = format!("{}-{}", ident, hash);
            let src_path = registry_source_path.join(&part);

            // Build the crate's unpacked destination directory the same way cargo's RegistrySource does as
            // of https://github.com/rust-lang/cargo/blob/176b5c17906cf43445888e83a4031e411f56e7dc/src/cargo/sources/registry.rs#L357-L358
            let dest = format!("{}-{}", pkgid.name(), pkgid.version());
            let full_path = src_path.join(&dest);

            if full_path.exists() {
                Some((
                    NameVersion::new(
                        pkgid.name().to_string(),
                        pkgid.version().to_owned(),
                    ),
                    full_path,
                ))
            } else {
                None
            }
        })
        .collect();

    paths
}

macro_rules! resolve_code_stats {
    ($getter:ident) => {
        |v| {
            let res = match v {
                Vertex::LanguageCodeStats(c) => c.$getter(),
                Vertex::LanguageBlob(c) => c.$getter(),
                u => {
                    unreachable!("cannot access files on vertex {u:?}")
                }
            };
            FieldValue::Uint64(res as u64)
        }
    };
}

pub struct IndicateAdapter {
    manifest_path: Rc<ManifestPath>,
    features: Vec<CargoOpt>,
    metadata: Rc<Metadata>,
    packages: Rc<PackageMap>,
    direct_dependencies: Rc<DirectDependencyMap>,
    source_map: Rc<SourceMap>,
    gh_client: Rc<RefCell<GitHubClient>>,
    advisory_client: OnceCell<Rc<AdvisoryClient>>,
    geiger_client: OnceCell<Rc<GeigerClient>>,
}

/// The functions here are essentially the fields on the RootQuery
impl IndicateAdapter {
    fn root_package(&self) -> VertexIterator<'static, Vertex> {
        let root = self.metadata.root_package().expect("no root package found");
        let v = Vertex::Package(Rc::new(root.clone()));
        Box::new(std::iter::once(v))
    }

    /// Retrieves an iterator over all package IDs of normal dependencies
    /// (transitive and direct)
    fn dependency_ids(&self, include_root: bool) -> Vec<PackageId> {
        // Use the direct, normal dependencies we already resolved when
        // parsing the metadata
        let mut dependency_package_ids = self
            .direct_dependencies
            .values()
            .map(|r| r.to_vec())
            .flatten()
            .collect::<Vec<_>>();

        // Remove root if requrested (is always included in dependency graph)
        if include_root {
            let root_package = self
                .metadata
                .root_package()
                .expect("could not resolve root node");
            dependency_package_ids.push(root_package.id.clone());
        }

        // Sorting gives us same output every time, and allows for
        // deduplicating. The duplicates are from multiple packages sharing the
        // same direct dependency
        dependency_package_ids.sort();
        dependency_package_ids.dedup();
        dependency_package_ids
    }

    /// Retrieves an iterator over all dependencies, optionally including the
    /// root package
    ///
    /// Only returns dependencies that are of the 'normal' kind, i.e. no
    /// dev or build dependencies.
    fn dependencies(
        &self,
        include_root: bool,
    ) -> VertexIterator<'static, Vertex> {
        let dependency_package_ids = self.dependency_ids(include_root);
        // We must call `.collect()`, to ensure lifetimes by enforcing the
        // `Rc::clone`. It will not affect the resolution or laziness, since
        // this is a starting node
        let dependencies = dependency_package_ids
            .iter()
            .map(|pid| {
                // We must be able to find it, since packages is based on this
                Vertex::Package(Rc::clone(self.packages().get(&pid).unwrap()))
            })
            .collect::<Vec<_>>()
            .into_iter();

        Box::new(dependencies)
    }

    /// Retrieves a vector of all transitive dependency IDs, i.e. dependencies
    /// that are dependencies of direct dependencies
    fn transitive_dependency_ids(&self) -> Vec<PackageId> {
        // Transitive dependencies are those that are direct dependencies to
        // anything but the root package
        let root_package_id = self
            .metadata
            .root_package()
            .expect("could not resolve root node")
            .id
            .clone();
        let mut transitive_dependency_ids = self
            .direct_dependencies
            .iter()
            .filter_map(|(p, dir_deps)| {
                if *p != root_package_id {
                    Some((*(*dir_deps)).clone())
                } else {
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>();

        // Sorting gives us same output every time, and allows for
        // deduplicating. The duplicates are from multiple packages sharing the
        // same direct dependency
        transitive_dependency_ids.sort();
        transitive_dependency_ids.dedup();
        transitive_dependency_ids
    }

    /// Retrieves an iterator over all transitive dependencies (dependencies
    /// of direct dependencies to the root package)
    ///
    /// Only returns dependencies that are of the 'normal' kind, i.e. no
    /// dev or build dependencies.
    fn transitive_dependencies(&self) -> VertexIterator<'static, Vertex> {
        let dependency_package_ids = self.transitive_dependency_ids();
        // We must call `.collect()`, to ensure lifetimes by enforcing the
        // `Rc::clone`. It will not affect the resolution or laziness, since
        // this is a starting node
        let dependencies = dependency_package_ids
            .iter()
            .map(|pid| {
                // We must be able to find it, since packages is based on this
                Vertex::Package(Rc::clone(self.packages().get(&pid).unwrap()))
            })
            .collect::<Vec<_>>()
            .into_iter();

        Box::new(dependencies)
    }
}

/// Helper methods to resolve fields using the metadata
impl IndicateAdapter {
    /// Creates a new [`IndicateAdapter`], using a manifest path as a starting point
    ///
    /// If control over what GitHub client is used, if a cached `advisory-db`
    /// is to be used etc., consider using
    /// [`IndicateAdapterBuilder`](adapter_builder::IndicateAdapterBuilder).
    pub fn new(manifest_path: ManifestPath) -> Self {
        IndicateAdapterBuilder::new(manifest_path).build()
    }

    /// Retrieves a new counted reference to this adapters [`Metadata`]
    #[must_use]
    fn metadata(&self) -> Rc<Metadata> {
        Rc::clone(&self.metadata)
    }

    /// Retrieves a new counted reference to this adapters [`PackageMap`]
    #[must_use]
    fn packages(&self) -> Rc<PackageMap> {
        Rc::clone(&self.packages)
    }

    /// Retrieves a new counted reference to this adapters [`PackageMap`]
    #[must_use]
    fn direct_dependencies(&self) -> Rc<DirectDependencyMap> {
        Rc::clone(&self.direct_dependencies)
    }

    /// Retrieves a new counted reference to this adapters [`GitHubClient`]
    #[must_use]
    fn gh_client(&self) -> Rc<RefCell<GitHubClient>> {
        Rc::clone(&self.gh_client)
    }

    /// Retrieve or create a [`AdvisoryClient`]
    ///
    /// Since this is an expensive operation, it should only be done when the
    /// data *must* be used.
    #[must_use]
    fn advisory_client(&self) -> Rc<AdvisoryClient> {
        let sac = self.advisory_client.get_or_init(|| {
            let ac = AdvisoryClient::new().unwrap_or_else(|e| {
                panic!("could not create advisory client due to error: {e}")
            });
            Rc::new(ac)
        });
        Rc::clone(sac)
    }

    /// Retrieve or evaluate a [`GeigerClient`] for the features and manifest
    /// path used by this adapter
    ///
    /// Since this is an expensive operation, it should only be done when the
    /// data *must* be used.
    #[must_use]
    fn geiger_client(&self) -> Rc<GeigerClient> {
        let sgc = self.geiger_client.get_or_init(|| {
            let gc = GeigerClient::new(
                &self.manifest_path,
                self.features.to_owned(),
            )
            .unwrap_or_else(|e| {
                panic!("failed to create geiger data due to error: {e}")
            });
            Rc::new(gc)
        });

        Rc::clone(sgc)
    }

    fn get_dependencies(
        packages: Rc<PackageMap>,
        direct_dependencies: Rc<DirectDependencyMap>,
        package_id: &PackageId,
    ) -> VertexIterator<'static, Vertex> {
        let dd = Rc::clone(&direct_dependencies);
        let dependency_ids = dd.get(package_id).unwrap_or_else(|| {
            panic!(
                "Could not extract dependency IDs for package {}",
                &package_id
            )
        });

        let dependencies = dependency_ids
            .iter()
            .map(move |id| {
                let p = packages.get(id).unwrap();
                Vertex::Package(Rc::clone(p))
            })
            .collect::<Vec<_>>()
            .into_iter();

        Box::new(dependencies)
    }

    /// Returns a form of repository, i.e. a variant that implements the
    /// `schema.trustfall.graphql` `repository` interface
    fn get_repository_from_url(
        url: &str,
        gh_client: Rc<RefCell<GitHubClient>>,
    ) -> Vertex {
        // TODO: Better identification of repository URLs...
        if url.contains("github.com") {
            match GitUrl::parse(url) {
                Ok(gurl) => {
                    if gurl.fullname != gurl.path.trim_matches('/') {
                        // Points to something inside the repo rather than
                        // the repo itself. For now just return webpage
                        eprintln!("{url} points to something inside repository, set as webpage");
                        Vertex::Webpage(String::from(url))
                    } else if matches!(gurl.host, Some(x) if x == "github.com")
                    {
                        // This is in fact a GitHub url, we attempt to retrieve it
                        let id = GitHubRepositoryId::new(
                            gurl.owner.unwrap_or_else(|| {
                                panic!("repository {url} had no owner",)
                            }),
                            gurl.name,
                        );

                        if let Some(fr) =
                            gh_client.borrow_mut().get_repository(&id)
                        {
                            Vertex::GitHubRepository(fr)
                        } else {
                            // We were unable to retrieve the repository
                            Vertex::Repository(String::from(url))
                        }
                    } else {
                        // The host is not github.com
                        Vertex::Repository(String::from(url))
                    }
                }
                Err(_) => Vertex::Repository(String::from(url)),
            }
        } else {
            Vertex::Repository(String::from(url))
        }
    }
}

impl<'a> BasicAdapter<'a> for IndicateAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name {
            // These edge names should match 1:1 for `schema.trustfall.graphql`
            "RootPackage" => self.root_package(),
            "Dependencies" => {
                // The unwrap is OK since trustfall will verify the parimeters
                // to match the schema
                let include_root =
                    parameters.get("includeRoot").unwrap().as_bool().unwrap();
                self.dependencies(include_root)
            }
            "TransitiveDependencies" => self.transitive_dependencies(),
            e => {
                unreachable!("edge {e} has no resolution as a starting vertex")
            }
        }
    }

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
        // This match statement must contain _all_ possible types provided
        // by `schema.trustfall.graphql`
        match (type_name, property_name) {
            ("Package", "id") => resolve_property_with(contexts, |v| {
                if let Some(s) = v.as_package() {
                    FieldValue::String(s.id.to_string())
                } else {
                    unreachable!("Not a package!")
                }
            }),
            ("Package", "name") => resolve_property_with(
                contexts,
                field_property!(as_package, name),
            ),
            ("Package", "version") => resolve_property_with(contexts, |v| {
                if let Some(s) = v.as_package() {
                    FieldValue::String(s.version.to_string())
                } else {
                    unreachable!("Not a package!")
                }
            }),
            ("Package", "license") => resolve_property_with(contexts, |v| {
                match &v.as_package().unwrap().license {
                    Some(l) => l.as_str().into(),
                    None => FieldValue::Null,
                }
            }),
            ("Package", "manifestPath") => {
                resolve_property_with(contexts, |v| {
                    let package = v.as_package().unwrap();
                    FieldValue::String(
                        package.manifest_path.clone().into_string(),
                    )
                })
            }
            ("Webpage" | "Repository" | "GitHubRepository", "url") => {
                resolve_property_with(contexts, |v| match v.as_webpage() {
                    Some(url) => FieldValue::String(url.to_owned()),
                    None => FieldValue::Null,
                })
            }
            ("GitHubRepository", "name") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, name),
            ),
            ("GitHubRepository", "starsCount") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, stargazers_count),
            ),
            ("GitHubRepository", "forksCount") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, forks_count),
            ),
            ("GitHubRepository", "openIssuesCount") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, open_issues_count),
            ),
            ("GitHubRepository", "hasIssues") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, has_issues),
            ),
            ("GitHubRepository", "archived") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, archived),
            ),
            ("GitHubRepository", "fork") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, fork),
            ),
            ("GitHubUser", "username") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_user, login),
            ),
            ("GitHubUser", "unixCreatedAt") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_user, created_at, {
                    created_at.map(|d| d.timestamp()).into() // Convert to Unix timestamp
                }),
            ),
            ("GitHubUser", "followersCount") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_user, followers),
            ),
            ("GitHubUser", "email") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_user, email),
            ),
            ("Advisory", "id") => resolve_property_with(
                contexts,
                accessor_property!(as_advisory, id, { id.to_string().into() }),
            ),
            ("Advisory", "title") => resolve_property_with(
                contexts,
                accessor_property!(as_advisory, title),
            ),
            ("Advisory", "description") => resolve_property_with(
                contexts,
                accessor_property!(as_advisory, description),
            ),
            ("Advisory", "unixDateReported") => resolve_property_with(
                contexts,
                accessor_property!(as_advisory, date, {
                    // TODO: This assumes the advisory being posted 00:00 UTC,
                    // which might or might not be a good idea
                    let dt: NaiveDateTime = NaiveDate::from_ymd_opt(
                        date.year() as i32,
                        date.month(),
                        date.day(),
                    )
                    .expect("could not parse advisory unix date")
                    .and_hms_opt(0, 0, 0)
                    .expect("could not create advisory timestamp");
                    dt.timestamp().into()
                }),
            ),
            ("Advisory", "unixDateWithdrawn") => resolve_property_with(
                contexts,
                field_property!(as_advisory, metadata, {
                    // TODO: This assumes the advisory being withdrawn 00:00 UTC,
                    // which might or might not be a good idea
                    match &metadata.withdrawn {
                        Some(date) => {
                            let dt: NaiveDateTime = NaiveDate::from_ymd_opt(
                                date.year() as i32,
                                date.month(),
                                date.day(),
                            )
                            .expect("could not parse advisory unix date")
                            .and_hms_opt(0, 0, 0)
                            .expect("could not create advisory timestamp");
                            dt.timestamp().into()
                        }
                        None => FieldValue::Null,
                    }
                }),
            ),
            ("Advisory", "affectedArch") => resolve_property_with(
                contexts,
                field_property!(as_advisory, affected, {
                    match affected {
                        Some(aff) => aff
                            .arch
                            .iter()
                            .map(|a| a.to_string())
                            .collect::<Vec<String>>()
                            .into(),
                        None => FieldValue::Null,
                    }
                }),
            ),
            ("Advisory", "affectedOs") => resolve_property_with(
                contexts,
                field_property!(as_advisory, affected, {
                    match affected {
                        Some(aff) => aff
                            .os
                            .iter()
                            .map(|a| a.to_string())
                            .collect::<Vec<String>>()
                            .into(),
                        None => FieldValue::Null,
                    }
                }),
            ),
            ("Advisory", "patchedVersions") => resolve_property_with(
                contexts,
                field_property!(as_advisory, versions, {
                    versions
                        .patched()
                        .iter()
                        .map(|vr| vr.to_string())
                        .collect::<Vec<String>>()
                        .into()
                }),
            ),
            ("Advisory", "unaffectedVersions") => resolve_property_with(
                contexts,
                field_property!(as_advisory, versions, {
                    versions
                        .unaffected()
                        .iter()
                        .map(|vr| vr.to_string())
                        .collect::<Vec<String>>()
                        .into()
                }),
            ),
            ("Advisory", "severity") => resolve_property_with(
                contexts,
                accessor_property!(as_advisory, severity, {
                    match severity {
                        Some(s) => FieldValue::String(s.to_string()),
                        None => FieldValue::Null,
                    }
                }),
            ),
            // ("Advisory", "cvss") => resolve_property_with(
            //     contexts,
            //     field_property!(as_advisory, metadata, {
            //         match &metadata.cvss {
            //             Some(_base) => todo!("enums not yet implemented"),
            //             None => FieldValue::Null,
            //         }
            //     }),
            // ),
            ("AffectedFunctionVersions", "functionPath") => {
                resolve_property_with(contexts, |vertex| {
                    let afv = vertex.as_affected_function_versions().unwrap();
                    afv.0.to_string().into()
                })
            }
            ("AffectedFunctionVersions", "versions") => {
                resolve_property_with(contexts, |vertex| {
                    let afv = vertex.as_affected_function_versions().unwrap();
                    afv.1
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<String>>()
                        .into()
                })
            }
            ("GeigerUnsafety", "forbidsUnsafe") => resolve_property_with(
                contexts,
                field_property!(as_geiger_unsafety, forbids_unsafe),
            ),
            ("GeigerCount", "safe") => resolve_property_with(
                contexts,
                field_property!(as_geiger_count, safe),
            ),
            ("GeigerCount", "unsafe") => resolve_property_with(
                contexts,
                field_property!(as_geiger_count, unsafe_),
            ),
            ("GeigerCount", "total") => resolve_property_with(
                contexts,
                accessor_property!(as_geiger_count, total),
            ),
            ("GeigerCount", "percentageUnsafe") => {
                resolve_property_with(contexts, |vertex| {
                    // From<f64> for FieldValue not implemented at this time
                    let count = vertex.as_geiger_count().unwrap();
                    let percentage = count.percentage_unsafe();
                    FieldValue::Float64(percentage)
                })
            }
            ("LanguageCodeStats" | "LanguageBlob", "files") => {
                resolve_property_with(contexts, resolve_code_stats!(files))
            }
            ("LanguageCodeStats" | "LanguageBlob", "blanks") => {
                resolve_property_with(contexts, resolve_code_stats!(blanks))
            }
            ("LanguageCodeStats" | "LanguageBlob", "code") => {
                resolve_property_with(contexts, resolve_code_stats!(code))
            }
            ("LanguageCodeStats" | "LanguageBlob", "comments") => {
                resolve_property_with(contexts, resolve_code_stats!(comments))
            }
            // ("Language", "language") => {
            //     resolve_property_with(contexts, field_property!(as_language, l))
            // }
            (t, p) => {
                unreachable!("unreachable property combination: {t}, {p}")
            }
        }
    }

    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> ContextOutcomeIterator<
        'a,
        Self::Vertex,
        VertexIterator<'a, Self::Vertex>,
    > {
        // These are all possible neighboring vertexes, i.e. parts of a vertex
        // that are not scalar values (`FieldValue`)
        match (type_name, edge_name) {
            ("Package", "dependencies") => {
                // Must be done here to ensure they live long enough (and are
                // not lazily evaluated)
                let packages = self.packages();
                let direct_dependencies = self.direct_dependencies();
                resolve_neighbors_with(contexts, move |vertex| {
                    // This is in fact a Package, otherwise it would be `None`
                    // First get all dependencies, and then resolve their package
                    // by finding that dependency by its ID in the metadata
                    let package = vertex.as_package().unwrap();
                    Self::get_dependencies(
                        Rc::clone(&packages),
                        Rc::clone(&direct_dependencies),
                        &package.id,
                    )
                })
            }
            ("Package", "repository") => {
                let gh_client = self.gh_client();
                resolve_neighbors_with(contexts, move |v| {
                    // Must be package
                    let package = v.as_package().unwrap();
                    match &package.repository {
                        Some(url) => Box::new(std::iter::once(
                            Self::get_repository_from_url(
                                url,
                                Rc::clone(&gh_client),
                            ),
                        )),
                        None => Box::new(std::iter::empty()),
                    }
                })
            }
            ("Package", "advisoryHistory") => {
                let advisory_client = self.advisory_client();
                let include_withdrawn =
                    parameters.get("includeWithdrawn").map(|p| p.to_owned());
                let arch = parameters.get("arch").map(|p| p.to_owned());
                let os = parameters.get("os").map(|p| p.to_owned());
                let min_severity =
                    parameters.get("minSeverity").map(|p| p.to_owned());

                resolve_neighbors_with(contexts, move |vertex| {
                    let package = vertex.as_package().unwrap();
                    let include_withdrawn = include_withdrawn
                        .to_owned()
                        .expect("includeWithdrawn parameter required but not provided")
                        .as_bool().expect("includeWithdrawn must be a boolean");

                    // Handle using Strings in the Schema as Rust enums
                    let arch = arch
                        .to_owned()
                        .and_then(|fv| {
                            fv.as_str().and_then(|s| s.to_string().into())
                        })
                        .map(|s| {
                            rustsec::platforms::Arch::from_str(s.as_str())
                                .unwrap_or_else(|_| {
                                    panic!("unknown arch parameter: {s}")
                                })
                        });
                    let os = os
                        .to_owned()
                        .and_then(|fv| {
                            fv.as_str().and_then(|s| s.to_string().into())
                        })
                        .map(|s| {
                            rustsec::platforms::OS::from_str(s.as_str())
                                .unwrap_or_else(|_| {
                                    panic!("unknown os parameter: {s}")
                                })
                        });
                    let min_severity = min_severity
                        .to_owned()
                        .and_then(|fv| {
                            fv.as_str().and_then(|s| s.to_string().into())
                        })
                        .map(|s|
                            cvss::Severity::from_str(s.as_str())
                            .unwrap_or_else(|e| panic!("{} is not a valid CVSS severity level ({e})", s)));

                    let res = advisory_client
                        .all_advisories_for_package(
                            rustsec::package::Name::from_str(&package.name)
                                .unwrap_or_else(|e| {
                                    panic!("package name {} not valid due to error: {e}", package.name)
                                }),
                            include_withdrawn,
                            arch,
                            os,
                            min_severity,
                        )
                        .iter()
                        .map(|a| Vertex::Advisory(Rc::new((*a).clone())))
                        .collect::<Vec<_>>() // Collect OK: We just convert back to vec
                        .into_iter();

                    Box::new(res)
                })
            }
            ("Package", "geiger") => {
                let geiger_client = self.geiger_client();
                resolve_neighbors_with(contexts, move |vertex| {
                    let package = vertex.as_package().unwrap();
                    let gid = package.into();
                    let unsafety = geiger_client.unsafety(&gid);

                    match unsafety {
                        Some(u) => {
                            Box::new(std::iter::once(Vertex::GeigerUnsafety(u)))
                        }
                        None => {
                            eprintln!(
                                "failed to resolve geiger unsafety for {} {}",
                                package.name, package.version
                            );
                            Box::new(std::iter::empty())
                        }
                    }
                })
            }
            ("Package", "codeStats") => {
                let source_map = Rc::clone(&self.source_map);
                // Parameters verified by `trustfall` and schema
                let ignored_paths =
                    parameters.get("ignoredPaths").unwrap().to_owned();

                // Either they are passed and _must_ be a bool according to
                // schema, or they are undefined
                let get_stat_bool_param = |pname| {
                    parameters.get(pname).map(|p| p.as_bool()).flatten()
                };

                let config = tokei::Config {
                        columns: None, // Unused for library
                        hidden: get_stat_bool_param("hidden"),
                        no_ignore: get_stat_bool_param("noIgnore"),
                        no_ignore_parent: get_stat_bool_param("noIgnoreParent"),
                        no_ignore_dot: get_stat_bool_param("noIgnoreDot"),
                        no_ignore_vcs: get_stat_bool_param("noIgnoreVcs"),
                        treat_doc_strings_as_comments: get_stat_bool_param(
                            "treatDocStringsAsComments",
                        ),
                        types: parameters.get("types").map(|t| {
                            t.as_vec(|i| {
                                let language_str = i.as_str().unwrap();
                                let lt = tokei::LanguageType::from_str(language_str).unwrap_or_else(|_| {
                                    panic!("parameter error: {language_str} is not a valid language name");
                                });
                                Some(lt)
                            })
                        }).flatten()
                            .to_owned(),
                        sort: None, // TODO: Not implemented
                    };

                resolve_neighbors_with(contexts, move |vertex| {
                    let package = vertex.as_package().unwrap();
                    let package_path = source_map
                        .get(&package.into())
                        .cloned()
                        .unwrap_or_else(|| {
                            let mut mp = package
                                .manifest_path
                                .clone()
                                .into_std_path_buf();

                            // Remove `Cargo.toml`
                            mp.pop();
                            mp
                        });
                    // .unwrap_or_else(|| {
                    //     panic!("could not resolve local registry path to {} {} ({}), {}", package.name, package.version, package.id, package.manifest_path);
                    // });

                    let ignored_paths =
                        ignored_paths.as_vec(|fv| fv.as_str()).unwrap();
                    let code_stats = get_code_stats(
                        &package_path,
                        ignored_paths.as_slice(),
                        &config,
                    );

                    Box::new(
                        code_stats
                            .into_iter()
                            .map(|cs| Vertex::LanguageCodeStats(Rc::new(cs))),
                    )
                })
            }
            ("GitHubRepository", "owner") => {
                let gh_client = self.gh_client();
                resolve_neighbors_with(contexts, move |vertex| {
                    // Must be GitHubRepository according to guarantees from Trustfall
                    let gh_repo = vertex.as_git_hub_repository().unwrap();
                    match &gh_repo.owner {
                        Some(simple_user) => {
                            let user = gh_client
                                .borrow_mut()
                                .get_public_user(&simple_user.login);

                            match user {
                                Some(u) => Box::new(std::iter::once(
                                    Vertex::GitHubUser(Arc::clone(&u)),
                                )),
                                None => Box::new(std::iter::empty()),
                            }
                        }
                        None => Box::new(std::iter::empty()),
                    }
                })
            }
            ("Advisory", "affectedFunctions") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let advisory = vertex.as_advisory().unwrap();
                    match &advisory.affected {
                        Some(aff) => Box::new(
                            aff.functions
                                .clone()
                                .into_iter()
                                .map(Vertex::AffectedFunctionVersions),
                        ),
                        None => Box::new(std::iter::empty()),
                    }
                })
            }
            ("GeigerUnsafety", "used") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let unsafety = vertex.as_geiger_unsafety().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCategories(
                        unsafety.used,
                    )))
                })
            }
            ("GeigerUnsafety", "unused") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let unsafety = vertex.as_geiger_unsafety().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCategories(
                        unsafety.unused,
                    )))
                })
            }
            ("GeigerUnsafety", "total") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let unsafety = vertex.as_geiger_unsafety().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCategories(
                        unsafety.total(),
                    )))
                })
            }
            ("GeigerCategories", "functions") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let categories = vertex.as_geiger_categories().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCount(
                        categories.functions,
                    )))
                })
            }
            ("GeigerCategories", "exprs") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let categories = vertex.as_geiger_categories().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCount(
                        categories.exprs,
                    )))
                })
            }
            ("GeigerCategories", "item_impls") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let categories = vertex.as_geiger_categories().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCount(
                        categories.item_impls,
                    )))
                })
            }
            ("GeigerCategories", "item_traits") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let categories = vertex.as_geiger_categories().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCount(
                        categories.item_traits,
                    )))
                })
            }
            ("GeigerCategories", "methods") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let categories = vertex.as_geiger_categories().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCount(
                        categories.methods,
                    )))
                })
            }
            ("GeigerCategories", "total") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let categories = vertex.as_geiger_categories().unwrap();
                    Box::new(std::iter::once(Vertex::GeigerCount(
                        categories.total(),
                    )))
                })
            }
            (t, e) => {
                unreachable!("unreachable neighbor combination: {t}, {e}")
            }
        }
    }

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
        // Ensure lifetimes by cloning
        let type_name = type_name.to_owned();
        let coerce_to_type = coerce_to_type.to_owned();
        Box::new(
            contexts
                .map(move |ctx| {
                    let current_vertex = &ctx.active_vertex();
                    let current_vertex = match current_vertex {
                        Some(v) => v,
                        None => return (ctx, false),
                    };

                    let can_coerce = match (
                        type_name.as_str(),
                        coerce_to_type.as_str()
                    ) {
                        (_, "Repository") => {
                            current_vertex.as_repository().is_some()
                        }
                        (_, "GitHubRepository") => {
                            current_vertex.as_git_hub_repository().is_some()
                        }
                        (t1, t2) => {
                            unreachable!(
                                "the coercion from {t1} to {t2} is unhandled but was attempted",
                            )
                        }
                    };

                    (ctx, can_coerce)
                })
        )
    }
}
