use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fs, io, thread};

use gix::ObjectId;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, ToSpan};

use crate::Result;
use crate::document::DocumentBuilder;
use crate::error::Error;
use crate::filter::TextFilter;

/// Work done after midnight belongs to the evening's session: a commit at
/// 1:30am counts toward the day before.
const DAY_ROLLOVER: SignedDuration = SignedDuration::from_hours(4);

/// Same schedule as the TUI's `App::reload`: covers the unlink/rename gap
/// of an editor's atomic save.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(10), Duration::from_millis(75)];

fn git(e: impl std::error::Error + Send + Sync + 'static) -> Error {
    Error::Git(Box::new(e))
}

/// Maps instants to the writing day they belong to, in local time.
pub struct Clock {
    tz: TimeZone,
}

impl Clock {
    pub fn system() -> Self {
        Self::new(TimeZone::system())
    }

    pub fn new(tz: TimeZone) -> Self {
        Self { tz }
    }

    pub fn today(&self) -> Date {
        self.day_of(Timestamp::now())
    }

    fn day_of(&self, ts: Timestamp) -> Date {
        self.tz.to_datetime(ts).saturating_sub(DAY_ROLLOVER).date()
    }

    fn day_of_seconds(&self, seconds: i64) -> Date {
        self.day_of(Timestamp::from_second(seconds).unwrap_or(Timestamp::MIN))
    }
}

/// Words in the charted files as of one day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// how many files matched (zero is still a real tally: "not written yet")
    pub files: usize,
    pub words: u32,
    /// words under the `--filter` heading, from the first file that has it
    /// (the TUI's first-match-wins rule)
    pub section: Option<u32>,
}

impl Tally {
    fn add(&mut self, file: FileCount) {
        self.files += 1;
        self.words += file.words;
        self.section = self.section.or(file.section);
    }
}

#[derive(Clone, Copy, Debug)]
struct FileCount {
    words: u32,
    section: Option<u32>,
}

/// Which entries of a directory a path argument selects.
#[derive(Debug)]
enum Name {
    Exact(OsString),
    Any,
    Glob(globset::GlobMatcher),
}

impl Name {
    fn matches(&self, name: &Path) -> bool {
        match self {
            Name::Exact(exact) => name.as_os_str() == exact,
            Name::Any => true,
            Name::Glob(glob) => glob.is_match(name),
        }
    }
}

/// The charted files' history: path arguments resolved against one git
/// repository, plus caches so each tree and blob is counted at most once.
pub struct History {
    repo: gix::Repository,
    workdir: PathBuf,
    /// Repo-relative directory → what to take from it. Every argument
    /// resolves to "some files directly inside one directory", the same
    /// one-level reach as run-once's directory args and globs, and the same
    /// test applies to historical trees and to today's working tree so the
    /// two can never disagree about membership.
    dirs: Vec<(PathBuf, Vec<Name>)>,
    lexer: TextFilter,
    /// `--filter`, uppercased the way `Document::get_heading` expects
    heading: Option<String>,
    blobs: HashMap<ObjectId, FileCount>,
    trees: HashMap<ObjectId, Tally>,
}

impl History {
    pub fn open(candidates: &[String], heading: Option<&str>) -> Result<Self> {
        let sources = candidates
            .iter()
            .map(|candidate| source(candidate))
            .collect::<Result<Vec<_>>>()?;
        let probe = sources
            .first()
            .map(|(dir, _)| existing_ancestor(dir))
            .ok_or(Error::NoPaths)?;

        let repo = gix::discover(probe).map_err(|e| {
            tracing::debug!("discover {}: {e}", probe.display());
            Error::NotARepository(probe.to_path_buf())
        })?;
        let workdir = repo
            .workdir()
            .ok_or_else(|| Error::NotARepository(probe.to_path_buf()))?;
        let workdir = fs::canonicalize(workdir)?;

        let mut dirs: Vec<(PathBuf, Vec<Name>)> = Vec::new();
        for (dir, name) in sources {
            let relative = dir
                .strip_prefix(&workdir)
                .map_err(|_| Error::OutsideRepository(dir.clone(), workdir.clone()))?
                .to_path_buf();
            match dirs.iter_mut().find(|(d, _)| *d == relative) {
                Some((_, names)) => names.push(name),
                None => dirs.push((relative, vec![name])),
            }
        }

        Ok(Self {
            repo,
            workdir,
            dirs,
            lexer: TextFilter::new(),
            heading: heading.map(|h| h.to_ascii_uppercase()),
            blobs: HashMap::new(),
            trees: HashMap::new(),
        })
    }

