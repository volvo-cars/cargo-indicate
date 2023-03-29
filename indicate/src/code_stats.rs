//! Client used to retrieve stats such as number of lines etc. for different
//! Rust packages
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Client providing mappings between paths and their reports
#[derive(Debug)]
pub struct CodeStatsClient {
    stats_cache: HashMap<PathBuf, Vec<Language>>,
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
    pub fn get_languages_from_path(&mut self, path: &Path) -> &Vec<Language> {
        self.stats_cache.entry(path.into()).or_insert_with(|| {
            let mut ls = tokei::Languages::new();
            ls.get_statistics(
                &[path],
                self.ignored_paths
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .as_slice(),
                &self.tokei_config,
            );
            let mut res = Vec::with_capacity(ls.len());
            for (lang_type, stats) in ls {
                res.push(Language::new(lang_type.to_string(), stats));
            }
            res
        })
    }
}
impl Default for CodeStatsClient {
    fn default() -> Self {
        CodeStatsClient::new(tokei::Config::default(), vec![])
    }
}

pub trait CodeStats {
    /// Retrieve the name of the language
    fn language(&self) -> &str;

    /// Retrieve the number of blank lines
    fn blanks(&self) -> usize;

    /// Retrieve the number of lines of code
    fn code(&self) -> usize;

    /// Retrieve the number of lines of comments
    fn comments(&self) -> usize;
}

#[derive(Debug, Clone)]
pub struct Language {
    language: String,
    stats: tokei::Language,
}

impl Language {
    pub fn new(language_name: String, stats: tokei::Language) -> Self {
        Self {
            language: language_name,
            stats,
        }
    }

    pub fn inaccurate(&self) -> bool {
        self.stats.inaccurate
    }

    pub fn children(&self) -> Vec<LanguageBlob> {
        let mut b = Vec::with_capacity(self.stats.children.len());
        for (lang_type, reports) in &self.stats.children {
            // Summarize all reports for this child
            let mut stats = tokei::CodeStats::new();
            for r in reports {
                stats += r.stats.clone();
            }
            b.push(LanguageBlob::new(lang_type.to_string(), stats));
        }
        b
    }
}

impl CodeStats for Language {
    fn language(&self) -> &str {
        &self.language
    }

    fn blanks(&self) -> usize {
        self.stats.blanks
    }

    fn code(&self) -> usize {
        self.stats.code
    }

    fn comments(&self) -> usize {
        self.stats.comments
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
    /// Retrieve the language blobs themselves inside this blob
    pub fn blobs(&self) -> Vec<LanguageBlob> {
        let mut b = Vec::with_capacity(self.stats.blobs.len());
        for (lang_type, stats) in &self.stats.blobs {
            b.push(LanguageBlob::new(lang_type.to_string(), stats.clone()));
        }
        b
    }
}

impl CodeStats for LanguageBlob {
    fn language(&self) -> &str {
        &self.language
    }

    fn blanks(&self) -> usize {
        self.stats.blanks
    }

    fn code(&self) -> usize {
        self.stats.code
    }

    fn comments(&self) -> usize {
        self.stats.comments
    }
}
