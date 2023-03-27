use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};

use cargo_metadata::{CargoOpt, Metadata, MetadataCommand};
use walkdir::WalkDir;

use crate::errors::ManifestPathError;

/// The absolute path to a `Cargo.toml` file for a valid Rust package,
/// used to extract metadata and the like
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestPath(PathBuf);

impl ManifestPath {
    /// Attempts to create an absolute path to a Rust package `Cargo.toml` file
    fn absolute_manifest_path_from(
        path: &Path,
    ) -> Result<PathBuf, Box<dyn Error>> {
        let mut manifest_path = path.to_path_buf();

        if manifest_path.is_dir() && !manifest_path.ends_with("Cargo.toml") {
            manifest_path.push("Cargo.toml")
        }

        manifest_path = if !manifest_path.is_absolute() {
            fs::canonicalize(manifest_path)?
        } else {
            manifest_path
        };

        if !manifest_path.exists() {
            Err(Box::new(ManifestPathError::CouldNotCreateValidPath(
                manifest_path.to_string_lossy().into_owned(),
            )))
        } else {
            Ok(manifest_path)
        }
    }

    /// Checks if two package names is equal, using `crates.io` behaviour
    fn equal_package_names(s1: &str, s2: &str) -> bool {
        s1.replace('-', "_").to_lowercase()
            == s2.replace('-', "_").to_lowercase()
    }

    /// Creates a new, guaranteed valid, path to a `Cargo.toml` manifest
    ///
    /// If the path is not an absolute path to a `Cargo.toml` file, it will be
    /// attempted to be converted to it. If a directory is passed, it will be
    /// assumed to contain a `Cargo.toml` file
    pub fn new(path: PathBuf) -> Self {
        let manifest_path = Self::absolute_manifest_path_from(&path)
            .unwrap_or_else(|e| {
                let current_dir = std::env::current_dir()
                    .map(|p| p.to_string_lossy().into())
                    .unwrap_or(String::from("unknown"));
                panic!(
                    "path {} to package could not be resolved due to error: {e} (current dir is {})",
                    path.to_string_lossy(),
                    current_dir
                )
            });
        Self(manifest_path)
    }

    /// Creates a new, guaranteed valid, path to a `Cargo.toml` manifest
    /// where the package name _must_ match the provided name (handling `-` and
    /// `_` as the same character)
    ///
    /// Used when there is a possibility that the provided path contains a
    /// workspace `Cargo.toml` file. In this case, the path will be changed
    /// to point to the correct `Cargo.toml` file.
    ///
    /// The motivation for `_` and `-` handling is that they are considered the
    /// same character by `crates.io` and `cargo`, except in presentation.
    ///
    /// This requires `Metadata` to be parsed (twice), so only use
    /// when it is unsure if the target is a workspace. Otherwise use
    /// [`ManifestPath::new`].
    pub fn with_package_name(path: PathBuf, name: String) -> Self {
        let mut s = Self::new(path);

        let ctf = cargo_toml::Manifest::from_path(&s.0).unwrap_or_else(|e| {
            panic!(
                "could not parse manifest file {} due to error {e}",
                s.0.to_string_lossy()
            )
        });

        // Either package is none and it is a workspace, or it has a name not
        // equal to what we're looking for
        if ctf
            .package
            .map_or(true, |p| !Self::equal_package_names(&p.name(), &name))
        {
            // It is probably a workspace, we'll have to find a `Cargo.toml`
            // file with matching name

            // Remove `Cargo.toml`
            s.0.pop();
            let manifest_paths = WalkDir::new(s.0.as_path())
                .follow_links(true)
                .into_iter()
                .filter_map(|entry| match entry {
                    Ok(dir_entry) if dir_entry.file_name() == "Cargo.toml" => {
                        Some(dir_entry.into_path())
                    }
                    _ => None,
                });

            for manifest_path in manifest_paths {
                // Read the file, parse as toml, and see if package.name mathces
                let ct = cargo_toml::Manifest::from_path(&manifest_path);
                match ct {
                    Ok(parsed_config_toml)
                        if parsed_config_toml.package.is_some() =>
                    {
                        if Self::equal_package_names(
                            &parsed_config_toml.package.unwrap().name(),
                            &name,
                        ) {
                            return Self::new(manifest_path);
                        }
                    }
                    Ok(_) => {
                        continue;
                    }
                    Err(_) => {
                        // Might not be a manifest file at all
                        continue;
                    }
                }
            }

            panic!("did not manage to find a `Cargo.toml` manifest file matching the package name {name}");
        } else {
            s
        }
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Extracts metadata from a `Cargo.toml` file, using the features provided.
    ///
    /// Optionally provide a list of features to be used when creating the metadata,
    /// however some combinations may not be viable (see [`CargoOpt`]).
    ///
    /// May return a failure if the features provided are not of a possible
    /// combination (such as `AllFeatures` with `NoDefaultFeatures`).
    pub fn metadata(
        &self,
        features: Vec<CargoOpt>,
    ) -> Result<Metadata, Box<dyn Error>> {
        let mut m = MetadataCommand::new();
        m.manifest_path(self.as_path());

        for feature in features {
            m.features(feature);
        }

        let res = m.exec()?;
        Ok(res)
    }
}

impl<T> From<T> for ManifestPath
where
    T: AsRef<Path>,
{
    fn from(value: T) -> Self {
        fn inner(path: &Path) -> ManifestPath {
            ManifestPath::new(path.to_path_buf())
        }
        inner(value.as_ref())
    }
}