    /// One tally per day from `first` through `today`, inclusive. Each past
    /// day is the tree of the last commit made on or before it (first-parent
    /// history from HEAD, by committer date); days before the first commit
    /// have none. Today reads the working tree, so uncommitted words count.
    pub fn daily(&mut self, clock: &Clock, first: Date, today: Date) -> Result<Vec<Option<Tally>>> {
        let chain = self.first_parent_chain(clock, first)?;
        let mut tallies = Vec::new();
        for day in first.series(1.day()).take_while(|&day| day < today) {
            let tree = chain.iter().find(|(d, _)| *d <= day).map(|&(_, tree)| tree);
            let tally = match tree {
                Some(tree) => Some(self.tally_tree(tree)?),
                None => None,
            };
            tracing::debug!(%day, ?tally);
            tallies.push(tally);
        }
        tallies.push(Some(self.tally_worktree()?));
        tracing::debug!(
            commits = chain.len(),
            trees = self.trees.len(),
            blobs = self.blobs.len(),
            "history read"
        );
        Ok(tallies)
    }

    /// `(day, tree)` for each commit from HEAD back along first parents,
    /// newest first, stopping at the first commit on or before `since` —
    /// the state `since` ends in. Committer dates, because amending keeps a
    /// commit's author date but the words landed when it was committed.
    fn first_parent_chain(&self, clock: &Clock, since: Date) -> Result<Vec<(Date, ObjectId)>> {
        let head = self.repo.head().map_err(git)?;
        let mut next = head.id().map(|id| id.detach());
        let mut chain = Vec::new();
        while let Some(id) = next {
            let commit = match self.repo.find_commit(id) {
                Ok(commit) => commit,
                // A shallow clone's history just ends early.
                Err(e) if !chain.is_empty() => {
                    tracing::debug!("history ends at {id}: {e}");
                    break;
                }
                Err(e) => return Err(git(e)),
            };
            let seconds = commit.time().map(|t| t.seconds).unwrap_or_default();
            let day = clock.day_of_seconds(seconds);
            chain.push((day, commit.tree_id().map_err(git)?.detach()));
            if day <= since {
                break;
            }
            next = commit.parent_ids().next().map(|id| id.detach());
        }
        Ok(chain)
    }

    fn tally_tree(&mut self, tree: ObjectId) -> Result<Tally> {
        if let Some(tally) = self.trees.get(&tree) {
            return Ok(*tally);
        }
        let mut tally = Tally::default();
        for (_, blob) in self.tree_files(tree)? {
            tally.add(self.count_blob(blob)?);
        }
        self.trees.insert(tree, tally);
        Ok(tally)
    }

    /// Matching files in `tree`, sorted by path like run-once's file list.
    fn tree_files(&self, tree: ObjectId) -> Result<Vec<(PathBuf, ObjectId)>> {
        let root = self.repo.find_tree(tree).map_err(git)?;
        let mut files = Vec::new();
        for (dir, names) in &self.dirs {
            let subtree;
            let listing = if dir.as_os_str().is_empty() {
                &root
            } else {
                match root.lookup_entry_by_path(dir).map_err(git)? {
                    Some(entry) if entry.mode().is_tree() => {
                        subtree = self.repo.find_tree(entry.object_id()).map_err(git)?;
                        &subtree
                    }
                    _ => continue,
                }
            };
            for entry in listing.iter() {
                let entry = entry.map_err(git)?;
                // Plain files only: a symlink's blob is its target path,
                // and run-once's directory walk skips symlinks too.
                if !entry.mode().is_blob() {
                    continue;
                }
                // Only fails for names the platform can't spell as a path,
                // which the working tree couldn't hold either.
                let Ok(name) = gix::path::from_bstr(entry.filename()) else {
                    continue;
                };
                if names.iter().any(|n| n.matches(&name)) {
                    files.push((dir.join(&*name), entry.object_id()));
                }
            }
        }
        files.sort();
        files.dedup_by(|a, b| a.0 == b.0);
        Ok(files)
    }

