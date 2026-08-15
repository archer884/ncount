use std::{
    fs, iter,
    path::{Path, PathBuf},
};

use clap::Parser;
use either::Either;

use crate::Result;
use crate::error::Error;

#[derive(Debug, Parser)]
#[command(author, version)]
pub struct Args {
    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Debug, clap::Args)]
pub struct CommonArgs {
    /// file or directory paths
    paths: Vec<String>,

    /// filter output by heading
    #[arg(short, long)]
    filter: Option<String>,

    /// print extended information
    #[arg(short, long)]
    verbose: bool,

    /// watch files and launch the interactive TUI
    #[arg(short, long)]
    watch: bool,
}

impl Args {
    pub fn parse() -> Self {
        Parser::parse()
    }
}

impl CommonArgs {
    pub fn materialize_files(&self) -> Result<Vec<PathBuf>> {
        // Resolve each input (file, dir, or glob) to a list of actual file
        // paths, then canonicalize so the TUI's watch path matches the
        // absolute paths notify hands back. We still have to sort because
        // the default enumeration order on non-Windows file systems is
        // freaking inode order. Thanks, guys!
        let mut files = Vec::new();
        for candidate in &self.paths {
            let p = Path::new(candidate);
            let entries: Vec<PathBuf> = if p.exists() {
                iter_path_files(p).collect()
            } else {
                // Absolutize the pattern first: with globwalk's
                // `max_depth(1)`, a relative pattern containing a directory
                // component (`src/*.md`) matches nothing at all — the
                // depth budget is spent before the walk reaches the
                // matches. (Same fix as `expand_pattern`.)
                let pattern = match std::path::absolute(p) {
                    Ok(absolute) => absolute.to_string_lossy().into_owned(),
                    Err(_) => candidate.clone(),
                };
                globwalk::glob_builder(&pattern)
                    .max_depth(1)
                    .build()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .map(|e| e.into_path())
                    .collect()
            };
            if entries.is_empty() {
                return Err(Error::FileNotFound(p.to_path_buf()));
            }
            files.extend(entries);
        }
        files.sort();
        files = files
            .into_iter()
            .map(|p| fs::canonicalize(&p).map_err(|_| Error::FileNotFound(p)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }

    /// Watch-mode counterpart to `materialize_files` (which is run-once's
    /// and stays exactly as it is): the same literal-vs-glob test — a
    /// candidate that names an existing path is literal, anything else is
    /// a glob — but globs come back unexpanded as `Pattern`s so the TUI
    /// can re-expand them live as directories change. Literal candidates
    /// are still strict (must exist and yield files); a pattern may match
    /// zero files right now.
    pub fn watch_sources(&self) -> Result<Vec<WatchSource>> {
        let mut sources = Vec::new();
        for candidate in &self.paths {
            let p = Path::new(candidate);
            if p.exists() {
                let entries: Vec<PathBuf> = iter_path_files(p).collect();
                if entries.is_empty() {
                    return Err(Error::FileNotFound(p.to_path_buf()));
                }
                let entries = entries
                    .into_iter()
                    .map(|p| fs::canonicalize(&p).map_err(|_| Error::FileNotFound(p)))
                    .collect::<Result<Vec<_>, _>>()?;
                sources.push(WatchSource::Literal(entries));
            } else {
                sources.push(WatchSource::Pattern(candidate.clone()));
            }
        }
        Ok(sources)
    }

    pub fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    pub fn verbose(&self) -> bool {
        self.verbose
    }

    pub fn watch(&self) -> bool {
        self.watch
    }
}

/// A command-line candidate as watch mode understands it: literal paths
/// (an existing file/dir arg — resolved once, and required to produce
/// files) or a live glob pattern (anything else — re-expanded as
/// directories change, and allowed to match zero files right now).
#[derive(Debug)]
pub enum WatchSource {
    Literal(Vec<PathBuf>),
    Pattern(String),
}

/// Expand a glob pattern to sorted, canonicalized file paths, tolerating
/// zero matches. Mirrors the globwalk invocation inside
/// `materialize_files` (run-once), which keeps its own copy because
/// run-once behavior must not change: it errors on zero matches instead.
pub fn expand_pattern(pattern: &str) -> Vec<PathBuf> {
    // Absolutize the pattern first: with globwalk's `max_depth(1)`, a
    // *relative* pattern containing a directory component (`src/*.md`)
    // matches nothing at all — the depth budget is spent before the walk
    // reaches the matches. Bare patterns (`*.md`) and absolute patterns
    // are unaffected. (`materialize_files` had the same bug until it was
    // ruled a regression and fixed the same way, 2026-07-24.)
    let pattern = match std::path::absolute(Path::new(pattern)) {
        Ok(absolute) => absolute.to_string_lossy().into_owned(),
        Err(_) => pattern.to_string(),
    };
    let mut files: Vec<PathBuf> = globwalk::glob_builder(&pattern)
        .max_depth(1)
        .build()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter_map(|p| fs::canonicalize(&p).ok())
        .collect();
    files.sort();
    files.dedup();
    files
}

/// The literal directory prefix of a glob pattern — every component before
/// the first one containing a metacharacter. That's the directory to watch
/// for changes in what the pattern matches (needed even when the pattern
/// currently matches nothing, in which case no matched file's parent
/// covers it). `.` for a pattern with no literal prefix (e.g. `*.md`).
pub fn pattern_base_dir(pattern: &str) -> PathBuf {
    let mut prefix = PathBuf::new();
    for component in Path::new(pattern).components() {
        match component {
            std::path::Component::Normal(part) => {
                let part = part.to_string_lossy();
                if part.contains(['*', '?', '[']) {
                    break;
                }
                prefix.push(part.as_ref());
            }
            other => prefix.push(other.as_os_str()),
        }
    }
    if prefix.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        prefix
    }
}

fn iter_path_files(path: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    let path = path.as_ref();
    if path.is_file() {
        Either::Left(iter::once(path.into()))
    } else {
        let paths = fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                entry
                    .file_type()
                    .ok()
                    .map(|kind| (kind, entry))
                    .filter(|x| x.0.is_file())
                    .map(|(_, entry)| entry.path())
            });
        Either::Right(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `set_current_dir` is process-wide; serialize tests that touch it.
    static CWD_LOCK: Mutex<()> = Mutex::new(());

    fn args(paths: &[&str]) -> CommonArgs {
        CommonArgs {
            paths: paths.iter().map(|s| s.to_string()).collect(),
            filter: None,
            verbose: false,
            watch: false,
        }
    }

    struct CurrentDirGuard {
        original: PathBuf,
    }

    impl CurrentDirGuard {
        fn enter(dir: &Path) -> Self {
            let original = std::env::current_dir().unwrap();
            std::env::set_current_dir(dir).unwrap();
            Self { original }
        }
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    #[test]
    fn materialize_canonicalizes_relative_paths() {
        // The whole point: a relative path the user typed on the command line
        // must round-trip through to a canonical (absolute) form, because
        // that's what notify hands back as the event path and what the TUI's
        // `Watch::changed` then string-compares against.
        let _lock = CWD_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("foo.md");
        std::fs::write(&file, "# x\n\nhello world").unwrap();

        let _guard = CurrentDirGuard::enter(temp.path());
        let files = args(&["foo.md"]).materialize_files().unwrap();

        assert_eq!(files.len(), 1);
        assert!(files[0].is_absolute(), "got relative: {:?}", files[0]);
        assert_eq!(files[0], file.canonicalize().unwrap());
    }

    #[test]
    fn materialize_reports_missing_file_with_path() {
        let err = args(&["definitely/does/not/exist.md"])
            .materialize_files()
            .unwrap_err();
        match err {
            Error::FileNotFound(p) => {
                assert_eq!(p, PathBuf::from("definitely/does/not/exist.md"));
            }
            other => panic!("expected FileNotFound, got {other:?}"),
        }
    }

    #[test]
    fn watch_sources_split_literals_from_patterns() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("foo.md");
        std::fs::write(&file, "# x\n\nhello world").unwrap();
        let pattern = temp.path().join("*.md").to_string_lossy().into_owned();

        let sources = args(&[file.to_str().unwrap(), &pattern])
            .watch_sources()
            .unwrap();

        assert_eq!(sources.len(), 2);
        assert!(
            matches!(&sources[0], WatchSource::Literal(paths) if paths == &vec![file.canonicalize().unwrap()])
        );
        assert!(matches!(&sources[1], WatchSource::Pattern(p) if p == &pattern));
    }

    #[test]
    fn watch_sources_tolerates_zero_match_patterns() {
        let temp = tempfile::tempdir().unwrap();
        let pattern = temp.path().join("*.md").to_string_lossy().into_owned();

        let sources = args(&[&pattern]).watch_sources().unwrap();

        assert!(matches!(&sources[0], WatchSource::Pattern(_)));
        // ...and expanding it right now simply yields nothing.
        assert!(expand_pattern(&pattern).is_empty());
    }

    #[test]
    fn watch_sources_still_errors_on_empty_directory_arg() {
        // Literal candidates stay strict in watch mode: a directory that
        // exists but contains no files is an error, same as run-once.
        let temp = tempfile::tempdir().unwrap();
        let empty = temp.path().join("empty");
        std::fs::create_dir(&empty).unwrap();

        let err = args(&[empty.to_str().unwrap()])
            .watch_sources()
            .unwrap_err();

        assert!(matches!(err, Error::FileNotFound(p) if p == empty));
    }

    #[test]
    fn expand_pattern_matches_sorts_and_canonicalizes() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("b.md"), "x").unwrap();
        std::fs::write(temp.path().join("a.md"), "x").unwrap();
        std::fs::write(temp.path().join("not-md.txt"), "x").unwrap();

