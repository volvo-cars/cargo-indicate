#![deny(unsafe_code)]
#![feature(iter_collect_into)]
use std::{
    cell::RefCell, collections::BTreeMap, fs, path::Path, rc::Rc, sync::Arc,
};

use adapter::IndicateAdapter;
use cargo_metadata::{Metadata, MetadataCommand};
use lazy_static::lazy_static;
use trustfall::{execute_query as trustfall_execute_query, FieldValue, Schema};

mod adapter;
mod vertex;

const RAW_SCHEMA: &str = include_str!("schema.trustfall.graphql");

lazy_static! {
    static ref SCHEMA: Schema =
        Schema::parse(RAW_SCHEMA).expect("Could not parse schema!");
}

/// Executes a Trustfall query at a defined path, using the schema
/// provided by `indicate`.
pub fn execute_query(query_path: &Path, metadata_path: &Path) {
    let raw_query = fs::read_to_string(query_path)
        .expect("Could not read query at {path}!");

    let metadata = extract_metadata_from_path(metadata_path);
    let adapter = Rc::new(RefCell::new(IndicateAdapter::new(&metadata)));
    let res = trustfall_execute_query(
        &SCHEMA,
        adapter,
        &raw_query,
        BTreeMap::new() as BTreeMap<Arc<str>, FieldValue>,
    );

    match res {
        Err(e) => panic!("Could not execute query due to error: {}", e),
        Ok(i) => {
            for r in i {
                println!("{:#?}", r);
            }
        }
    }
}

/// Extracts metadata from a `Cargo.toml` file by its direct path
pub fn extract_metadata_from_path(path: &Path) -> Metadata {
    MetadataCommand::new()
        .manifest_path(path)
        .exec()
        .unwrap_or_else(|_| {
            panic!("Could not extract metadata from path {:?}", path)
        })
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
