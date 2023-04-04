//! Client used to retrieve stats such as number of lines etc. for different
//! Rust packages
use std::path::Path;

/// Retrieves code stats via `tokei` for a project
///
/// Ignored paths can be path-like, globs, etc. (see
/// [`tokei::Languages::get_statistics`]), like `".git"`.
pub(crate) fn get_code_stats(
    root_path: &Path,
    ignored_paths: &[&str],
    config: &tokei::Config,
) -> Vec<LanguageCodeStats> {
    let mut ls = tokei::Languages::new();
    ls.get_statistics(&[root_path], ignored_paths, config);
    let mut res = Vec::with_capacity(ls.len());
    for (lang_type, stats) in ls {
        res.push(LanguageCodeStats::new(lang_type.to_string(), stats));
    }
    res
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
pub struct LanguageCodeStats {
    language: String,
    stats: tokei::Language,
}

impl LanguageCodeStats {
    pub fn new(language_name: String, stats: tokei::Language) -> Self {
        Self {
            language: language_name,
            stats,
        }
    }

    /// Return a summary of the code stats for this language
    pub fn summary(&self) -> LanguageCodeStats {
        Self::new(self.language.to_owned(), self.stats.summarise())
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

impl CodeStats for LanguageCodeStats {
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

    /// Return a new language blob where all children blobs have been summarized
    pub fn summary(&self) -> LanguageBlob {
        Self::new(self.language.to_owned(), self.stats.summarise())
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
