use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

use cargo_metadata::{Metadata, MetadataCommand};
use lazy_static::lazy_static;
use serde::Deserialize;
use trustfall_core::{ir::FieldValue, schema::Schema};

mod adapter;
mod vertex;

const RAW_SCHEMA: &'static str = include_str!("schema.trustfall.graphql");

lazy_static! {
    static ref SCHEMA: Schema =
        Schema::parse(RAW_SCHEMA).expect("Could not parse schema!");
}

/// Type representing a thread-safe JSON object, like
/// ```json
/// {
///     "name": "hello",
///     "value": true,
/// }
/// ```
type ObjectMap = BTreeMap<Arc<str>, FieldValue>;

/// Struct representing a query to `indicate`
#[derive(Debug, Clone, Deserialize)]
struct Query<'a> {
    query: &'a str,
    args: Arc<ObjectMap>,
}

/// Executes a Trustfall query at a defined path, using the schema
/// provided by `indicate`.
pub fn execute_query(path: &Path) {
    let raw_query =
        fs::read_to_string(path).expect("Could not read query at {path}!");
    let query: Query = ron::from_str(&raw_query)
        .expect("Could not parse the raw query as .ron!");
    todo!("Use the adapter")
}

/// Extracts metadata from a `Cargo.toml` file by its direct path
pub fn extract_metadata_from_path(path: &Path) -> Metadata {
    MetadataCommand::new()
        .manifest_path(path)
        .exec()
        .expect(&format!("Could not extract metadata from path {:?}", path))
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::extract_metadata_from_path;

    const TEST_ROOT: &'static str = "test_data/fake_crates";

    macro_rules! fake_crate {
        ($name:literal) => {
            Path::new(&format!("{TEST_ROOT}/{}/Cargo.toml", $name))
        };
    }

    #[test]
    #[ignore = "debugging purposes"]
    fn dependency_resolve() {
        let metadata =
            extract_metadata_from_path(fake_crate!("direct_dependencies"));
        println!("{:#?}", metadata.resolve.map(|n| n.nodes).unwrap());
    }
}
