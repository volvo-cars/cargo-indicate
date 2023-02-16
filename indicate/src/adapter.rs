use std::{collections::BTreeMap, rc::Rc};

use cargo_metadata::{Dependency, Metadata, Package, PackageId};
use trustfall_core::{
    field_property,
    interpreter::{
        basic_adapter::BasicAdapter, helpers::resolve_property_with,
        VertexIterator,
    },
    ir::FieldValue,
};

use crate::vertex::Vertex;

struct IndicateAdapter {
    metadata: Metadata,
    packages: BTreeMap<PackageId, Rc<Package>>,

    /// Direct dependencies to a package, i.e. _not_ dependencies to dependencies
    direct_dependencies: BTreeMap<PackageId, Rc<Vec<PackageId>>>,
}

/// Helper methods to resolve fields using the metadata
impl IndicateAdapter {
    fn new(metadata: Metadata) -> Self {
        let mut packages = BTreeMap::new();
        metadata
            .packages
            .iter()
            .map(|p| packages.insert(p.id.clone(), Rc::new(p.clone())));
        let mut direct_dependencies = BTreeMap::new();
        metadata.resolve.as_ref().unwrap().nodes.iter().map(|n| {
            direct_dependencies
                .insert(n.id.clone(), Rc::new(n.dependencies.clone()))
        });

        Self {
            metadata,
            packages,
            direct_dependencies,
        }
    }
}

/// The functions here are essentially the fields on the RootQuery
impl IndicateAdapter {
    fn root_package(&self) -> VertexIterator<'static, Vertex> {
        let root = self
            .metadata
            .root_package()
            .expect("No root package found!");
        let v = Vertex::Package(Rc::new(root.clone().into()));
        Box::new(std::iter::once(v))
    }
}

impl BasicAdapter<'static> for IndicateAdapter {
    type Vertex = Vertex;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: &str,
        _parameters: Option<&trustfall_core::ir::EdgeParameters>,
    ) -> VertexIterator<'static, Self::Vertex> {
        match edge_name {
            // These edge names should match 1:1 for `schema.trustfall.graphql`
            "RootPackage" => self.root_package(),
            _ => unreachable!(),
        }
    }

    fn resolve_property(
        &mut self,
        contexts: trustfall_core::interpreter::ContextIterator<
            'static,
            Self::Vertex,
        >,
        type_name: &str,
        property_name: &str,
    ) -> trustfall_core::interpreter::ContextOutcomeIterator<
        'static,
        Self::Vertex,
        trustfall_core::ir::FieldValue,
    > {
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
            _ => unreachable!(),
        }
    }

    fn resolve_neighbors(
        &mut self,
        contexts: trustfall_core::interpreter::ContextIterator<
            'static,
            Self::Vertex,
        >,
        type_name: &str,
        edge_name: &str,
        _parameters: Option<&trustfall_core::ir::EdgeParameters>,
    ) -> trustfall_core::interpreter::ContextOutcomeIterator<
        'static,
        Self::Vertex,
        VertexIterator<'static, Self::Vertex>,
    > {
        // These are all possible neighboring vertexes, i.e. parts of a vertex
        // that are not scalar values (`FieldValue`)
        match (type_name, edge_name) {
            ("Package", "dependencies") => {
                // First get all dependencies, and then resolve their package
                // by finding that dependency by its ID in the metadata
                Box::new(contexts.map(|ctx| {
                    let current_vertex = &ctx.current_token;
                    let neighbors_iter: VertexIterator<'static, Self::Vertex> =
                        match current_vertex {
                            None => Box::new(std::iter::empty()),
                            Some(vertex) => {
                                // This is in fact a Package, otherwise it would be `None`
                                let package = vertex.as_package().unwrap();

                                let dependency_ids = self.direct_dependencies
                                    .get(&package.id)
                                    .expect(&format!("Could not extract dependency IDs for package {}", &package.id));

                                let dependencies = dependency_ids
                                    .iter()
                                    .map(|id| {
                                        let p = self.packages.get(id).unwrap();
                                        Vertex::Package(p.clone())
                                    });

                                Box::new(dependencies)
                            }
                        };
                    (ctx, neighbors_iter)
                }))
            }
            _ => unreachable!(),
        }
    }

    fn resolve_coercion(
        &mut self,
        contexts: trustfall_core::interpreter::ContextIterator<
            'static,
            Self::Vertex,
        >,
        type_name: &str,
        coerce_to_type: &str,
    ) -> trustfall_core::interpreter::ContextOutcomeIterator<
        'static,
        Self::Vertex,
        bool,
    > {
        todo!()
    }
}
