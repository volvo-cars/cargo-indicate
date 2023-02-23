//! Module providing connection to the GitHub API, with caching using ETags and
//! the `httpcache` feature. With this feature, `304 Not Modified` responses
//! from the GitHub will instead be fetched from a local cache.

use std::{collections::HashMap, sync::Arc};

use octorust::{
    auth::Credentials,
    http_cache::HttpCache,
    types::{FullRepository, PublicUser},
    Client,
};
use once_cell::sync::Lazy;

use crate::RUNTIME;

/// A unique identifier of a GitHub repository consisting of the owner and the
/// repository, i.e. on the form github.com/<owner>/<repository>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct GitHubRepositoryId {
    owner: String,
    repo: String,
}

impl GitHubRepositoryId {
    pub fn new(owner: String, repo: String) -> Self {
        Self { owner, repo }
    }
}

impl From<(String, String)> for GitHubRepositoryId {
    fn from(value: (String, String)) -> Self {
        Self {
            owner: value.0,
            repo: value.1,
        }
    }
}

static GITHUB_CLIENT: Lazy<octorust::Client> = Lazy::new(|| {
    // It might be a good idea to cache GitHub URLs in a HashMap, that only
    // exists in memory for one set of queries. This way, the amount of even
    // slight requests is held at a minimum

    // TODO: This should probably be dynamic depending on settings and cfg
    let http_cache = <dyn HttpCache>::in_home_dir();

    // TODO: Better handling of agent
    let agent = std::env::var("USER_AGENT")
        .expect("USER_AGENT environment variable not set");

    // TODO: Better handling of token
    let credentials = Credentials::Token(
        std::env::var("GITHUB_API_TOKEN")
            .expect("GITHUB_API_TOKEN environment variable not set"),
    );

    Client::custom(
        agent,
        credentials,
        reqwest::Client::builder()
            .build()
            .expect("could not create GitHub reqwest client")
            .into(),
        http_cache,
    )
});

static GITHUB_REPOS_CLIENT: Lazy<octorust::repos::Repos> =
    Lazy::new(|| octorust::repos::Repos::new(GITHUB_CLIENT.clone()));

static GITHUB_USERS_CLIENT: Lazy<octorust::users::Users> =
    Lazy::new(|| octorust::users::Users::new(GITHUB_CLIENT.clone()));

/// Wrapper for interacting with the GitHub API. Caches previous requests, and
/// will not remake queries it has already made. Uses the global static clients
/// of its module.
pub(crate) struct GitHubClient {
    repo_cache: HashMap<GitHubRepositoryId, Arc<FullRepository>>,
    user_cache: HashMap<Arc<str>, Arc<PublicUser>>,
}

impl GitHubClient {
    pub fn new() -> Self {
        Self {
            repo_cache: HashMap::new(),
            user_cache: HashMap::new(),
        }
    }

    pub fn get_repository(
        &mut self,
        id: &GitHubRepositoryId,
    ) -> Option<Arc<FullRepository>> {
        match self.repo_cache.get(&id) {
            Some(r) => Some(Arc::clone(r)),
            None => {
                let future = GITHUB_REPOS_CLIENT.get(&id.owner, &id.repo);

                // We just block until this resolves for now
                match RUNTIME.block_on(future) {
                    Ok(r) => {
                        // Insert into the cache
                        let arcr = Arc::new(r);
                        self.repo_cache.insert(id.clone(), Arc::clone(&arcr));
                        Some(arcr)
                    }
                    Err(e) => {
                        eprintln!("Failed to resolve GitHub repository {}/{} due to error: {e}", id.owner, id.repo);
                        None
                    }
                }
            }
        }
    }

    pub fn get_public_user(
        &mut self,
        username: &str,
    ) -> Option<Arc<PublicUser>> {
        match self.user_cache.get(username) {
            Some(r) => Some(Arc::clone(r)),
            None => {
                let future = GITHUB_USERS_CLIENT.get_by_username(username);

                // We just block until this resolves for now
                match RUNTIME.block_on(future) {
                    Ok(u) => {
                        // Insert into the cache
                        let u = u.public_user().expect(
                            "could not convert user response to public user",
                        ).to_owned();

                        let arc_pubu = Arc::new(u);
                        self.user_cache.insert(
                            username.clone().into(),
                            Arc::clone(&arc_pubu),
                        );
                        Some(arc_pubu)
                    }
                    Err(e) => {
                        eprintln!("Failed to resolve GitHub user {} due to error: {e}", username);
                        None
                    }
                }
            }
        }
    }
}
