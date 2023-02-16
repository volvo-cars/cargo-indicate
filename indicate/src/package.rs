//! Module containing API to retrieve dependency information

use std::rc::Rc;

use cargo_metadata::{semver::Version, Metadata, PackageId};
use trustfall_core::interpreter::VertexIterator;

#[cfg(test)]
mod test {
    use std::path::Path;

    use crate::{extract_metadata_from_path, vertex::Vertex};

    const TEST_ROOT: &'static str = "test_data/fake_crates";

    macro_rules! fake_crate {
        ($name:literal) => {
            Path::new(&format!("{TEST_ROOT}/{}/Cargo.toml", $name))
        };
    }

    #[test]
    fn dependency_resolve() {
        let metadata =
            extract_metadata_from_path(fake_crate!("direct_dependencies"));
        println!("{:#?}", metadata.resolve.map(|n| n.nodes).unwrap());
    }
}
