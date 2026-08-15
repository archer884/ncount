use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ratatui::widgets::TableState;

use crate::Result;
use crate::cli::{CommonArgs, WatchSource, expand_pattern, pattern_base_dir};
use crate::document::{Document, DocumentBuilder, Paragraphs};
use crate::filter::TextFilter;

pub struct LoadedFile {
    pub path: PathBuf,
    /// `None` while the file can't be read (deleted, or momentarily absent
    /// mid-save): the file drops out of the table until a reload succeeds.
    /// See `App::reload`.
    pub document: Option<Document>,
    /// True when this file entered the set via a live glob pattern. Such
    /// files are dropped entirely when a re-expansion stops matching them
    /// (`App::sync_patterns`); literal-arg files stay forever and merely
    /// hide when unreadable.
    pub from_pattern: bool,
}

pub enum Mode {
    Normal,
    Filter {
        buffer: String,
        previous: Option<String>,
    },
    /// Shortcuts dialog is open. Any key press returns to `Normal`.
    Help,
}

/// A single document section, retaining enough tree metadata for folding.
#[derive(Clone)]
pub struct RowData {
    pub path: PathBuf,
    pub heading: String,
    pub level: i32,
    pub paragraphs: Paragraphs,
    pub parent: Option<(PathBuf, String)>,
    /// `Paragraphs` aggregated over this section plus every descendant. Used
    /// for the detail columns when a parent is collapsed (its children are
    /// hidden, so the row speaks for the whole subtree); a leaf's value
    /// equals `paragraphs`.
    pub subtree_paragraphs: Paragraphs,
    pub has_children: bool,
    /// True when this pinned row is visible only because an ancestor is folded.
    pub pinned_exception: bool,
}

pub struct App {
    pub files: Vec<LoadedFile>,
    pub filter: Option<String>,
    /// Sections currently shown unfolded. Keys are (file path, heading), not
    /// row indices, so membership changes cannot scramble fold state.
    pub expanded: HashSet<(PathBuf, String)>,
    /// Pins are keyed by (file path, heading), not row index, so a
    /// membership change that reshuffles the table (see `sync_patterns`)
    /// can't scramble pinned rows.
    pub pinned: HashSet<(PathBuf, String)>,
    pub mode: Mode,
    pub table_state: TableState,
    /// A row whose visible index must be restored after a projection
    /// change: either a pinned exception reopened by expanding its parent,
    /// or the selected row at the moment its file hid — the selection
    /// returns to it when the reload succeeds (see `rows`).
    deferred_selection: Option<(PathBuf, String)>,
    /// Row keys of the previous `rows()` projection, in render order, so
    /// `rows()` can tell which row the selection was on before the table
    /// changed shape — an index alone is ambiguous once rows shift.
    last_rows_keys: Vec<(PathBuf, String)>,
    pub status: Option<String>,
    pub should_quit: bool,
    /// Live glob patterns from the command line (see
    /// `CommonArgs::watch_sources`), re-expanded by `sync_patterns`.
    patterns: Vec<String>,
    text_filter: TextFilter,
}

