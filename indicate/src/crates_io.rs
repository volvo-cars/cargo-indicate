//! Client and methods for retrieving information from the crates.io API
//!
//! _Note_: Due to the crates.io crawler policy, the amount of requests that
//! can be made is limited. [`CratesIoClient`] attempts to make this less
//! noticeable with caching and doing large fetches, but please keep this in
//! mind.
//! 
//! See https://crates.io/policies#crawlers for more information.

use std::{collections::HashMap, time::Duration, rc::Rc};

use crates_io_api::{SyncClient, FullCrate};
use rustsec::Version;

use crate::NameVersion;

/// Wrapper around a [`crates_io_api::SyncClient`], with added caching
pub struct CratesIoClient {
    client: SyncClient,

    /// Cache between crate name and information about it
    cache: HashMap<String, Rc<FullCrate>>,
}

impl CratesIoClient {
    pub fn new(user_agent: &str, rate_limit: Duration) -> Self {
        let client = SyncClient::new(user_agent, rate_limit).unwrap_or_else(|e| {
            panic!("could not create CratesIoClient due to error: {e}");
        });

        Self {
            client,
            cache: HashMap::new(),
        }
    }

    pub fn full_crate(&mut self, crate_name: &str) -> Option<Rc<FullCrate>> {
        // TODO: Remove this Rc
        match self.cache.get(crate_name) {
            Some(fc) => Some(Rc::clone(fc)),
            None => {
                match self.client.full_crate(crate_name, true) {
                    Ok(fc) => {
                        self.cache.insert(crate_name.to_string(), Rc::new(fc));
                        // Avoid borrow after move
                        Some(Rc::clone(self.cache.get(crate_name).unwrap()))
                    }
                    Err(e) => {
                        eprintln!("failed to retrieve crates.io information about {crate_name} due to error: {e}");
                        None
                    }
                }
            }
        }
    }

    /// Retrieves the total amount of downloads for a crate, all versions
    ///
    /// # See also
    /// [`version_downloads`](CratesIoClient::version_downloads)
    pub fn total_downloads(&mut self, crate_name: &str) -> Option<u64> {
        self.full_crate(crate_name).map(|fc| fc.total_downloads)
    }

    /// Retrieves the total amount of downloads for a specific crate version
    ///
    /// # See also
    /// [`total_downloads`](CratesIoClient::total_downloads)
    pub fn version_downloads(&mut self, name_version: &NameVersion) -> Option<u64> {
        self.full_crate(&name_version.name).and_then(|fc| {
            fc.versions.iter().find_map(|fv| {
                match Version::parse(&fv.num) {
                    Ok(current_version) => {
                        if current_version == name_version.version {
                            Some(fv.downloads)
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        eprintln!("could not parse crate.io version for {name_version:?} due to error: {e}");
                        None
                    }
                }
            })
        })
    }
}

impl Default for CratesIoClient {
    fn default() -> Self {
        let user_agent = std::env::var("USER_AGENT")
            .expect("USER_AGENT environment variable not set");
        Self::new(&user_agent, Duration::from_secs(1))
    }
}