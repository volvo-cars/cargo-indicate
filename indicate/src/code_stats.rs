//! Client used to retrieve stats such as number of lines etc. for different
//! Rust packages
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};
use tokei::Languages;

/// Client providing mappings between paths and their reports
#[derive(Debug)]
pub struct CodeStatsClient {
    stats_cache: HashMap<PathBuf, tokei::Languages>,
    tokei_config: tokei::Config,
    /// Ignored relative paths. Is passed to
    /// [`tokei::Languages::get_statistics`]
    ignored_paths: Vec<String>,
}
impl CodeStatsClient {
    /// Creates a new client using the configuration provided
    ///
    /// Often [`CodeStatsClient::default()`] can be used instead.
    pub fn new(
        tokei_config: tokei::Config,
        ignored_paths: Vec<String>,
    ) -> Self {
        Self {
            stats_cache: HashMap::new(),
            tokei_config,
            ignored_paths,
        }
    }

    /// Retrieves language information from a path, using a cached version
    /// if available
    pub fn get_languages_from_path(&mut self, path: &Path) -> &Languages {
        self.stats_cache.entry(path.into()).or_insert_with(|| {
            let mut ls = Languages::new();
            ls.get_statistics(
                &[path],
                self.ignored_paths
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .as_slice(),
                &self.tokei_config,
            );
            ls
        })
    }
}
impl Default for CodeStatsClient {
    fn default() -> Self {
        CodeStatsClient::new(tokei::Config::default(), vec![])
    }
}

#[derive(Debug, Clone)]
pub struct LanguageBlob {
    language: String,
    stats: tokei::CodeStats,
}
impl LanguageBlob {
    pub fn new(language: String, stats: tokei::CodeStats) -> Self {
        Self { language, stats }
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn blanks(&self) -> usize {
        self.stats.blanks
    }

    pub fn code(&self) -> usize {
        self.stats.code
    }

    pub fn comments(&self) -> usize {
        self.stats.comments
    }

    /// Retrieve the language blobs themselves inside this blob
    pub fn blobs(&self) -> Vec<LanguageBlob> {
        let mut b = Vec::with_capacity(self.stats.blobs.len());
        for (lang_type, stats) in &self.stats.blobs {
            b.push(LanguageBlob::new(lang_type.to_string(), stats.clone()));
        }
        b
    }
}