        let pattern = temp.path().join("*.md").to_string_lossy().into_owned();
        let files = expand_pattern(&pattern);

        assert_eq!(
            files,
            vec![
                temp.path().join("a.md").canonicalize().unwrap(),
                temp.path().join("b.md").canonicalize().unwrap(),
            ]
        );
    }

    #[test]
    fn materialize_handles_relative_globs_with_directory_components() {
        // Regression: `ncount 'src/chapter.*'` (quoted) used to error
        // `file not found` — the globwalk call never absolutized the
        // pattern, so with `max_depth(1)` the depth budget ran out above
        // the matches.
        let _lock = CWD_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        let file = temp.path().join("src").join("chapter.01.md");
        std::fs::write(&file, "# x\n\nhello world").unwrap();

        let _guard = CurrentDirGuard::enter(temp.path());
        let files = args(&["src/chapter.*"]).materialize_files().unwrap();

        assert_eq!(files, vec![file.canonicalize().unwrap()]);
    }

    #[test]
    fn materialize_still_errors_on_zero_match_glob() {
        // The fix for relative globs must not change zero-match handling:
        // a pattern that matches nothing is still an error in run-once.
        let _lock = CWD_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();

        let _guard = CurrentDirGuard::enter(temp.path());
        let err = args(&["src/nothing.*"]).materialize_files().unwrap_err();

        assert!(matches!(err, Error::FileNotFound(p) if p == Path::new("src/nothing.*")));
    }

    #[test]
    fn expand_pattern_handles_relative_patterns_with_directory_components() {
        // The case that bit in real usage (`ncount -w "src/chapter.*"`):
        // a relative pattern with a directory component matched nothing
        // until the pattern was absolutized internally.
        let _lock = CWD_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        let file = temp.path().join("src").join("chapter.01.md");
        std::fs::write(&file, "x").unwrap();

        let _guard = CurrentDirGuard::enter(temp.path());
        let files = expand_pattern("src/chapter.*");

        assert_eq!(files, vec![file.canonicalize().unwrap()]);
    }

    #[test]
    fn pattern_base_dir_takes_the_literal_prefix() {
        assert_eq!(pattern_base_dir("src/chapter.*"), PathBuf::from("src"));
        assert_eq!(pattern_base_dir("*.md"), PathBuf::from("."));
        assert_eq!(
            pattern_base_dir("/books/src/*.md"),
            PathBuf::from("/books/src")
        );
        assert_eq!(pattern_base_dir("a/b?c/d.md"), PathBuf::from("a"));
    }
}
