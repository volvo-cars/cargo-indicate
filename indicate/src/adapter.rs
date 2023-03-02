use std::{cell::RefCell, collections::HashMap, fmt::Debug, rc::Rc, sync::Arc};

use cargo_metadata::{Metadata, Package, PackageId};
use git_url_parse::GitUrl;
use trustfall::{
    provider::{
        field_property, resolve_neighbors_with, resolve_property_with,
        BasicAdapter, ContextIterator, ContextOutcomeIterator, DataContext,
        EdgeParameters, VertexIterator,
    },
    FieldValue,
};

use crate::{
    repo::github::{GitHubClient, GitHubRepositoryId},
    vertex::Vertex,
};

type DirectDependencyMap = HashMap<PackageId, Rc<Vec<PackageId>>>;
type PackageMap = HashMap<PackageId, Rc<Package>>;

pub struct IndicateAdapter {
    metadata: Rc<Metadata>,
    packages: Rc<PackageMap>,

    /// Direct dependencies to a package, i.e. _not_ dependencies to dependencies
    direct_dependencies: Rc<DirectDependencyMap>,
    gh_client: Rc<RefCell<GitHubClient>>,
}

/// Helper methods to resolve fields using the metadata
impl IndicateAdapter {
    pub fn new(metadata: Metadata) -> Self {
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
            metadata: Rc::new(metadata),
            packages: Rc::new(packages),
            direct_dependencies: Rc::new(direct_dependencies),
            gh_client: Rc::new(RefCell::new(GitHubClient::new())),
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

/// Resolve the neighbor of a vertex, when it is known that the Vertex can be
/// downcast using an `as_<type>`. The passed closure will be used to resolve
/// the desired neighbors.
///
/// There is room for performance improvements here, as it must currently
/// collect an iterator to ensure lifetimes.
// fn resolve_neighbors_with_collect<'a, V, F>(
//     contexts: ContextIterator<'a, V>,
//     mut resolve: F,
// ) -> ContextOutcomeIterator<'a, V, VertexIterator<'a, V>>
// where
//     V: Clone + Debug + 'a,
//     F: FnMut(&V) -> VertexIterator<'a, V>,
// {
//     Box::new(
//         contexts
//             .map(|ctx| {
//                 let current_vertex = &ctx.active_vertex();
//                 let neighbors_iter: VertexIterator<'a, V> = match current_vertex
//                 {
//                     Some(v) => resolve(v),
//                     None => Box::new(std::iter::empty()),
//                 };

//                 (ctx, neighbors_iter)
//             })
//             .collect::<Vec<(DataContext<V>, VertexIterator<'a, V>)>>()
//             .into_iter(),
//     )
// }

/// The functions here are essentially the fields on the RootQuery
impl IndicateAdapter {
    fn root_package(&self) -> VertexIterator<'static, Vertex> {
        let root = self.metadata.root_package().expect("no root package found");
        let v = Vertex::Package(Rc::new(root.clone()));
        Box::new(std::iter::once(v))
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
                    Some(l) => l.into(),
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
