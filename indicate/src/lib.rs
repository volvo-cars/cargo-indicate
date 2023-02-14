use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

use lazy_static::lazy_static;
use serde::Deserialize;
use trustfall_core::{ir::FieldValue, schema::Schema};

lazy_static! {
    static ref SCHEMA: Schema = {
        Schema::parse(include_str!("schema.trustfall.graphql"))
            .expect("Could not parse schema!")
    };
}

/// Type representing a thread-safe JSON object, like
/// ```json
/// {
///     "name": "hello",
///     "value": true,
/// }
/// ```
type ObjectMap = Arc<BTreeMap<Arc<str>, FieldValue>>;

/// Struct representing a query to `indicate`
#[derive(Debug, Clone, Deserialize)]
struct Query<'a> {
    query: &'a str,
    args: ObjectMap,
}

pub fn execute_query(path: &Path) {
    let raw_query =
        fs::read_to_string(path).expect("Could not read query at {path}!");
    let query: Query = ron::from_str(&raw_query)
        .expect("Could not parse the raw query as .ron!");
    todo!("Use the adapter")
}
