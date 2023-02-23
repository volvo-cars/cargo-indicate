use std::{collections::HashMap, iter::Empty, rc::Rc, sync::Arc};

use cargo_metadata::{Metadata, Package, PackageId};
use trustfall::{
    provider::{
        accessor_property, field_property, resolve_property_with, BasicAdapter,
        ContextIterator, ContextOutcomeIterator, DataContext, EdgeParameters,
        VertexIterator,
    },
    FieldValue,
};

use crate::{github::GitHubClient, vertex::Vertex};

pub struct IndicateAdapter<'a> {
    metadata: &'a Metadata,
    packages: HashMap<PackageId, Rc<Package>>,

    /// Direct dependencies to a package, i.e. _not_ dependencies to dependencies
    direct_dependencies: HashMap<PackageId, Rc<Vec<PackageId>>>,
    github_client: GitHubClient<'a>,
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
            github_client: GitHubClient::new(),
        }
    }

    fn dependencies(
        &self,
        package_id: &PackageId,
    ) -> VertexIterator<'static, Vertex> {
        let dependency_ids = self
            .direct_dependencies
            .get(&package_id)
            .unwrap_or_else(|| {
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
}

/// The functions here are essentially the fields on the RootQuery
impl IndicateAdapter<'_> {
    fn root_package(&self) -> VertexIterator<'static, Vertex> {
        let root = self
            .metadata
            .root_package()
            .expect("No root package found!");
        let v = Vertex::Package(Rc::new(root.clone()));
        Box::new(std::iter::once(v))
    }
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
            _ => unreachable!(),
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
                    None => FieldValue::Null,
                    Some(url) => FieldValue::String(url.to_owned()),
                })
            }
            ("GitHubRepository", "name") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, name),
            ),
            ("GitHubRepository", "starCount") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, stargazers_count),
            ),
            ("GitHubRepository", "forksCount") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, forks_count),
            ),
            ("GitHubRepository", "hasIssues") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_repository, has_issues, {
                    (*has_issues).into()
                }),
            ),
            ("GitHubUser", "name") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_user, name),
            ),
            ("GitHubUser", "createdAt") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_user, created_at),
            ),
            ("GitHubUser", "followersCount") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_user, followers),
            ),
            ("GitHubUser", "email") => resolve_property_with(
                contexts,
                field_property!(as_git_hub_user, email),
            ),
            _ => unreachable!(),
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
                // First get all dependencies, and then resolve their package
                // by finding that dependency by its ID in the metadata
                let res = contexts
                    .map(|ctx| {
                        let current_vertex = &ctx.active_vertex();
                        let neighbors_iter: VertexIterator<'a, Self::Vertex> =
                            match current_vertex {
                                None => Box::new(std::iter::empty()),
                                Some(vertex) => {
                                    // This is in fact a Package, otherwise it would be `None`
                                    let package = vertex.as_package().unwrap();
                                    self.dependencies(&package.id)
                                }
                            };
                        (ctx, neighbors_iter)
                    })
                    .collect::<Vec<(
                        DataContext<Self::Vertex>,
                        VertexIterator<'a, Self::Vertex>,
                    )>>()
                    .into_iter();

                Box::new(res)
            }
            ("Package", "repository") => {
                let res = contexts
                    .map(|ctx| {
                        let current_vertex = &ctx.active_vertex();
                        let neighbors_iter: VertexIterator<'a, Self::Vertex> =
                            match current_vertex {
                                None => Box::new(std::iter::empty()),
                                Some(v) => {
                                    // Must be package
                                    let package = v.as_package().unwrap();
                                    match &package.repository {
                                        Some(url) => Box::new(std::iter::once(
                                            get_repository_from_url(&url),
                                        )),
                                        None => Box::new(std::iter::empty()),
                                    }
                                }
                            };

                        (ctx, neighbors_iter)
                    })
                    .collect::<Vec<(
                        DataContext<Self::Vertex>,
                        VertexIterator<'a, Self::Vertex>,
                    )>>()
                    .into_iter();

                Box::new(res)
            }
            ("GitHubRepository", "owner") => {
                let res = contexts
                    .map(|ctx| {
                        let current_vertex = &ctx.active_vertex();
                        let neighbors_iter: VertexIterator<'a, Self::Vertex> =
                            match current_vertex {
                                None => Box::new(std::iter::empty()),
                                Some(vertex) => {
                                    // Must be GitHubRepository according to guarantees from Trustfall
                                    let gh_repo =
                                        vertex.as_git_hub_repository().unwrap();
                                    match &gh_repo.owner {
                                        None => Box::new(std::iter::empty()),
                                        Some(simple_user) => {
                                            let user = self
                                                .github_client
                                                .get_public_user(
                                                    &simple_user.name,
                                                );

                                            // TODO: A bit sketchy error handling here
                                            match user {
                                                None => {
                                                    Box::new(std::iter::empty())
                                                }
                                                Some(u) => {
                                                    Box::new(std::iter::once(
                                                        Vertex::GitHubUser(
                                                            Arc::clone(&u),
                                                        ),
                                                    ))
                                                }
                                            }
                                        }
                                    }
                                }
                            };

                        (ctx, neighbors_iter)
                    })
                    .collect::<Vec<(
                        DataContext<Self::Vertex>,
                        VertexIterator<'a, Self::Vertex>,
                    )>>()
                    .into_iter();

                Box::new(res)
            }
            _ => unreachable!(),
        }
    }

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'a, Self::Vertex>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
        todo!()
    }
}

/// Returns a form of repository, i.e. a variant that implements the
/// `schema.trustfall.graphql` `repository` interface
fn get_repository_from_url(url: &str) -> Vertex {
    todo!()
}