impl App {
    pub fn load(common: &CommonArgs) -> Result<Self> {
        let text_filter = TextFilter::new();
        let mut files = Vec::new();
        let mut patterns = Vec::new();
        // Strict on purpose: at startup every resolved path must read
        // cleanly on the first try — no retries, no hiding. (Contrast
        // `reload`, where a vanished file is a normal event, not an error.)
        for source in common.watch_sources()? {
            match source {
                WatchSource::Literal(paths) => {
                    for path in paths {
                        let text = fs::read_to_string(&path)?;
                        let document = build_document(&text_filter, &text);
                        files.push(LoadedFile {
                            path,
                            document: Some(document),
                            from_pattern: false,
                        });
                    }
                }
                WatchSource::Pattern(pattern) => {
                    for path in expand_pattern(&pattern) {
                        let text = fs::read_to_string(&path)?;
                        let document = build_document(&text_filter, &text);
                        files.push(LoadedFile {
                            path,
                            document: Some(document),
                            from_pattern: true,
                        });
                    }
                    patterns.push(pattern);
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));

        let mut table_state = TableState::default();
        if !files.is_empty() {
            table_state.select(Some(0));
        }

        Ok(Self {
            files,
            filter: common.filter().map(String::from),
            expanded: HashSet::new(),
            pinned: HashSet::new(),
            mode: Mode::Normal,
            table_state,
            deferred_selection: None,
            last_rows_keys: Vec::new(),
            status: None,
            should_quit: false,
            patterns,
            text_filter,
        })
    }

    pub fn watched_paths(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().map(|f| f.path.as_path())
    }

    /// Directories to watch beyond the parents of the current files: the
    /// literal prefix of each live pattern. Without this, a pattern that
    /// matches nothing at startup watches nothing, and files matching it
    /// later would appear (via `sync_patterns`) but never live-refresh
    /// their content. Prefixes that don't exist yet can't be watched and
    /// are skipped.
    pub fn pattern_dirs(&self) -> Vec<PathBuf> {
        self.patterns
            .iter()
            .map(|p| pattern_base_dir(p))
            .filter_map(|d| fs::canonicalize(d).ok())
            .collect()
    }

    /// Re-reads and re-counts a single file in place. Called when the
    /// watcher reports that file changed; every other file's `Document` is
    /// left untouched.
    ///
    /// Never fails: a file that can't be read right now (editors replace
    /// files via unlink/rename dances that leave the path momentarily
    /// absent, or it was deleted outright) simply drops out of the table —
    /// same as a glob not matching it under `watch ncount src/*`. It comes
    /// back on its own when the watcher reports it again and a reload
    /// succeeds.
    pub fn reload(&mut self, path: &Path) {
        let Some(file) = self.files.iter_mut().find(|f| f.path == path) else {
            return;
        };
        file.document = read_with_retries(&file.path)
            .ok()
            .map(|text| build_document(&self.text_filter, &text));
    }

    /// Re-expand the live glob patterns and reconcile the file set with
    /// the result: newly matching files are added, pattern-matched files
    /// that no longer match are dropped. Literal-arg files are never
    /// touched — they hide via `reload` instead. Called once per event-
    /// loop tick (it early-returns when there are no patterns, and a
    /// re-expansion is just a directory scan), which makes membership
    /// self-healing without depending on which exact events arrive.
    /// Returns true when the set changed, so the caller can update the
    /// watcher's tracked paths.
    pub fn sync_patterns(&mut self) -> bool {
        if self.patterns.is_empty() {
            return false;
        }

        let mut matched = HashSet::new();
        for pattern in &self.patterns {
            matched.extend(expand_pattern(pattern));
        }

        let before = self.files.len();
        self.files
            .retain(|f| !f.from_pattern || matched.contains(&f.path));
        let mut changed = self.files.len() != before;

        for path in matched {
            if self.files.iter().any(|f| f.path == path) {
                // Already tracked. A file present but unreadable (document
                // is None) is deliberately NOT retried here — that would
                // burn the retry sleeps every tick on a permanently
                // unreadable file. Its recovery is event-driven, via
                // `reload`, same as any other tracked file.
                continue;
            }
            let document = read_with_retries(&path)
                .ok()
                .map(|text| build_document(&self.text_filter, &text));
            self.files.push(LoadedFile {
                path,
                document,
                from_pattern: true,
            });
            changed = true;
        }

        if changed {
            self.files.sort_by(|a, b| a.path.cmp(&b.path));
        }
        changed
    }

    /// The currently visible rows: a filtered subtree if `filter` matches
    /// something, otherwise every file chained in order. Mirrors the CLI's
    /// `StatFmt::apply_filter` fallback, including the "no match" warning.
    /// Files that currently can't be read (see `reload`) contribute no rows.
    pub fn rows(&mut self) -> Vec<RowData> {
        let previous_key = self
            .table_state
            .selected()
            .and_then(|i| self.last_rows_keys.get(i).cloned());
        let rows = self.collect_rows();

        if let Some(key) = &self.deferred_selection {
            if let Some(index) = rows.iter().position(|row| row_key(row) == *key) {
                self.deferred_selection = None;
                self.table_state.select(Some(index));
            } else if self.file_loaded(&key.0) {
                // The file is back but the heading isn't (renamed or
                // deleted in that edit): there is nothing to snap back to.
                self.deferred_selection = None;
            }
            // Otherwise the file is still hidden or dropped — keep the
            // restore pending and let the cursor sit clamped on a
            // surviving row below instead of hiding it for the gap.
        }

        // A selected row that just vanished because its file hid or was
        // dropped defers a snap-back for when the reload succeeds (the
        // same keyed restore the pinned-exception fold flow uses). Fold
        // and collapse flows reposition the selection before `rows()` runs,
        // so their previous key is always present and never defers.
        if self.deferred_selection.is_none()
            && let Some(key) = previous_key
            && !rows.iter().any(|row| row_key(row) == key)
            && !self.file_loaded(&key.0)
        {
            self.deferred_selection = Some(key);
        }

        // Rows can shrink out from under the selection when a file drops
        // out of the table; don't leave the cursor pointing into the void.
        if let Some(i) = self.table_state.selected()
            && i >= rows.len()
        {
            self.table_state.select(rows.len().checked_sub(1));
        }

        self.last_rows_keys = rows.iter().map(row_key).collect();
        rows
    }

    fn collect_rows(&mut self) -> Vec<RowData> {
        let rows = if let Some(filter) = self.filter.clone() {
            let needle = filter.to_ascii_uppercase();
            for file in &self.files {
                let Some(document) = &file.document else {
                    continue;
                };
                if let Some(found) = document.get_heading(&needle) {
                    self.status = None;
                    return self.visible_rows(flatten_document(&file.path, found));
                }
            }
            self.status = Some(format!(
                "no heading matching {filter:?} found; showing everything"
            ));
            self.all_rows()
        } else {
            self.status = None;
            self.all_rows()
        };

        self.visible_rows(rows)
    }

    fn all_rows(&self) -> Vec<RowData> {
        self.files
            .iter()
            .flat_map(|f| {
                f.document
                    .as_ref()
                    .map(|document| flatten_document(&f.path, document))
                    .unwrap_or_default()
            })
            .collect()
    }

    fn visible_rows(&self, rows: Vec<RowData>) -> Vec<RowData> {
        let mut structural_keys = HashSet::new();
        let mut visible = Vec::new();

        for mut row in rows {
            let key = row_key(&row);
            let structurally_visible = row.parent.as_ref().is_none_or(|parent| {
                structural_keys.contains(parent) && self.expanded.contains(parent)
            });
            let pinned = self.pinned.contains(&key);

            if structurally_visible || pinned {
                row.pinned_exception = pinned && !structurally_visible;
                if structurally_visible {
                    structural_keys.insert(key);
                }
                visible.push(row);
            }
        }

        visible
    }

    pub fn select_next(&mut self, row_count: usize) {
        self.deferred_selection = None;
        if row_count == 0 {
            return;
        }
        let next = match self.table_state.selected() {
            Some(i) if i + 1 < row_count => i + 1,
            Some(i) => i,
            None => 0,
        };
        self.table_state.select(Some(next));
    }

    pub fn select_prev(&mut self) {
        self.deferred_selection = None;
        let prev = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(prev));
    }

