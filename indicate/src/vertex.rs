//! Includes the tokens that correspond to the types and relationships
//! defined by [`SCHEMA`](crate::SCHEMA).

use std::{rc::Rc, sync::Arc};

use cargo_metadata::Package;
use octorust::types::{FullRepository, PublicUser};
use trustfall::provider::TrustfallEnumVertex;

/// A node in the GraphQL schema as defined in the schema.
///
/// Each node wraps a reference to some type of actual data.
#[derive(Debug, Clone, TrustfallEnumVertex)]
pub enum Vertex {
    Package(Rc<Package>),

    #[trustfall(skip_conversion)]
    Webpage(String),

    #[trustfall(skip_conversion)]
    Repository(String),
    GitHubRepository(Arc<FullRepository>),
    GitHubUser(Arc<PublicUser>),
}

impl Vertex {
    pub fn as_webpage(&self) -> Option<&str> {
        match self {
            Vertex::Webpage(url) => Some(url.as_ref()),
            Vertex::Repository(url) => Some(url.as_ref()),
            Vertex::GitHubRepository(r) => Some(&r.url),
            _ => None,
        }
    }

    pub fn as_repository(&self) -> Option<&str> {
        match self {
            Vertex::Repository(url) => Some(url.as_ref()),
            Vertex::GitHubRepository(r) => Some(&r.url),
            _ => None,
        }
    }
}

impl From<Package> for Vertex {
    fn from(value: Package) -> Self {
        Self::Package(Rc::new(value))
    }
}

#[cfg(test)]
mod test {
    /// Verify that all `Vertex` variants are types in
    /// the schema, and that all types are nodes variants
    #[test]
    #[ignore = "not possible at this point"]
    fn verify_nodes_in_schema() {
        // TODO: Use trustfall_core::schema::Schema to
        // access vertexes
        todo!()
    }
}
