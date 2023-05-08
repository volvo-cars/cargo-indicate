//! Module providing connection to the GitHub API, with caching using `ETags`
//! and the `httpcache` feature. With this feature, `304 Not Modified`
//! responses from the GitHub will instead be fetched from a local cache.

use std::{collections::HashMap, sync::Arc, time::Duration};

#[cfg(test)]
use global_counter::primitive::exact::CounterUsize;
use octorust::{
    auth::Credentials,
    http_cache::HttpCache,
    types::{FullRepository, PublicUser},
    Client,
};
use once_cell::sync::Lazy;

use crate::RUNTIME;

#[cfg(test)]
pub(crate) static GH_API_CALL_COUNTER: CounterUsize = CounterUsize::new(0);

/// A unique identifier of a GitHub repository consisting of the owner and the
/// repository, i.e. on the form github.com/<owner>/<repository>
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitHubRepositoryId {
    owner: String,
    repo: String,
}

impl GitHubRepositoryId {
    #[must_use]
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

/// Static global client used to connect to GitHub
///
/// Will use an HTTP cache to only retrieve full API responses if the data has
/// changed, otherwise it will use data cached locally on the machine.
static GITHUB_CLIENT: Lazy<octorust::Client> = Lazy::new(|| {
    // TODO: This should probably be dynamic depending on settings and cfg,
    // but this is currently not supported by octorust
    let http_cache = <dyn HttpCache>::in_home_dir();

    // TODO: Better handling of agent
    let user_agent = std::env::var("USER_AGENT")
        .expect("USER_AGENT environment variable not set");

    // TODO: Better handling of token
    let credentials = Credentials::Token(
        std::env::var("GITHUB_API_TOKEN")
            .expect("GITHUB_API_TOKEN environment variable not set"),
    );

    Client::custom(
        user_agent,
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

static GITHUB_RATE_LIMIT_CLIENT: Lazy<octorust::rate_limit::RateLimit> =
    Lazy::new(|| octorust::rate_limit::RateLimit::new(GITHUB_CLIENT.clone()));

/// Wrapper for interacting with the GitHub API. Caches previous requests, and
/// will not remake queries it has already made. Uses the global static clients
/// of its module.
#[derive(Debug, Clone)]
pub struct GitHubClient {
    repo_cache: HashMap<GitHubRepositoryId, Arc<FullRepository>>,
    user_cache: HashMap<Arc<str>, Arc<PublicUser>>,

    /// If the client is to await a new quota if the current one is emptied
    ///
    /// This may take a _very_ long time.
    await_quota: bool,
}

enum AwaitQuotaResult {
    QuotaAwaited { success: bool },
    QuotaNotReached,
    CouldNotCheck,
}

impl GitHubClient {
    /// Creates a new GitHub client, using the `GITHUB_TOKEN` for authentication
    ///
    /// If this client is to await quota, it will sleep once it reaches its
    /// quota until it is replaced. This may take a _really_ long time.
    #[must_use]
    pub fn new(await_quota: bool) -> Self {
        Self {
            repo_cache: HashMap::new(),
            user_cache: HashMap::new(),
            await_quota,
        }
    }

    /// Awaits new quota for GitHub if needed
    ///
    /// This will perform a `GET` request, and should be held at a low (even if
    /// this request itself does not affect the quota).
    ///
    /// # Panics
    ///
    /// Panics if `Self` is set to not await quota.
    fn await_new_quota(&self) -> AwaitQuotaResult {
        if self.await_quota {
            let future = GITHUB_RATE_LIMIT_CLIENT.get();
            match RUNTIME.block_on(future) {
                Ok(r) => {
                    // See https://docs.github.com/en/rest/rate-limit?apiVersion=2022-11-28#get-rate-limit-status-for-the-authenticated-user
                    let rate = r.resources.core;
                    if rate.remaining == 0 {
                        let current_time = chrono::Utc::now();
                        let time_until_new_quota = Duration::from_millis(
                            (rate.reset - current_time.timestamp_millis())
                                as u64,
                        );
                        println!("No GitHub rate remaining, will wait for {} seconds ({} minutes)", time_until_new_quota.as_secs(), time_until_new_quota.as_secs() / 60);
                        std::thread::sleep(time_until_new_quota);

                        // Make sure we have more quota now!
                        return match self.await_new_quota() {
                            AwaitQuotaResult::QuotaNotReached => {
                                // This await worked, because now it says we
                                // have not reached the quota
                                AwaitQuotaResult::QuotaAwaited { success: true }
                            }
                            AwaitQuotaResult::QuotaAwaited { success } => {
                                AwaitQuotaResult::QuotaAwaited { success }
                            }
                            AwaitQuotaResult::CouldNotCheck => {
                                AwaitQuotaResult::CouldNotCheck
                            }
                        };
                    }
                    AwaitQuotaResult::QuotaNotReached
                }
                Err(e) => {
                    eprintln!(
                        "Failed to check GitHub rate limit due to error {e}"
                    );
                    AwaitQuotaResult::CouldNotCheck
                }
            }
        } else {
            panic!("client tried awaiating a new GitHub quota, but was marked to not do so");
        }
    }

    /// Retrieves a GitHub repository from a [`GitHubRepositoryId`]
    ///
    /// Will first try to see if this instance has retrieved this repository
    /// before, if so it will return a cached value. If not, it will try to use
    /// an HTTP cache to only retrieve the data if it has changed.
    pub fn get_repository(
        &mut self,
        id: &GitHubRepositoryId,
    ) -> Option<Arc<FullRepository>> {
        if let Some(r) = self.repo_cache.get(id) {
            Some(Arc::clone(r))
        } else {
            let future = GITHUB_REPOS_CLIENT.get(&id.owner, &id.repo);

            // println!("Get {:?}", id);

            #[cfg(test)]
            {
                GH_API_CALL_COUNTER.inc();
            }

            // We just block until this resolves for now
            match RUNTIME.block_on(future) {
                Ok(r) => {
                    // Insert into the cache
                    let arcr = Arc::new(r);
                    self.repo_cache.insert(id.clone(), Arc::clone(&arcr));
                    Some(arcr)
                }
                Err(e) => {
                    if self.await_quota {
                        // It is possible that we have reached a rate limit
                        match self.await_new_quota() {
                            AwaitQuotaResult::QuotaAwaited {
                                success: true,
                            } => {
                                // The quota was reached by this request, try again!
                                return self.get_repository(id);
                            }
                            AwaitQuotaResult::QuotaAwaited {
                                success: false,
                            } => {
                                eprintln!("GitHub quota reached, but new could not be awaited");
                            }
                            _ => {}
                        }
                    }
                    eprintln!("Failed to resolve GitHub repository {}/{} due to error: {e}", id.owner, id.repo);
                    None
                }
            }
        }
    }

    /// Retrieves a GitHub repository from a GitHub username
    ///
    /// Will first try to see if this instance has retrieved this user
    /// before, if so it will return a cached value. If not, it will try to use
    /// an HTTP cache to only retrieve the data if it has changed.
    pub fn get_public_user(
        &mut self,
        username: &str,
    ) -> Option<Arc<PublicUser>> {
        if let Some(r) = self.user_cache.get(username) {
            Some(Arc::clone(r))
        } else {
            let future = GITHUB_USERS_CLIENT.get_by_username(username);

            #[cfg(test)]
            {
                GH_API_CALL_COUNTER.inc();
            }

            // We just block until this resolves for now
            match RUNTIME.block_on(future) {
                Ok(u) => {
                    // Insert into the cache
                    let u = u
                        .public_user()
                        .expect(
                            "could not convert user response to public user",
                        )
                        .clone();

                    let arc_pubu = Arc::new(u);
                    self.user_cache
                        .insert(username.into(), Arc::clone(&arc_pubu));
                    Some(arc_pubu)
                }
                Err(e) => {
                    if self.await_quota {
                        // It is possible that we have reached a rate limit
                        match self.await_new_quota() {
                            AwaitQuotaResult::QuotaAwaited {
                                success: true,
                            } => {
                                // The quota was reached by this request, try again!
                                return self.get_public_user(username);
                            }
                            AwaitQuotaResult::QuotaAwaited {
                                success: false,
                            } => {
                                eprintln!("GitHub quota reached, but new could not be awaited");
                            }
                            _ => {}
                        }
                    }
                    eprintln!("Failed to resolve GitHub user {username} due to error: {e}");
                    None
                }
            }
        }
    }
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new(false)
    }
}