    /// Select a specific row directly (e.g. via a mouse click), discarding any
    /// deferred pinned-exception selection. The keyboard navigators
    /// (`select_next`/`select_prev`/`select_page_*`) clear deferred too; this
    /// is the direct-index counterpart for pointer input.
    pub fn select_index(&mut self, index: usize) {
        self.deferred_selection = None;
        self.table_state.select(Some(index));
    }

    pub fn select_page_down(&mut self, page_size: usize, row_count: usize) {
        self.deferred_selection = None;
        let offset = self.table_state.offset();
        let selected = self.table_state.selected().unwrap_or(offset);
        let (new_offset, new_selected) = page_down(offset, selected, page_size, row_count);
        *self.table_state.offset_mut() = new_offset;
        self.table_state.select(Some(new_selected));
    }

    pub fn select_page_up(&mut self, page_size: usize) {
        self.deferred_selection = None;
        let offset = self.table_state.offset();
        let selected = self.table_state.selected().unwrap_or(offset);
        let (new_offset, new_selected) = page_up(offset, selected, page_size);
        *self.table_state.offset_mut() = new_offset;
        self.table_state.select(Some(new_selected));
    }

    pub fn expand_selected(&mut self, rows: &[RowData]) {
        if let Some(key) = self.deferred_selection.clone() {
            let parent = self
                .all_rows()
                .into_iter()
                .find(|row| row_key(row) == key)
                .and_then(|row| row.parent);
            if let Some(parent) = parent {
                self.expanded.insert(parent);
                return;
            }
            self.deferred_selection = None;
        }

        if let Some(row) = self.selected_row(rows) {
            if row.pinned_exception
                && let Some(parent) = row.parent.clone()
            {
                self.expanded.insert(parent);
                self.deferred_selection = Some(row_key(row));
                self.table_state.select(None);
                return;
            }
            if row.has_children {
                self.expanded.insert(row_key(row));
            }
        }
    }

