use std::{cell::RefCell, rc::Rc};

use cargo_metadata::Metadata;

use crate::{advisory::AdvisoryClient, repo::github::GitHubClient};

use super::{parse_metadata, IndicateAdapter};

/// Builder for [`IndicateAdapter`]
pub struct IndicateAdapterBuilder {
    metadata: Metadata,
    gh_client: Option<GitHubClient>,
    advisory_client: Option<AdvisoryClient>,
}

impl IndicateAdapterBuilder {
    pub fn new(metadata: Metadata) -> IndicateAdapterBuilder {
        Self {
            metadata,
            gh_client: None,
            advisory_client: None,
        }
    }

    pub fn build(self) -> IndicateAdapter {
        let (packages, direct_dependencies) = parse_metadata(&self.metadata);
        IndicateAdapter {
            metadata: Rc::new(self.metadata),
            packages: Rc::new(packages),
            direct_dependencies: Rc::new(direct_dependencies),
            gh_client: Rc::new(RefCell::new(
                self.gh_client.unwrap_or_else(|| GitHubClient::new()),
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

    pub fn metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn gh_client(mut self, gh_client: GitHubClient) -> Self {
        self.gh_client = Some(gh_client);
        self
    }

    pub fn advisory_client(mut self, advisory_client: AdvisoryClient) -> Self {
        self.advisory_client = Some(advisory_client);
        self
    }
}
