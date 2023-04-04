use std::{cell::RefCell, rc::Rc};

use cargo_metadata::{CargoOpt, Metadata};
use once_cell::unsync::OnceCell;

use crate::{
    advisory::AdvisoryClient, geiger::GeigerClient, repo::github::GitHubClient,
    util, ManifestPath,
};

use super::IndicateAdapter;

/// Builder for [`IndicateAdapter`]
pub struct IndicateAdapterBuilder {
    manifest_path: ManifestPath,
    features: Vec<CargoOpt>,
    metadata: Option<Metadata>,
    github_client: Option<GitHubClient>,
    advisory_client: Option<AdvisoryClient>,
    geiger_client: Option<GeigerClient>,
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
            geiger_client: None,
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
            None => self
                .manifest_path
                .metadata(self.features.clone())
                .unwrap_or_else(|e| {
                    panic!("could not generate metadata due to error: {e}")
                }),
        };

        // unwrap OK, if-statement above guarantees self.metadata to exist
        let (packages, direct_dependencies) = util::parse_metadata(&metadata);
        let advisory_client = self
            .advisory_client
            .map(|ac| OnceCell::with_value(Rc::new(ac)))
            .unwrap_or_else(OnceCell::new);
        let geiger_client = self
            .geiger_client
            .map(|gc| OnceCell::with_value(Rc::new(gc)))
            .unwrap_or_else(OnceCell::new);

        IndicateAdapter {
            manifest_path: Rc::new(self.manifest_path),
            features: self.features,
            metadata: Rc::new(metadata),
            packages: Rc::new(packages),
            direct_dependencies: Rc::new(direct_dependencies),
            gh_client: Rc::new(RefCell::new(
                self.github_client.unwrap_or_default(),
            )),
            advisory_client,
            geiger_client,
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

    /// Manually sets the `cargo-geiger` client used by the adapter
    ///
    /// This should generally not be done, since it is an expensive operation to
    /// run `cargo-geiger`; Instead set the desired `manifest_path` and features,
    /// which will make a lazily evaluated [`GeigerClient`] be available to the
    /// adapter.
    pub fn geiger_client(mut self, geiger_client: GeigerClient) -> Self {
        self.geiger_client = Some(geiger_client);
        self
    }
}

impl From<IndicateAdapterBuilder> for IndicateAdapter {
    fn from(value: IndicateAdapterBuilder) -> Self {
        value.build()
    }
}
