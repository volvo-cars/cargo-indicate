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
        let root_package = self
            .metadata
            .root_package()
            .expect("found not root package in the metadata")
            .clone();
        let iterator = root_package
            .dependencies
            .into_iter()
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
        // TODO: While this does list all dependencies, it is without any true organization
        let packages = self.metadata.packages.clone();
        let iterator = packages
            .into_iter()
            .map(|p| p.dependencies.clone())
            .flatten()
            .map(|d| Node::Dependency(Rc::new(d)))
            .into_iter();
        Box::new(iterator)
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::{extract_metadata_from_path, node::Node};

    use super::DependencyAdapter;

    const TEST_ROOT: &'static str = "test_data/fake_crates";

    macro_rules! fake_crate {
        ($name:literal) => {
            format!("{TEST_ROOT}/{}/Cargo.toml", $name);
        };
    }

    #[test]
    fn direct_dependencies() {
        let r = fake_crate!("direct_dependencies");
        let metadata = extract_metadata_from_path(Path::new(&r));
        let da = DependencyAdapter::new(&metadata);
        let direct_deps: Vec<Node> = da.direct_dependencies().collect();

        assert_eq!(direct_deps.len(), 2);
    }
}
