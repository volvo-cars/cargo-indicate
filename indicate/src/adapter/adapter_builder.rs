use std::{cell::RefCell, rc::Rc};

use cargo_metadata::{CargoOpt, Metadata};

use crate::{
    advisory::AdvisoryClient, repo::github::GitHubClient, ManifestPath,
};

use super::{parse_metadata, IndicateAdapter};

/// Builder for [`IndicateAdapter`]
pub struct IndicateAdapterBuilder {
    manifest_path: ManifestPath,
    features: Vec<CargoOpt>,
    metadata: Option<Metadata>,
    github_client: Option<GitHubClient>,
    advisory_client: Option<AdvisoryClient>,
}

impl IndicateAdapterBuilder {
    /// Creates a new builder for a [`IndicateAdapter`]
    ///
    /// Without any manual calls to set the fields of the future adapter, it
    /// will produce the same adapter as [`IndicateAdapter::new`]. This means
    /// that default features will be used when parsing metadata, if features
    /// are not set using [`IndicateAdapterBuilder::features`].
    pub fn new(manifest_path: ManifestPath) -> IndicateAdapterBuilder {
        Self {
            manifest_path,
            features: Vec::new(),
            metadata: None,
            github_client: None,
            advisory_client: None,
        }
    }

    /// Will build the [`IndicateAdapter`]
    ///
    /// If metadata is not explicitly set, one will be generated using the
    /// features provided (or if none, default features).
    ///
    /// Will panic if both features and metadata have been set manually.
    pub fn build(self) -> IndicateAdapter {
        if !self.features.is_empty() && self.metadata.is_some() {
            panic!(
                "features and metadata both set explicitly at the same time"
            );
        }

        let metadata = match self.metadata {
            Some(m) => m,
            None => {
                self.manifest_path
                    .metadata(self.features)
                    .unwrap_or_else(|e| {
                        panic!("could not generate metadata due to error: {e}")
                    })
            }
        };

        // unwrap OK, if-statement above guarantees self.metadata to exist
        let (packages, direct_dependencies) = parse_metadata(&metadata);
        IndicateAdapter {
            manifest_path: Rc::new(self.manifest_path),
            metadata: Rc::new(metadata),
            packages: Rc::new(packages),
            direct_dependencies: Rc::new(direct_dependencies),
            gh_client: Rc::new(RefCell::new(
                self.github_client.unwrap_or_else(GitHubClient::new),
            )),
            advisory_client: Rc::new(
                self.advisory_client
                    .unwrap_or_else(|| {
                        AdvisoryClient::new()
                        .unwrap_or_else(|e| {
                                panic!("could not create advisory client due to error: {e}")
                            })
                    }),
            ),
        }
    }

    /// Features used when generating metadata
    ///
    /// Cannot be set explicitly at the same time as metadata, and will
    /// cause a panic when built.
    pub fn features(mut self, features: Vec<CargoOpt>) -> Self {
        self.features = features;
        self
    }

    /// Explicitly set metadata for the adapter
    ///
    /// Note that this metadata will prevent one from being generated using
    /// [`IndicateAdapterBuilder::features`], causing a panic if both are set
    /// when [`IndicateAdapterBuilder::build`] is called.
    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Manually sets the GitHub client used by the adapter
    pub fn github_client(mut self, github_client: GitHubClient) -> Self {
        self.github_client = Some(github_client);
        self
    }

    /// Manually sets the `advisory-db` client used by the adapter
    pub fn advisory_client(mut self, advisory_client: AdvisoryClient) -> Self {
        self.advisory_client = Some(advisory_client);
        self
    }
}

impl From<IndicateAdapterBuilder> for IndicateAdapter {
    fn from(value: IndicateAdapterBuilder) -> Self {
        value.build()
    }
}