    fn tally_worktree(&mut self) -> Result<Tally> {
        let mut files = Vec::new();
        for (dir, names) in &self.dirs {
            let Ok(entries) = fs::read_dir(self.workdir.join(dir)) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
                    continue;
                }
                let name = entry.file_name();
                if names.iter().any(|n| n.matches(Path::new(&name))) {
                    files.push(dir.join(&name));
                }
            }
        }
        files.sort();
        files.dedup();

        let mut tally = Tally::default();
        for path in files {
            if let Some(bytes) = read_retrying(&self.workdir.join(&path))? {
                tally.add(self.count(&bytes));
            }
        }
        Ok(tally)
    }

    fn count_blob(&mut self, id: ObjectId) -> Result<FileCount> {
        if let Some(count) = self.blobs.get(&id) {
            return Ok(*count);
        }
        let blob = self.repo.find_blob(id).map_err(git)?;
        let count = self.count(&blob.data);
        self.blobs.insert(id, count);
        Ok(count)
    }

    fn count(&self, bytes: &[u8]) -> FileCount {
        // Lossy rather than strict: a stray invalid byte in one old commit
        // shouldn't make the whole chart fail.
        let text = String::from_utf8_lossy(bytes);
        let mut builder = DocumentBuilder::new();
        builder.apply(self.lexer.lex(&text));
        let document = builder.finalize();
        FileCount {
            words: document.stats().subtree_paragraphs().total,
            section: self
                .heading
                .as_deref()
                .and_then(|heading| document.get_heading(heading))
                .map(|section| section.stats().subtree_paragraphs().total),
        }
    }
}

/// Resolve a path argument to (absolute canonical directory, selector).
/// Existing files and directories resolve as run-once would; anything
/// else is a glob, or a literal file that only exists in history.
fn source(candidate: &str) -> Result<(PathBuf, Name)> {
    let path = Path::new(candidate);
    if path.is_dir() {
        return Ok((fs::canonicalize(path)?, Name::Any));
    }
    let (dir, rest) = if path.is_file() {
        (fs::canonicalize(path)?, PathBuf::new())
    } else {
        let (dir, rest) = split_glob(path);
        (canonicalize_lenient(&dir)?, rest)
    };
    if !rest.as_os_str().is_empty() {
        let glob = globset::GlobBuilder::new(&rest.to_string_lossy())
            .literal_separator(true)
            .build()
            .map_err(|e| Error::BadPattern(candidate.to_string(), e))?;
        return Ok((dir, Name::Glob(glob.compile_matcher())));
    }
    match (dir.parent(), dir.file_name()) {
        (Some(parent), Some(name)) => Ok((parent.to_path_buf(), Name::Exact(name.to_owned()))),
        _ => Err(Error::FileNotFound(path.to_path_buf())),
    }
}

/// Split at the first component holding a glob metacharacter: the literal
/// directory part, and the pattern part (empty when there isn't one).
fn split_glob(path: &Path) -> (PathBuf, PathBuf) {
    let mut dir = PathBuf::new();
    let mut rest = PathBuf::new();
    for component in path.components() {
        let globby = component
            .as_os_str()
            .to_string_lossy()
            .contains(['*', '?', '[', '{']);
        if rest.as_os_str().is_empty() && !globby {
            dir.push(component);
        } else {
            rest.push(component);
        }
    }
    if dir.as_os_str().is_empty() {
        dir.push(".");
    }
    (dir, rest)
}

/// Canonicalize the longest prefix of `path` that exists and append the
/// rest, so a file deleted since (still present in history) lands at the
/// same spelling as the repository's canonical workdir.
fn canonicalize_lenient(path: &Path) -> io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    let mut existing = path.as_path();
    let mut tail = Vec::new();
    loop {
        if let Ok(canonical) = fs::canonicalize(existing) {
            return Ok(tail
                .iter()
                .rev()
                .fold(canonical, |acc, part| acc.join(part)));
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                tail.push(name.to_owned());
                existing = parent;
            }
            _ => return Ok(path),
        }
    }
}