    pub fn collapse_selected(&mut self, rows: &[RowData]) {
        let Some(selected) = self.table_state.selected() else {
            return;
        };
        let Some(row) = rows.get(selected) else {
            return;
        };

        let (key, move_to_parent) = if row.has_children {
            (Some(row_key(row)), false)
        } else {
            (row.parent.clone(), true)
        };
        let Some(key) = key else {
            return;
        };

        self.expanded.remove(&key);
        if move_to_parent {
            let selected_key = row_key(row);
            if self.pinned.contains(&selected_key) {
                self.deferred_selection = Some(selected_key);
                self.table_state.select(None);
            } else if let Some(parent) = rows.iter().position(|candidate| row_key(candidate) == key)
            {
                self.deferred_selection = None;
                self.table_state.select(Some(parent));
            }
        }
    }

    pub fn toggle_selected(&mut self, rows: &[RowData]) {
        if let Some(key) = self.deferred_selection.clone() {
            if !self.pinned.remove(&key) {
                self.pinned.insert(key);
            }
            return;
        }

        if let Some(row) = self.selected_row(rows) {
            let key = row_key(row);
            if !self.pinned.remove(&key) {
                self.pinned.insert(key);
            }
        }
    }

    fn selected_row<'a>(&self, rows: &'a [RowData]) -> Option<&'a RowData> {
        self.table_state.selected().and_then(|i| rows.get(i))
    }

    /// True when `path` is tracked and currently readable. False means the
    /// file is hidden by a failed reload or dropped by `sync_patterns` —
    /// i.e. temporarily absent, and liable to reappear.
    fn file_loaded(&self, path: &Path) -> bool {
        self.files
            .iter()
            .any(|f| f.path == path && f.document.is_some())
    }

    pub fn enter_filter_mode(&mut self) {
        self.mode = Mode::Filter {
            buffer: String::new(),
            previous: self.filter.clone(),
        };
    }

    pub fn apply_filter(&mut self) {
        if let Mode::Filter { buffer, .. } = &self.mode {
            self.filter = if buffer.is_empty() {
                None
            } else {
                Some(buffer.clone())
            };
        }
        self.mode = Mode::Normal;
    }

    pub fn cancel_filter(&mut self) {
        if let Mode::Filter { previous, .. } = &mut self.mode {
            self.filter = previous.take();
        }
        self.mode = Mode::Normal;
    }
}

fn flatten_document(path: &Path, document: &Document) -> Vec<RowData> {
    let mut rows = Vec::new();
    let stats = document.stats();
    if stats.heading().is_some() {
        flatten_stats(path, stats, None, &mut rows);
    } else {
        for child in stats.children() {
            flatten_stats(path, child, None, &mut rows);
        }
    }
    rows
}

fn flatten_stats(
    path: &Path,
    stats: crate::document::DocumentStats<'_>,
    parent: Option<(PathBuf, String)>,
    rows: &mut Vec<RowData>,
) {
    let next_parent = if let Some(heading) = stats.heading() {
        let key = (path.to_path_buf(), heading.to_string());
        let subtree_paragraphs = stats.subtree_paragraphs();
        rows.push(RowData {
            path: path.to_path_buf(),
            heading: heading.to_string(),
            level: stats.level(),
            paragraphs: stats.paragraphs(),
            parent,
            subtree_paragraphs,
            has_children: stats.has_children(),
            pinned_exception: false,
        });
        Some(key)
    } else {
        parent
    };

    for child in stats.children() {
        flatten_stats(path, child, next_parent.clone(), rows);
    }
}

