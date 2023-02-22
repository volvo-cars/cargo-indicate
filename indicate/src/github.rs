//! Module providing connection to the GitHub API, with caching using ETags and
//! the `httpcache` feature. With this feature, `304 Not Modified` responses
//! from the GitHub will instead be fetched from a local cache.

use lazy_static::lazy_static;
use octorust::{auth::Credentials, http_cache::HttpCache, Client};

lazy_static! {
    static ref GITHUB_CLIENT: octorust::Client = {
        // It might be a good idea to cache GitHub URLs in a HashMap, that only
        // exists in memory for one set of queries. This way, the amount of even
        // slight requests is held at a minimum

        // TODO: This should probably be dynamic depending on settings and cfg
        let http_cache = <dyn HttpCache>::in_home_dir();

        // TODO: Better handling of token
        let credentials = Credentials::Token(
            std::env::var("GITHUB_API_TOKEN")
            .expect("GITHUB_API_TOKEN environment variable not set")
        );

        // TODO: Better handling of agent
        let agent = std::env::var("USER_AGENT").expect("USER_AGENT environment variable not set");

        Client::custom(
            agent,
            credentials,
            reqwest::Client::builder().build().expect("could not create GitHub reqwest client").into(),
            http_cache
        )
    };
}
