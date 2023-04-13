//! These are signals related to repositories, such as GitHub or GitLab.
pub mod github;

use url::Url;

pub(crate) enum RepoId<'a> {
    GitHub(github::GitHubRepositoryId),
    GitLab(&'a str),
    Unknown(&'a str),
}

impl<'a> From<&'a str> for RepoId<'a> {
    fn from(url: &'a str) -> Self {
        match Url::parse(url) {
            Ok(u) => match u.host_str() {
                Some(host) if host == "github.com" => {
                    // The two first parts of the path are owner and repo
                    if let Some(path) = u.path_segments() {
                        let owner_repo = path.take(2).collect::<Vec<_>>();

                        if owner_repo.len() != 2 {
                            eprintln!("owner and repo could not be resolved for repo url {url}");
                            return RepoId::Unknown(url);
                        }

                        RepoId::GitHub(github::GitHubRepositoryId::new(
                            owner_repo[0].to_string(),
                            owner_repo[1].to_string(),
                        ))
                    } else {
                        eprintln!("could not figure out owner and repo for GitHub url {url}");
                        RepoId::Unknown(url)
                    }
                }
                Some(host) if host == "gitlab.com" => RepoId::GitLab(url),
                Some(_) => RepoId::Unknown(url),
                None => {
                    eprintln!("found no host for repo url {url}");
                    RepoId::Unknown(url)
                }
            },
            Err(e) => {
                eprintln!("failed to parse repo url {url} due to error: {e}");
                RepoId::Unknown(url)
            }
        }
    }
}
