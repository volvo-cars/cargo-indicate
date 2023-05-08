use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

use cargo_metadata::{DependencyKind, Metadata, Package};
use trustfall::{FieldValue, TransparentValue};

use crate::adapter::{DirectDependencyMap, PackageMap};

/// Transform a result from [`execute_query`](trustfall::execute_query) to one where the fields can easily
/// be serialized to JSON using [`TransparentValue`].
pub fn transparent_results(
    res: Vec<BTreeMap<Arc<str>, FieldValue>>,
) -> Vec<BTreeMap<Arc<str>, TransparentValue>> {
    res.into_iter()
        .map(|entry| entry.into_iter().map(|(k, v)| (k, v.into())).collect())
        .collect()
}

/// Retrieves the path to a package downloaded locally
///
/// Most likely in the `~/.cargo/registry/` directory.
pub fn local_package_path(package: &Package) -> PathBuf {
    let mut p = package.manifest_path.to_owned().into_std_path_buf();

    // Remove `Cargo.toml`
    p.pop();
    p
}

/// Parse metadata to create a map over direct dependencies for all packages
///
/// Direct dependencies will only include 'normal' dependencies, i.e.
/// not build nor test deps.
///
/// _Note_: This operation is quite expensive as it must traverse the dependency
/// tree. Avoid if not required.
pub fn get_direct_dependencies(metadata: &Metadata) -> DirectDependencyMap {
    let mut direct_dependencies =
        HashMap::with_capacity(metadata.packages.len());

    for node in metadata
        .resolve
        .as_ref()
        .expect("No nodes found!")
        .nodes
        .iter()
    {
        let id = node.id.to_owned();

        // Filter out dependencies that are not normal
        let normal_deps = node
            .deps
            .iter()
            .filter_map(|nd| {
                if nd
                    .dep_kinds
                    .iter()
                    .any(|dki| dki.kind == DependencyKind::Normal)
                {
                    // A dependency can have many kinds; We only care if it is
                    // normal
                    Some(nd.pkg.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        direct_dependencies.insert(id, Rc::new(normal_deps));
    }

    direct_dependencies
}

/// Parse metadata to create a map over packages
pub fn get_packages(
    metadata: &Metadata,
) -> PackageMap {
    let mut packages = HashMap::with_capacity(metadata.packages.len());

    for p in &metadata.packages {
        let id = p.id.to_owned();
        let package = p.to_owned();
        packages.insert(id, Rc::new(package));
    }

    packages
}
