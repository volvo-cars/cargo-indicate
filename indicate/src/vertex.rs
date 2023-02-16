//! Includes the tokens that correspond to the types and relationships
//! defined by [`SCHEMA`](crate::SCHEMA).

use std::rc::Rc;

use cargo_metadata::Package;

/// A node in the GraphQL schema as defined in the schema.
///
/// Each node wraps a reference to some type of actual data.
#[derive(Debug, Clone)]
pub(crate) enum Vertex {
    Package(Rc<Package>),
}

impl Vertex {
    /// Provides the `__typename` property
    pub fn typename(&self) -> &'static str {
        match self {
            Vertex::Package(_) => "Package",
            _ => unreachable!(),
        }
    }

    pub fn as_package(&self) -> Option<&Package> {
        match self {
            Vertex::Package(d) => Some(d.as_ref()),
            _ => None,
        }
    }
}

impl From<Package> for Vertex {
    fn from(value: Package) -> Self {
        Self::Package(Rc::new(value.into()))
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
