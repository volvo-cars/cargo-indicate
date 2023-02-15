//! Includes the tokens that correspond to the types and relationships
//! defined by [`SCHEMA`](crate::SCHEMA).

use std::rc::Rc;

use cargo_metadata::Dependency;

/// A node im the GraphQL schema as defined in the schema.
///
/// Each node wraps a reference to some type of actual data.
#[derive(Debug, Clone)]
pub(crate) enum Node {
    Dependency(Rc<Dependency>),
}

impl Node {
    /// Provides the `__typename` property
    pub fn typename(&self) -> &'static str {
        match self {
            Node::Dependency(_) => "Crate",
            _ => unreachable!(),
        }
    }

    pub fn as_crate(&self) -> Option<&Dependency> {
        match self {
            Node::Dependency(d) => Some(d.as_ref()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod test {
    /// Verify that all `Node` variants are types in
    /// the schema, and that all types are nodes variants
    #[test]
    #[ignore = "not possible at this point"]
    fn verify_nodes_in_schema() {
        // TODO: Use trustfall_core::schema::Schema to
        // access vertexes
        todo!()
    }
}