fn row_key(row: &RowData) -> (PathBuf, String) {
    (row.path.clone(), row.heading.clone())
}

fn build_document(filter: &TextFilter, text: &str) -> Document {
    let mut builder = DocumentBuilder::new();
    builder.apply(filter.lex(text));
    builder.finalize()
}

/// Delays before re-trying a failed read: once almost immediately (covers
/// the unlink/rename gap of an editor's atomic save), then once more after
/// a longer pause. Anything still unreadable after that has been gone for
/// longer than a save dance — no point polling further, since the
/// directory watch fires a fresh event the moment the path exists again.
const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(10), Duration::from_millis(75)];

fn read_with_retries(path: &Path) -> io::Result<String> {
    let mut result = fs::read_to_string(path);
    for delay in RETRY_DELAYS {
        if result.is_ok() {
            break;
        }
        std::thread::sleep(delay);
        result = fs::read_to_string(path);
    }
    result
}

/// Scroll the viewport down one page while keeping the cursor on the same
/// screen row. Returns the new `(offset, selected)`. The viewport scrolls
/// until its last page is full (`max_offset = row_count - page_size`); at
/// the bottom the cursor clamps onto the final rows.
fn page_down(offset: usize, selected: usize, page_size: usize, row_count: usize) -> (usize, usize) {
    if row_count == 0 || page_size == 0 {
        return (offset, selected);
    }
    let screen_pos = selected.saturating_sub(offset);
    let max_offset = row_count.saturating_sub(page_size);
    let new_offset = (offset + page_size).min(max_offset);
    let new_selected = (new_offset + screen_pos).min(row_count - 1);
    (new_offset, new_selected)
}

