//! Client used to retrieve stats such as number of lines etc. for different
//! Rust packages
use std::path::Path;

/// Retrieves code stats via `tokei` for a project
///
/// Ignored paths can be path-like, globs, etc. (see
/// [`tokei::Languages::get_statistics`]), like `".git"`.
///
/// If `included_paths` is provided, all except these paths are excluded, but
/// `ignored_paths` is still applied.
pub(crate) fn get_code_stats(
    root_path: &Path,
    ignored_paths: &[&str],
    included_paths: Option<Vec<impl AsRef<str>>>,
    config: &tokei::Config,
) -> Vec<LanguageCodeStats> {
    let mut ls = tokei::Languages::new();

    if let Some(included_paths) = included_paths {
        let target_paths = included_paths
            .iter()
            .map(|s| {
                let mut p = root_path.to_path_buf();
                p.push(s.as_ref());
                p
            })
            .collect::<Vec<_>>();
        ls.get_statistics(target_paths.as_slice(), ignored_paths, config)
    } else {
        ls.get_statistics(&[root_path], ignored_paths, config);
    }

    let mut res = Vec::with_capacity(ls.len());
    for (lang_type, stats) in ls {
        res.push(LanguageCodeStats::new(lang_type.to_string(), stats));
    }
    res
}

pub trait CodeStats {
    /// Retrieve the name of the language
    fn language(&self) -> &str;

    /// Retrieve the number of files
    fn files(&self) -> usize;

    /// Retrieve the total number of lines
    fn lines(&self) -> usize;

    /// Retrieve the number of blank lines
    fn blanks(&self) -> usize;

    /// Retrieve the number of lines of code
    fn code(&self) -> usize;

    /// Retrieve the number of lines of comments
    fn comments(&self) -> usize;

    /// Calculates `#lines of comments / #lines of code`
    fn comments_to_code(&self) -> f64 {
        self.comments() as f64 / self.code() as f64
    }

    /// Summarizes the code stats
    fn summary(&self) -> Self;
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
            b.push(LanguageBlob::new(
                lang_type.to_string(),
                reports.len(),
                stats,
            ));
        }
        b
    }
}

impl CodeStats for LanguageCodeStats {
    fn language(&self) -> &str {
        &self.language
    }

    fn files(&self) -> usize {
        self.stats.reports.len()
    }

    fn lines(&self) -> usize {
        self.stats.lines()
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

    fn summary(&self) -> LanguageCodeStats {
        Self::new(self.language.to_owned(), self.stats.summarise())
    }
}

#[derive(Debug, Clone)]
pub struct LanguageBlob {
    language: String,
    files: usize,
    stats: tokei::CodeStats,
}

impl LanguageBlob {
    pub fn new(
        language: String,
        files: usize,
        stats: tokei::CodeStats,
    ) -> Self {
        Self {
            language,
            files,
            stats,
        }
    }

    /// Retrieve the language blobs themselves inside this blob
    pub fn blobs(&self) -> Vec<LanguageBlob> {
        let mut b = Vec::with_capacity(self.stats.blobs.len());
        for (lang_type, stats) in &self.stats.blobs {
            b.push(LanguageBlob::new(
                lang_type.to_string(),
                self.files,
                stats.clone(),
            ));
        }
        b
    }
}

impl CodeStats for LanguageBlob {
    fn language(&self) -> &str {
        &self.language
    }

    fn files(&self) -> usize {
        self.files
    }

    fn lines(&self) -> usize {
        self.stats.lines()
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

    /// Return a new language blob where all children blobs have been summarized
    fn summary(&self) -> LanguageBlob {
        Self::new(self.language.to_owned(), self.files, self.stats.summarise())
    }
}