fn existing_ancestor(path: &Path) -> &Path {
    path.ancestors().find(|p| p.is_dir()).unwrap_or(path)
}

/// `Ok(None)` for a file that vanished between listing and reading.
fn read_retrying(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let mut delays = RETRY_DELAYS.iter();
    loop {
        match fs::read(path) {
            Ok(bytes) => return Ok(Some(bytes)),
            Err(e) => match delays.next() {
                Some(delay) => thread::sleep(*delay),
                None if e.kind() == io::ErrorKind::NotFound => return Ok(None),
                None => return Err(e),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use jiff::civil::date;

    use super::*;

    struct Repo {
        dir: tempfile::TempDir,
    }

    impl Repo {
        fn new() -> Self {
            let repo = Self {
                dir: tempfile::tempdir().unwrap(),
            };
            repo.git(&["init", "-q"], "2026-01-01 12:00:00 +0000");
            repo
        }

        fn path(&self, relative: &str) -> String {
            self.dir
                .path()
                .join(relative)
                .to_string_lossy()
                .into_owned()
        }

        fn write(&self, relative: &str, text: &str) {
            let path = self.dir.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        fn remove(&self, relative: &str) {
            fs::remove_file(self.dir.path().join(relative)).unwrap();
        }

        /// Commit everything, as committed at `when` (git's date format).
        fn commit(&self, when: &str) {
            self.git(&["add", "-A"], when);
            self.git(&["commit", "-q", "--allow-empty", "-m", "words"], when);
        }

        fn git(&self, args: &[&str], when: &str) {
            // Isolated from the developer's own git config (hooks, signing).
            let status = Command::new("git")
                .args(args)
                .current_dir(self.dir.path())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "a")
                .env("GIT_AUTHOR_EMAIL", "a@example.com")
                .env("GIT_COMMITTER_NAME", "a")
                .env("GIT_COMMITTER_EMAIL", "a@example.com")
                .env("GIT_AUTHOR_DATE", when)
                .env("GIT_COMMITTER_DATE", when)
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?}");
        }

        /// Word totals for each day from `first` through `today`.
        fn words(&self, args: &[String], first: Date, today: Date) -> Vec<Option<u32>> {
            let mut history = History::open(args, None).unwrap();
            history
                .daily(&Clock::new(TimeZone::UTC), first, today)
                .unwrap()
                .into_iter()
                .map(|tally| tally.map(|t| t.words))
                .collect()
        }
    }

    #[test]
    fn days_carry_the_last_commit_forward_and_today_reads_the_worktree() {
        let repo = Repo::new();
        repo.write("src/one.md", "# One\n\none two\n");
        repo.commit("2026-03-02 12:00:00 +0000");
        repo.write("src/one.md", "# One\n\none two three four five\n");
        repo.commit("2026-03-04 12:00:00 +0000");
        // Uncommitted: today should see it anyway.
        repo.write("src/one.md", "# One\n\none two three four five six seven\n");

        let words = repo.words(&[repo.path("src")], date(2026, 3, 1), date(2026, 3, 6));

        assert_eq!(
            words,
            vec![None, Some(2), Some(2), Some(5), Some(5), Some(7)]
        );
    }

    #[test]
    fn small_hours_belong_to_the_day_before() {
        let repo = Repo::new();
        repo.write("a.md", "one\n");
        repo.commit("2026-03-02 12:00:00 +0000");
        repo.write("a.md", "one two three\n");
        repo.commit("2026-03-04 02:30:00 +0000");

        let words = repo.words(&[repo.path("a.md")], date(2026, 3, 2), date(2026, 3, 5));

        assert_eq!(words[..3], [Some(1), Some(3), Some(3)]);
    }

    #[test]
    fn glob_membership_follows_history() {
        let repo = Repo::new();
        repo.write("src/chapter.01.md", "one two\n");
        repo.write("src/notes.md", "not counted at all\n");
        repo.commit("2026-03-02 12:00:00 +0000");
        repo.write("src/chapter.02.md", "three four five\n");
        repo.commit("2026-03-03 12:00:00 +0000");
        repo.remove("src/chapter.01.md");
        repo.commit("2026-03-04 12:00:00 +0000");

        let words = repo.words(
            &[repo.path("src/chapter.*")],
            date(2026, 3, 2),
            date(2026, 3, 5),
        );

        assert_eq!(words, vec![Some(2), Some(5), Some(3), Some(3)]);
    }

    #[test]
    fn a_deleted_literal_file_still_counts_in_history() {
        let repo = Repo::new();
        repo.write("gone.md", "one two three\n");
        repo.commit("2026-03-02 12:00:00 +0000");
        repo.remove("gone.md");
        repo.commit("2026-03-03 12:00:00 +0000");

        let words = repo.words(&[repo.path("gone.md")], date(2026, 3, 2), date(2026, 3, 4));

        assert_eq!(words, vec![Some(3), Some(0), Some(0)]);
    }

    #[test]
    fn directories_reach_one_level_like_run_once() {
        let repo = Repo::new();
        repo.write("src/a.md", "one\n");
        repo.write("src/nested/b.md", "two three\n");
        repo.commit("2026-03-02 12:00:00 +0000");

        let words = repo.words(&[repo.path("src")], date(2026, 3, 2), date(2026, 3, 3));

        assert_eq!(words, vec![Some(1), Some(1)]);
    }

    #[test]
    fn overlapping_arguments_count_a_file_once() {
        let repo = Repo::new();
        repo.write("src/a.md", "one two\n");
        repo.commit("2026-03-02 12:00:00 +0000");

        let words = repo.words(
            &[
                repo.path("src"),
                repo.path("src/a.md"),
                repo.path("src/*.md"),
            ],
            date(2026, 3, 2),
            date(2026, 3, 3),
        );

        assert_eq!(words, vec![Some(2), Some(2)]);
    }

    #[test]
    fn filter_takes_the_first_file_with_the_heading() {
        let repo = Repo::new();
        repo.write("a.md", "# One\n\nfirst file words\n");
        repo.write("b.md", "# Two\n\nsecond\n\n## Sub\n\nmore words here\n");
        repo.commit("2026-03-02 12:00:00 +0000");

        let mut history = History::open(&[repo.path("*.md")], Some("two")).unwrap();
        let tallies = history
            .daily(
                &Clock::new(TimeZone::UTC),
                date(2026, 3, 2),
                date(2026, 3, 3),
            )
            .unwrap();

        let tally = tallies[0].unwrap();
        assert_eq!((tally.words, tally.section), (7, Some(4)));
    }

    #[test]
    fn an_empty_repository_has_only_today() {
        let repo = Repo::new();
        repo.write("a.md", "one two\n");

        let words = repo.words(&[repo.path("a.md")], date(2026, 3, 2), date(2026, 3, 4));

        assert_eq!(words, vec![None, None, Some(2)]);
    }

    #[test]
    fn outside_a_repository_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.md"), "one\n").unwrap();
        let path = dir.path().join("a.md").to_string_lossy().into_owned();

        let err = History::open(&[path], None).err().unwrap();

        assert!(matches!(err, Error::NotARepository(_)), "{err:?}");
    }

    #[test]
    fn arguments_spanning_repositories_are_an_error() {
        let (one, two) = (Repo::new(), Repo::new());
        one.write("a.md", "one\n");
        two.write("b.md", "two\n");

        let err = History::open(&[one.path("a.md"), two.path("b.md")], None)
            .err()
            .unwrap();

        assert!(matches!(err, Error::OutsideRepository(..)), "{err:?}");
    }

    #[test]
    fn split_glob_separates_literal_directory_from_pattern() {
        assert_eq!(
            split_glob(Path::new("src/chapter.*")),
            (PathBuf::from("src"), PathBuf::from("chapter.*"))
        );
        assert_eq!(
            split_glob(Path::new("*.md")),
            (PathBuf::from("."), PathBuf::from("*.md"))
        );
        assert_eq!(
            split_glob(Path::new("src/missing.md")),
            (PathBuf::from("src/missing.md"), PathBuf::new())
        );
    }
}