/// Scroll the viewport up one page while keeping the cursor on the same
/// screen row. Returns the new `(offset, selected)`.
fn page_up(offset: usize, selected: usize, page_size: usize) -> (usize, usize) {
    if page_size == 0 {
        return (offset, selected);
    }
    let screen_pos = selected.saturating_sub(offset);
    let new_offset = offset.saturating_sub(page_size);
    (new_offset, new_offset + screen_pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_down_keeps_cursor_on_same_screen_row() {
        // viewport of 10, cursor at screen row 3 -> page lands at offset 10,
        // cursor still on screen row 3 (now item 13).
        assert_eq!(page_down(0, 3, 10, 50), (10, 13));
    }

    #[test]
    fn page_down_clamps_at_the_bottom() {
        // last full page already in view (max_offset = 40): no movement.
        assert_eq!(page_down(40, 43, 10, 50), (40, 43));
    }

    #[test]
    fn page_down_near_bottom_scroll_fully_but_keep_screen_row() {
        // offset 35 -> clamps to 40; cursor screen row 3 preserved (38 -> 43).
        assert_eq!(page_down(35, 38, 10, 50), (40, 43));
    }

    #[test]
    fn page_down_is_a_noop_when_everything_fits() {
        // fewer rows than a viewport: max_offset is 0, so nothing scrolls.
        assert_eq!(page_down(0, 2, 10, 5), (0, 2));
    }

    #[test]
    fn page_up_keeps_cursor_on_same_screen_row() {
        assert_eq!(page_up(20, 23, 10), (10, 13));
    }

    #[test]
    fn page_up_clamps_at_the_top() {
        // offset 3 -> 0; cursor screen row 2 preserved (5 -> 2).
        assert_eq!(page_up(3, 5, 10), (0, 2));
    }

    fn test_app(files: Vec<LoadedFile>, patterns: Vec<String>) -> App {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        App {
            files,
            filter: None,
            expanded: HashSet::new(),
            pinned: HashSet::new(),
            mode: Mode::Normal,
            table_state,
            deferred_selection: None,
            last_rows_keys: Vec::new(),
            status: None,
            should_quit: false,
            patterns,
            text_filter: TextFilter::new(),
        }
    }

    fn file_on_disk(path: &Path) -> LoadedFile {
        let text_filter = TextFilter::new();
        let document = build_document(&text_filter, &fs::read_to_string(path).unwrap());
        LoadedFile {
            path: path.to_path_buf(),
            document: Some(document),
            from_pattern: false,
        }
    }

    fn app_with_file(dir: &tempfile::TempDir, name: &str, text: &str) -> (App, PathBuf) {
        let path = dir.path().join(name);
        fs::write(&path, text).unwrap();
        (test_app(vec![file_on_disk(&path)], Vec::new()), path)
    }

    /// An app over several named files, given already sorted by name.
    fn app_with_files(dir: &tempfile::TempDir, texts: &[(&str, &str)]) -> App {
        let files = texts
            .iter()
            .map(|(name, text)| {
                let path = dir.path().join(name);
                fs::write(&path, text).unwrap();
                file_on_disk(&path)
            })
            .collect();
        test_app(files, Vec::new())
    }

    #[test]
    fn reload_picks_up_new_content() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, path) = app_with_file(&dir, "ch1.md", "# One\n\nalpha beta gamma\n");

        fs::write(&path, "# One\n\nalpha beta gamma delta epsilon\n").unwrap();
        app.reload(&path);

        let rows = app.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].paragraphs.total, 5);
    }

    #[test]
    fn reload_hides_a_file_that_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, path) = app_with_file(&dir, "ch1.md", "# One\n\nalpha beta\n");

        fs::remove_file(&path).unwrap();
        app.reload(&path);

        assert!(app.files[0].document.is_none());
        assert!(app.rows().is_empty());
        // ...and the selection doesn't dangle past the now-empty table.
        assert_eq!(app.table_state.selected(), None);
    }

    #[test]
    fn reload_brings_a_returned_file_back() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, path) = app_with_file(&dir, "ch1.md", "# One\n\nalpha\n");

        fs::remove_file(&path).unwrap();
        app.reload(&path);
        fs::write(&path, "# One\n\nalpha beta gamma delta\n").unwrap();
        app.reload(&path);

        let rows = app.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].paragraphs.total, 4);
    }

    #[test]
    fn selection_snaps_back_when_hidden_file_reappears() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_files(
            &dir,
            &[("a.md", "# Aye\n\nalpha\n"), ("b.md", "# Bee\n\nbeta\n")],
        );

        app.rows();
        app.select_next(2);
        assert_eq!(app.table_state.selected(), Some(1));

        // Rapid saves can outlast the read retries: b hides and its rows
        // vanish; the cursor clamps onto the surviving neighbor.
        let b = dir.path().join("b.md");
        fs::remove_file(&b).unwrap();
        app.reload(&b);
        let rows = app.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(app.table_state.selected(), Some(0));
        assert!(app.deferred_selection.is_some());

        // The file returns; the selection snaps back to the row it was on.
        fs::write(&b, "# Bee\n\nbeta gamma\n").unwrap();
        app.reload(&b);
        let rows = app.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(app.table_state.selected(), Some(1));
        assert_eq!(rows[1].heading, "Bee");
    }

    #[test]
    fn navigation_while_file_hidden_cancels_restore() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_files(
            &dir,
            &[("a.md", "# Aye\n\nalpha\n"), ("b.md", "# Bee\n\nbeta\n")],
        );

        app.rows();
        app.select_next(2);
        let b = dir.path().join("b.md");
        fs::remove_file(&b).unwrap();
        app.reload(&b);
        app.rows();
        assert!(app.deferred_selection.is_some());

        // Moving the cursor during the gap is an explicit choice — the
        // pending restore is discarded.
        app.select_prev();
        assert!(app.deferred_selection.is_none());

        fs::write(&b, "# Bee\n\nbeta\n").unwrap();
        app.reload(&b);
        let rows = app.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn deferred_restore_gives_up_when_heading_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_files(
            &dir,
            &[("a.md", "# Aye\n\nalpha\n"), ("b.md", "# Bee\n\nbeta\n")],
        );

        app.rows();
        app.select_next(2);
        let b = dir.path().join("b.md");
        fs::remove_file(&b).unwrap();
        app.reload(&b);
        app.rows();
        assert!(app.deferred_selection.is_some());

        // b returns with its heading renamed: no row to restore to, so the
        // deferred key is dropped instead of lingering forever.
        fs::write(&b, "# Boo\n\nbeta\n").unwrap();
        app.reload(&b);
        let rows = app.rows();
        assert!(app.deferred_selection.is_none());
        assert_eq!(
            rows.iter()
                .map(|row| row.heading.as_str())
                .collect::<Vec<_>>(),
            ["Aye", "Boo"]
        );
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn cursor_stays_visible_while_selected_file_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = app_with_files(
            &dir,
            &[("a.md", "# Aye\n\nalpha\n"), ("b.md", "# Bee\n\nbeta\n")],
        );

        app.rows();
        app.select_next(2);
        let b = dir.path().join("b.md");
        fs::remove_file(&b).unwrap();
        app.reload(&b);
        let rows = app.rows();
        assert_eq!(rows.len(), 1);
        // Not select(None): the cursor rides the neighbor until b returns.
        assert_eq!(app.table_state.selected(), Some(0));
    }

    #[test]
    fn rows_start_collapsed_and_expand_into_children() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, _path) = app_with_file(
            &dir,
            "book.md",
            "# One\n\none two\n\n## Child\n\nthree four five\n\n# Two\n\na b\n",
        );

        let rows = app.rows();
        assert_eq!(
            rows.iter()
                .map(|row| row.heading.as_str())
                .collect::<Vec<_>>(),
            ["One", "Two"]
        );
        assert_eq!(rows[0].subtree_paragraphs.total, 5);
        assert_eq!(rows[0].paragraphs.total, 2);
        assert!(rows[0].has_children);

        app.expand_selected(&rows);
        let rows = app.rows();
        assert_eq!(
            rows.iter()
                .map(|row| row.heading.as_str())
                .collect::<Vec<_>>(),
            ["One", "Child", "Two"]
        );
        assert_eq!(rows[1].subtree_paragraphs.total, 3);
        assert!(!rows[1].has_children);
    }

    #[test]
    fn collapsing_an_unpinned_leaf_selects_its_parent() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, _path) = app_with_file(
            &dir,
            "book.md",
            "# One\n\none two\n\n## Child\n\nthree four five\n\n# Two\n\na b\n",
        );

        let rows = app.rows();
        app.expand_selected(&rows);
        let rows = app.rows();
        app.select_next(rows.len());
        assert_eq!(app.table_state.selected(), Some(1));

        app.collapse_selected(&rows);

        assert_eq!(app.table_state.selected(), Some(0));
        let rows = app.rows();
        assert_eq!(
            rows.iter()
                .map(|row| row.heading.as_str())
                .collect::<Vec<_>>(),
            ["One", "Two"]
        );
    }

    #[test]
    fn pinned_child_survives_when_parent_is_collapsed() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, _path) = app_with_file(
            &dir,
            "book.md",
            "# One\n\none two\n\n## Child\n\nthree four five\n\n# Two\n\na b\n",
        );

        let rows = app.rows();
        app.expand_selected(&rows);
        let rows = app.rows();
        app.select_next(rows.len());
        app.toggle_selected(&rows);
        app.select_prev();
        app.collapse_selected(&rows);

        let rows = app.rows();
        assert_eq!(
            rows.iter()
                .map(|row| row.heading.as_str())
                .collect::<Vec<_>>(),
            ["One", "Child", "Two"]
        );
        assert!(rows[1].pinned_exception);
        assert_eq!(rows[0].subtree_paragraphs.total, 5);
    }

    #[test]
    fn pinned_leaf_selection_survives_parent_fold_and_reopens_it() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, _path) = app_with_file(
            &dir,
            "book.md",
            "# One\n\none\n\n## A\n\na\n\n## Child\n\nchild\n\n## C\n\nc\n\n# Two\n\ntwo\n",
        );

        let rows = app.rows();
        app.expand_selected(&rows);
        let rows = app.rows();
        app.select_next(rows.len());
        app.select_next(rows.len());
        assert_eq!(rows[app.table_state.selected().unwrap()].heading, "Child");
        app.toggle_selected(&rows);

        app.collapse_selected(&rows);
        let rows = app.rows();
        assert_eq!(
            rows.iter()
                .map(|row| row.heading.as_str())
                .collect::<Vec<_>>(),
            ["One", "Child", "Two"]
        );
        assert_eq!(app.table_state.selected(), Some(1));
        assert!(rows[1].pinned_exception);

        app.expand_selected(&rows);
        let rows = app.rows();
        assert_eq!(
            rows.iter()
                .map(|row| row.heading.as_str())
                .collect::<Vec<_>>(),
            ["One", "A", "Child", "C", "Two"]
        );
        assert_eq!(app.table_state.selected(), Some(2));
    }

    #[test]
    fn filtered_rows_keep_the_matching_heading_as_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, _path) = app_with_file(
            &dir,
            "book.md",
            "# One\n\none two\n\n## Child\n\nthree four five\n",
        );
        app.filter = Some("Child".to_string());

        let rows = app.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].heading, "Child");
        assert_eq!(rows[0].parent, None);
    }

    /// An app watching a (canonicalized) `*.md` pattern over `dir`, with no
    /// files resolved yet — the zero-match startup case.
    fn app_with_pattern(dir: &tempfile::TempDir) -> (App, PathBuf) {
        let base = dir.path().canonicalize().unwrap();
        let pattern = base.join("*.md").to_string_lossy().into_owned();
        (test_app(Vec::new(), vec![pattern]), base)
    }

    #[test]
    fn sync_patterns_picks_up_new_files() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, base) = app_with_pattern(&dir);
        assert!(app.rows().is_empty());

        fs::write(base.join("ch1.md"), "# One\n\nalpha beta\n").unwrap();
        assert!(app.sync_patterns());
        let rows = app.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].paragraphs.total, 2);

        fs::write(base.join("ch2.md"), "# Two\n\ngamma delta epsilon\n").unwrap();
        assert!(app.sync_patterns());
        assert_eq!(app.rows().len(), 2);

        // A steady state: no membership change, sync reports nothing.
        assert!(!app.sync_patterns());
    }

    #[test]
    fn sync_patterns_drops_files_that_stop_matching() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, base) = app_with_pattern(&dir);
        fs::write(base.join("ch1.md"), "# One\n\nalpha\n").unwrap();
        fs::write(base.join("ch2.md"), "# Two\n\nbeta\n").unwrap();
        app.sync_patterns();
        assert_eq!(app.files.len(), 2);

        fs::remove_file(base.join("ch1.md")).unwrap();
        assert!(app.sync_patterns());

        assert_eq!(app.files.len(), 1);
        assert_eq!(app.files[0].path.file_name().unwrap(), "ch2.md");
        let rows = app.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].heading, "Two");
    }

    #[test]
    fn sync_patterns_never_drops_literal_files() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let path = base.join("ch1.md");
        fs::write(&path, "# One\n\nalpha beta\n").unwrap();
        let text_filter = TextFilter::new();
        let document = build_document(&text_filter, &fs::read_to_string(&path).unwrap());
        let mut app = test_app(
            vec![LoadedFile {
                path: path.clone(),
                document: Some(document),
                from_pattern: false,
            }],
            vec![base.join("*.md").to_string_lossy().into_owned()],
        );

        // The literal file is also matched by the pattern — no duplicate
        // is added, and the existing entry stays literal.
        assert!(!app.sync_patterns());
        assert_eq!(app.files.len(), 1);
        assert!(!app.files[0].from_pattern);

        // When it vanishes, sync leaves it alone (hiding is reload's job),
        // even though the pattern no longer matches it.
        fs::remove_file(&path).unwrap();
        assert!(!app.sync_patterns());
        assert_eq!(app.files.len(), 1);
    }

    #[test]
    fn pins_survive_membership_changes() {
        let dir = tempfile::tempdir().unwrap();
        let (mut app, base) = app_with_pattern(&dir);
        fs::write(base.join("b.md"), "# Bee\n\nalpha beta\n").unwrap();
        app.sync_patterns();

        let rows = app.rows();
        app.toggle_selected(&rows);
        assert_eq!(app.pinned.len(), 1);

        // A new file that sorts BEFORE the pinned one shifts every row
        // index; the pin (keyed by path, not index) must not move.
        fs::write(base.join("a.md"), "# Aye\n\ngamma\n").unwrap();
        app.sync_patterns();

        let rows = app.rows();
        assert_eq!(rows[0].heading, "Aye");
        assert_eq!(rows[1].heading, "Bee");
        assert!(app.pinned.contains(&(base.join("b.md"), "Bee".to_string())));
    }
}
