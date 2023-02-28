use std::{collections::HashMap, fmt::Debug, rc::Rc, sync::Arc};

use cargo_metadata::{Metadata, Package, PackageId};
use chrono::{NaiveDate, NaiveDateTime};
use git_url_parse::GitUrl;
use trustfall::{
    provider::{
        accessor_property, field_property, resolve_neighbors_with,
        resolve_property_with, BasicAdapter, ContextIterator,
        ContextOutcomeIterator, DataContext, EdgeParameters, VertexIterator,
    },
    FieldValue,
};

use crate::{
    advisory,
    repo::github::{GitHubClient, GitHubRepositoryId},
    vertex::Vertex,
};

pub struct IndicateAdapter<'a> {
    metadata: &'a Metadata,
    packages: HashMap<PackageId, Rc<Package>>,

    /// Direct dependencies to a package, i.e. _not_ dependencies to dependencies
    direct_dependencies: HashMap<PackageId, Rc<Vec<PackageId>>>,
    gh_client: GitHubClient,
}

/// The functions here are essentially the fields on the RootQuery
impl IndicateAdapter<'_> {
    fn root_package(&self) -> VertexIterator<'static, Vertex> {
        let root = self.metadata.root_package().expect("no root package found");
        let v = Vertex::Package(Rc::new(root.clone()));
        Box::new(std::iter::once(v))
    }
}

/// Helper methods to resolve fields using the metadata
impl<'a> IndicateAdapter<'a> {
    pub fn new(metadata: &'a Metadata) -> Self {
        let mut packages = HashMap::with_capacity(metadata.packages.len());

        for p in &metadata.packages {
            let id = p.id.clone();
            let package = p.clone();
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
            let id = node.id.clone();
            let deps = node.dependencies.clone();
            direct_dependencies.insert(id, Rc::new(deps));
        }

        Self {
            metadata,
            packages,
            direct_dependencies,
            gh_client: GitHubClient::new(),
        }
    }

    fn dependencies(
        &self,
        package_id: &PackageId,
    ) -> VertexIterator<'static, Vertex> {
        let dependency_ids =
            self.direct_dependencies.get(package_id).unwrap_or_else(|| {
                panic!(
                    "Could not extract dependency IDs for package {}",
                    &package_id
                )
            });

        let dependencies = dependency_ids
            .iter()
            .map(|id| {
                let p = self.packages.get(id).unwrap();
                Vertex::Package(Rc::clone(p))
            })
            .collect::<Vec<Vertex>>()
            .into_iter();

        Box::new(dependencies)
    }

    /// Returns a form of repository, i.e. a variant that implements the
    /// `schema.trustfall.graphql` `repository` interface
    fn get_repository_from_url(&mut self, url: &str) -> Vertex {
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

                        if let Some(fr) = self.gh_client.get_repository(&id) {
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

/// Resolve the neighbor of a vertex, when it is known that the Vertex can be
/// downcast using an `as_<type>`. The passed closure will be used to resolve
/// the desired neighbors.
///
/// There is room for performance improvements here, as it must currently
/// collect an iterator to ensure lifetimes.
fn resolve_neighbors_with_collected<'a, V, F>(
    contexts: ContextIterator<'a, V>,
    mut resolve: F,
) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, V>>
where
    V: Clone + Debug + 'a,
    F: FnMut(&V) -> VertexIterator<'a, V>,
{
    Box::new(
        contexts
            .map(|ctx| {
                let current_vertex = &ctx.active_vertex();
                let neighbors_iter: VertexIterator<'a, V> = match current_vertex
                {
                    Some(v) => resolve(v),
                    None => Box::new(std::iter::empty()),
                };

                (ctx, neighbors_iter)
            })
            .collect::<Vec<(DataContext<V>, VertexIterator<'a, V>)>>()
            .into_iter(),
    )
}

impl<'a> BasicAdapter<'a> for IndicateAdapter<'a> {
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
                    created_at.into() // Convert to Unix timestamp
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
            ("Advisory", "severity") => todo!("enums not yet implemented"),
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
            ("Advisory", "cvss") => resolve_property_with(
                contexts,
                field_property!(as_advisory, metadata, {
                    match &metadata.cvss {
                        Some(_base) => todo!("enums not yet implemented"),
                        None => FieldValue::Null,
                    }
                }),
            ),
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
        _parameters: &EdgeParameters,
    ) -> ContextOutcomeIterator<
        'a,
        Self::Vertex,
        VertexIterator<'a, Self::Vertex>,
    > {
        // These are all possible neighboring vertexes, i.e. parts of a vertex
        // that are not scalar values (`FieldValue`)
        match (type_name, edge_name) {
            ("Package", "dependencies") => {
                resolve_neighbors_with_collected(contexts, |vertex| {
                    // This is in fact a Package, otherwise it would be `None`
                    // First get all dependencies, and then resolve their package
                    // by finding that dependency by its ID in the metadata
                    let package = vertex.as_package().unwrap();
                    self.dependencies(&package.id)
                })
            }
            ("Package", "repository") => {
                resolve_neighbors_with_collected(contexts, |v| {
                    // Must be package
                    let package = v.as_package().unwrap();
                    match &package.repository {
                        Some(url) => Box::new(std::iter::once(
                            self.get_repository_from_url(url),
                        )),
                        None => Box::new(std::iter::empty()),
                    }
                })
            }
            ("GitHubRepository", "owner") => {
                resolve_neighbors_with_collected(contexts, |vertex| {
                    // Must be GitHubRepository according to guarantees from Trustfall
                    let gh_repo = vertex.as_git_hub_repository().unwrap();
                    match &gh_repo.owner {
                        Some(simple_user) => {
                            let user = self
                                .gh_client
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
            ("Advisory", "affected") => {
                resolve_neighbors_with(contexts, |vertex| {
                    // Caller guarantees this is `Vertex::Advisory`
                    let advisory = vertex.as_advisory().unwrap();
                    match &advisory.affected {
                        Some(a) => Box::new(std::iter::once(Vertex::Affected(
                            Rc::new(a.clone()), // This `Rc` is ugly, but alternative might be uglier
                        ))),
                        None => Box::new(std::iter::empty::<Self::Vertex>()),
                    }
                })
            }
            ("Advisory", "versions") => {
                resolve_neighbors_with(contexts, |vertex| {
                    let advisory = vertex.as_advisory().unwrap();
                    Box::new(std::iter::once(Vertex::AffectedVersions(
                        Rc::new(advisory.versions.clone()),
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
        Box::new(
            contexts
                .map(|ctx| {
                    let current_vertex = &ctx.active_vertex();
                    let current_vertex = match current_vertex {
                        Some(v) => v,
                        None => return (ctx, false),
                    };

                    let can_coerce = match (
                        type_name as &str,
                        coerce_to_type
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
                .collect::<Vec<(DataContext<Self::Vertex>, bool)>>()
                .into_iter(),
        )
    }
}
