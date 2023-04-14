use std::{collections::HashMap, time::Duration};

use crates_io_api::{SyncClient, FullCrate};

/// Wrapper around a [`crates_io_api::SyncClient`], with added caching
pub struct CratesIoClient {
    client: SyncClient,

    /// Cache between crate name and information about it
    cache: HashMap<String, FullCrate>
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
}

impl Default for CratesIoClient {
    fn default() -> Self {
        // See https://crates.io/policies#crawlers
        let user_agent = std::env::var("USER_AGENT")
            .expect("USER_AGENT environment variable not set");
        Self::new(&user_agent, Duration::from_secs(1))
    }
}