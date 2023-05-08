//! Client and methods for retrieving information from the crates.io API
//!
//! _Note_: Due to the crates.io crawler policy, the amount of requests that
//! can be made is limited. [`CratesIoClient`] attempts to make this less
//! noticeable with caching and doing large fetches, but please keep this in
//! mind.
//! 
//! See [the crates.io crawler policy](https://crates.io/policies#crawlers) for
//! more information.

use std::{collections::HashMap, time::Duration};

use crates_io_api::{SyncClient, Crate, CrateResponse, Version};

use crate::NameVersion;

/// Wrapper around a [`crates_io_api::SyncClient`], with added caching
pub struct CratesIoClient {
    client: SyncClient,

    /// Cache between crate name and downloads info
    ///
    /// We do not want requests for the same crate to fail and then work in the
    /// same query, so we store if we were able to find it the first time via
    /// the option.
    cache: HashMap<String, Option<CrateResponse>>,
}

impl CratesIoClient {
    /// Creates a new `crates.io` client and cache
    ///
    /// # Panics
    ///
    /// Panics if the given agent parameters are invalid.
    #[must_use]
    pub fn new(user_agent: &str, rate_limit: Duration) -> Self {
        let client = SyncClient::new(user_agent, rate_limit).unwrap_or_else(|e| {
            panic!("could not create CratesIoClient due to error: {e}");
        });

        Self {
            client,
            cache: HashMap::new(),
        }
    }

    /// Retrieves information about a crate from the `crates.io` API
    ///
    /// Will return `None` if the request fails, and will cache this crate as
    /// such.
    pub fn crate_response(&mut self, crate_name: &str) -> Option<&mut CrateResponse> {
        self.cache.entry(crate_name.to_string()).or_insert_with(|| {
           match self.client.get_crate(crate_name)  {
                Ok(cr) => Some(cr),
                Err(e) => {
                    eprintln!("failed to retrieve crates.io information about {crate_name} due to error: {e}");
                    None
                }
            }
        }).as_mut()
       }

    /// Retrieve data about a crate from the `crates.io` API
    pub fn crate_data(&mut self, crate_name: &str) -> Option<&Crate> {
        self.crate_response(crate_name).map(|cr| &cr.crate_data)
    }

    /// Retrieves data about all versions of a crate from the `crates.io` API
    pub fn versions(&mut self, crate_name: &str) -> Option<&Vec<Version>> {
        self.crate_response(crate_name).map(|cr| &cr.versions)
    }

    /// Returns the number of versions of a crate from the `crates.io` API
    pub fn versions_count(&mut self, crate_name: &str) -> Option<usize> {
        self.versions(crate_name).map(Vec::len)
    }

    /// Retrieves the total amount of downloads for a crate, all versions
    pub fn total_downloads(&mut self, crate_name: &str) -> Option<u64> {
        self.crate_data(crate_name).map(|c| c.downloads)
    }

    /// Retrieves the total amount of downloads for a crate, all versions
    pub fn recent_downloads(&mut self, crate_name: &str) -> Option<u64> {
        self.crate_data(crate_name).and_then(|c| c.recent_downloads)
    }

    /// Retrieves the total amount of downloads for a specific crate version
    pub fn version_downloads(&mut self, name_version: &NameVersion) -> Option<u64> {
        self.versions(&name_version.name).and_then(|versions| {
            versions.iter().find_map(|v| {
            match rustsec::Version::parse(&v.num) {
                    Ok(current_version) => {
                        if current_version == name_version.version {
                            Some(v.downloads)
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        eprintln!("could not parse crates.io version for {name_version:?} due to error: {e}");
                        None
                    }
                }        })        })
    }

    /// Returns if this version is yanked from `crates.io`
    pub fn yanked(&mut self, name_version: &NameVersion) -> Option<bool> {
        self.versions(&name_version.name).and_then(|versions| {
            versions.iter().find_map(|v| {
                match rustsec::Version::parse(&v.num) {
                    Ok(current_version) => {
                        if current_version == name_version.version {
                            Some(v.yanked)
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        eprintln!("could not parse crates.io version for {name_version:?} due to error: {e}");
                        None
                    }
                }
            })
        })
    }

    /// Retrieves all versions for a crate that has been marked as yanked
    ///
    /// If only the count of yanked versions is desired, use
    /// [`yanked_versions_count`](Self::yanked_versions_count) instead.
    pub fn yanked_versions(&mut self, crate_name: &str) -> Option<Vec<String>> {
        self.versions(crate_name).map(|versions| {
            versions.iter().filter_map(|fv| {
                if fv.yanked {
                    // We do not parse version, as that may fail, leading
                    // to odd results
                    Some(fv.num.clone())
                } else {
                    None
                }
            }).collect()
        })
    }

    /// Counts the number of versions marked as _yanked_ on `crates.io` for this
    /// crate
    pub fn yanked_versions_count(&mut self, crate_name: &str) -> Option<usize> {
        // Do not rely on Self::yanked_version, as it is more expensive
        self.versions(crate_name).map(|versions| {
            versions.iter().filter(|v| v.yanked).count()
        })
    }

    /// Calculates the ratio of yanked versions to all crate versions
    pub fn yanked_ratio(&mut self, crate_name: &str) -> Option<f64> {
        self.yanked_versions_count(crate_name).and_then(|y| {
           self.versions_count(crate_name).map(|v| y as f64 / v as f64)    
        }
)
    }
}

impl Default for CratesIoClient {
    fn default() -> Self {
        let user_agent = std::env::var("USER_AGENT")
            .expect("USER_AGENT environment variable not set");
        Self::new(&user_agent, Duration::from_secs(1))
    }
}