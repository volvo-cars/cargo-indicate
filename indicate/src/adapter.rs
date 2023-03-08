use std::{
    cell::RefCell, collections::HashMap, rc::Rc, str::FromStr, sync::Arc,
};

use cargo_metadata::{Metadata, Package, PackageId};
use chrono::{NaiveDate, NaiveDateTime};
use git_url_parse::GitUrl;
use trustfall::{
    provider::{
        accessor_property, field_property, resolve_neighbors_with,
        resolve_property_with, BasicAdapter, ContextIterator,
        ContextOutcomeIterator, EdgeParameters, VertexIterator,
    },
    FieldValue,
};

use crate::{
    advisory::AdvisoryClient,
    repo::github::{GitHubClient, GitHubRepositoryId},
    vertex::Vertex,
};

pub mod adapter_builder;

type DirectDependencyMap = HashMap<PackageId, Rc<Vec<PackageId>>>;
type PackageMap = HashMap<PackageId, Rc<Package>>;

/// Parse metadata to create maps over the packages and dependency
/// relations in it
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
        let deps = node.dependencies.to_owned();
        direct_dependencies.insert(id, Rc::new(deps));
    }

    (packages, direct_dependencies)
}

pub struct IndicateAdapter {
    metadata: Rc<Metadata>,
    packages: Rc<PackageMap>,

    /// Direct dependencies to a package, i.e. _not_ dependencies to dependencies
    direct_dependencies: Rc<DirectDependencyMap>,
    gh_client: Rc<RefCell<GitHubClient>>,
    advisory_client: Rc<AdvisoryClient>,
}

/// The functions here are essentially the fields on the RootQuery
impl IndicateAdapter {
    fn root_package(&self) -> VertexIterator<'static, Vertex> {
        let root = self.metadata.root_package().expect("no root package found");
        let v = Vertex::Package(Rc::new(root.clone()));
        Box::new(std::iter::once(v))
    }
}

/// Helper methods to resolve fields using the metadata
impl IndicateAdapter {
    /// Creates a new [`IndicateAdapter`], using a provided metadata as a starting point
    ///
    /// If control over what GitHub client is used, if a cached `advisory-db`
    /// is to be used etc., consider using
    /// [`IndicateAdapterBuilder`](adapter_builder::IndicateAdapterBuilder)
    pub fn new(metadata: Metadata) -> Self {
        let (packages, direct_dependencies) = parse_metadata(&metadata);

        // If we are in a test environment, we try to use
        // a cached version of `advisory-db`
        // TODO: Make this a CLI flag, possibly using a IndicateCfg passed
        let advisory_client;
        if cfg!(test) {
            advisory_client = match AdvisoryClient::from_default_path() {
                Ok(client) => client,
                Err(_) => AdvisoryClient::new().unwrap_or_else(|e| {
                    panic!("could not create advisory client due to error: {e}")
                }),
            };
        } else {
            advisory_client = AdvisoryClient::new().unwrap_or_else(|e| {
                panic!("could not create advisory client due to error: {e}")
            });
        }

        Self {
            metadata: Rc::new(metadata),
            packages: Rc::new(packages),
            direct_dependencies: Rc::new(direct_dependencies),
            gh_client: Rc::new(RefCell::new(GitHubClient::new())),
            advisory_client: Rc::new(advisory_client),
        }
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
                    if matches!(gurl.host, Some(x) if x == "github.com") {
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
        _parameters: &EdgeParameters,
    ) -> VertexIterator<'a, Self::Vertex> {
        match edge_name {
            // These edge names should match 1:1 for `schema.trustfall.graphql`
            "RootPackage" => self.root_package(),
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
                let advisory_client = Rc::clone(&self.advisory_client);
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
