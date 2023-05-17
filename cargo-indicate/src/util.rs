use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
};

/// Ensures the parent directories exists, and if they don't, attempt to create
/// them
pub(crate) fn ensure_parents_exist(path: &Path) -> Result<(), std::io::Error> {
    if let Some(leading_dirs) = path.parent() {
        return fs::create_dir_all(leading_dirs);
    }
    Ok(())
}

/// Creates paths for output files, named according to the input queries
///
/// To avoid overwriting when we have duplicate query name prefixes, a number is
/// appended to the prefix if a duplicate is found.
pub(crate) fn create_output_paths(
    query_paths: &[&Path],
    output_dir: &Path,
) -> Vec<PathBuf> {
    let mut used_file_prefix: BTreeSet<OsString> = BTreeSet::new();
    let mut res = Vec::with_capacity(query_paths.len());

    for p in query_paths {
        let mut pb = PathBuf::from(output_dir);

        // TODO: Replace `util::file_prefix` with `Path::file_prefix` once
        // stabilized
        let Some (true_file_prefix) = file_prefix(p) else {
            panic!(
                "could not extract file prefix from {}",
                p.to_string_lossy()
            );
        };

        let file_prefix = if used_file_prefix.contains(true_file_prefix) {
            let mut i: u32 = 1;
            let mut file_prefix = OsString::from(true_file_prefix);
            while used_file_prefix.contains(file_prefix.as_os_str()) {
                // This is to avoid file_prefix-1-2-3-4-....
                file_prefix = OsString::from(true_file_prefix);
                file_prefix.push(i.to_string());
                i += 1;
            }
            used_file_prefix.insert(file_prefix.clone());
            file_prefix
        } else {
            used_file_prefix.insert(true_file_prefix.to_os_string());
            OsString::from(true_file_prefix)
        };

        pb.push(file_prefix);
        pb.set_extension("out.json"); // first  `.` inserted automatically

        res.push(pb);
    }

    res
}

/// Extracts the prefix of a filename; stand-in for [`Path::file_prefix`] with
/// a naive implementation
///
/// According to the nightly definition, a prefix is:
///
/// * [`None`], if there is no file name;
/// * The entire file name if there is no embedded `.`;
/// * The portion of the file name before the first non-beginning `.`;
/// * The entire file name if the file name begins with `.` and has no other `.`s within;
/// * The portion of the file name before the second `.` if the file name begins with `.`
///
/// _TODO_: Remove once `path_file_prefix` is stabilized (see
/// [#86319](https://github.com/rust-lang/rust/issues/86319))
#[must_use]
pub(crate) fn file_prefix(path: &Path) -> Option<&OsStr> {
    path.file_name().and_then(|filename| {
        // Handle special cases
        if filename.is_empty() || filename == ".." || filename == "." {
            return None;
        }

        // While this is not optimal, it saves us a headache
        let s = match filename.to_str() {
            Some(s) => s,
            None => {
                eprintln!(
                    "filename {} could not be parsed",
                    filename.to_string_lossy()
                );
                return None;
            }
        };

        // Remove leading `.` if present
        let trimmed = match s.strip_prefix('.') {
            Some(t) => t,
            None => s,
        };

        // Split the file name to at most 2 parts, separated by '.'
        let mut iter = trimmed.splitn(2, |c| c == '.');
        let before = iter.next();
        let after = iter.next();

        match (before, after) {
            // The entire file name if there is no embedded `.`
            // The entire file name if the file name begins with `.` and has no other `.`s within
            (Some(_), None) => Some(filename),
            // The portion of the file name before the first non-beginning `.`
            // The portion of the file name before the second `.` if the file name begins with `.`
            (Some(b), Some(_)) => Some(OsStr::new(b)),
            _ => {
                eprintln!(
                    "could not figure out how to parse filename {}",
                    filename.to_string_lossy()
                );
                None
            }
        }
    })
}

#[cfg(test)]
mod test {
    use std::{ffi::OsStr, path::Path};

    use crate::util::file_prefix;
    use test_case::test_case;

    #[test_case("", None ; "empty filename")]
    #[test_case("some_name", Some(OsStr::new("some_name")) ; "no period")]
    #[test_case(".some_name", Some(OsStr::new(".some_name")) ; "only leading period")]
    #[test_case("some_name.jpg", Some(OsStr::new("some_name")) ; "suffix")]
    #[test_case(".some_name.jpg", Some(OsStr::new("some_name")) ; "only leading period and suffix")]
    #[test_case("some_name.tar.xz", Some(OsStr::new("some_name")) ; "tarball suffix")]
    #[test_case("somedir/some_name", Some(OsStr::new("some_name")) ; "dir no period")]
    #[test_case("somedir/.some_name", Some(OsStr::new(".some_name")) ; "dir only leading period")]
    #[test_case("somedir/some_name.jpg", Some(OsStr::new("some_name")) ; "dir suffix")]
    #[test_case("somedir/.some_name.jpg", Some(OsStr::new("some_name")) ; "dir only leading period and suffix")]
    #[test_case("somedir/some_name.tar.xz", Some(OsStr::new("some_name")) ; "dir tarball suffix")]
    fn test_file_prefix(path_str: &str, expected: Option<&OsStr>) {
        assert_eq!(file_prefix(Path::new(path_str)), expected);
    }
}
