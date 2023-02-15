//! Module containing API to retrieve dependency information

use std::rc::Rc;

use cargo_metadata::{DependencyKind, Metadata};
use trustfall_core::interpreter::VertexIterator;

use crate::node::Node;

pub(crate) struct DependencyAdapter<'a> {
    metadata: &'a Metadata,
}

impl<'a> DependencyAdapter<'a> {
    pub fn new(metadata: &'a Metadata) -> Self {
        Self { metadata }
    }

    /// Retrieves an iterator over all direct dependencies for the metadata file
    /// provided to create this adapter
    pub fn direct_dependencies(&self) -> VertexIterator<'static, Node> {
        let packages = self.metadata.packages.clone();
        let iterator = packages
            .into_iter()
            .map(|p| p.dependencies.clone())
            .flatten()
            .map(|d| Node::Dependency(Rc::new(d)))
            .into_iter();
        Box::new(iterator)
    }

    /// Retrieves an iterator of all dependencies (including recursive
    /// dependencies of dependencies) for the matadata file provided to create
    /// this adapter
    pub fn dependencies(
        &self,
        kind: DependencyKind,
    ) -> VertexIterator<'static, Node> {
        todo!()
    }
}
