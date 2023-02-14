//! Includes the tokens that correspond to the types and relationships
//! defined by [`SCHEMA`](crate::SCHEMA).

/// A node im the GraphQL schema as defined in the schema.
///
/// Each node wraps a reference to some type of actual data.
#[derive(Debug, Clone)]
pub(crate) enum Node {}

#[cfg(test)]
mod test {
    use crate::SCHEMA;

    use super::Node;

    /// Verify that all `Node` variants are types in
    /// the schema, and that all types are nodes variants
    fn verify_nodes_in_schema() {
        todo!();
        //let types = SCHEMA.vertex_types
    }
}
