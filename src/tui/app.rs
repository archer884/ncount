use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use ratatui::widgets::TableState;

use crate::cli::{expand_pattern, pattern_base_dir, CommonArgs, WatchSource};
use crate::document::{Document, DocumentBuilder, Paragraphs};
use crate::filter::TextFilter;
use crate::Result;

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
}

/// A single flattened row for display: one heading, wherever it lives.
pub struct RowData {
    pub path: PathBuf,
    pub heading: String,
    pub level: i32,
    pub paragraphs: Paragraphs,
}

pub struct App {
    pub files: Vec<LoadedFile>,
    pub filter: Option<String>,
    /// Pins are keyed by (file path, heading), not row index, so a
    /// membership change that reshuffles the table (see `sync_patterns`)
    /// can't scramble which rows are expanded.
    pub expanded: HashSet<(PathBuf, String)>,
    pub mode: Mode,
    pub table_state: TableState,
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
            mode: Mode::Normal,
            table_state,
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
        let rows = self.collect_rows();

        // Rows can shrink out from under the selection when a file drops
        // out of the table; don't leave the cursor pointing into the void.
        if let Some(i) = self.table_state.selected() {
            if i >= rows.len() {
                self.table_state.select(rows.len().checked_sub(1));
            }
        }

        rows
    }

    fn collect_rows(&mut self) -> Vec<RowData> {
        if let Some(filter) = self.filter.clone() {
            let needle = filter.to_ascii_uppercase();
            for file in &self.files {
                let Some(document) = &file.document else {
                    continue;
                };
                if let Some(found) = document.get_heading(&needle) {
                    self.status = None;
                    return flatten(&file.path, found);
                }
            }
            self.status = Some(format!(
                "no heading matching {filter:?} found; showing everything"
            ));
        } else {
            self.status = None;
        }

        self.files
            .iter()
            .flat_map(|f| {
                f.document
                    .as_ref()
                    .map(|document| flatten(&f.path, document))
                    .unwrap_or_default()
            })
            .collect()
    }

    pub fn select_next(&mut self, row_count: usize) {
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
        let prev = match self.table_state.selected() {
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.table_state.select(Some(prev));
    }

    pub fn select_page_down(&mut self, page_lines: usize, rows: &[RowData]) {
        if rows.is_empty() || page_lines == 0 {
            return;
        }
        let offset = self.table_state.offset();
        let selected = self.table_state.selected().unwrap_or(offset);
        let heights = self.heights(rows);
        let (new_offset, new_selected) = page_down(&heights, offset, selected, page_lines);
        *self.table_state.offset_mut() = new_offset;
        self.table_state.select(Some(new_selected));
    }

    pub fn select_page_up(&mut self, page_lines: usize, rows: &[RowData]) {
        if rows.is_empty() || page_lines == 0 {
            return;
        }
        let offset = self.table_state.offset();
        let selected = self.table_state.selected().unwrap_or(offset);
        let heights = self.heights(rows);
        let (new_offset, new_selected) = page_up(&heights, offset, selected, page_lines);
        *self.table_state.offset_mut() = new_offset;
        self.table_state.select(Some(new_selected));
    }

    /// Terminal-line height of each row for paging. Only **pinned** rows count
    /// as 2 lines — the selected row's extra detail line is deliberately *not*
    /// counted, because the selection moves on every PgUp/PgDn and counting it
    /// would make the height map change under each move. That change is what
    /// made paging drift a notch per cycle (forward paid for the 2-line top,
    /// backward didn't). A pinned set is stable across moves, so the pager
    /// becomes an exact inverse of itself and drift disappears. With no pins
    /// (the usual case) this is a flat all-1 map — the classic row-for-row
    /// page. The selected row still *renders* 2 lines tall either way; only the
    /// paging math ignores its extra line.
    fn heights(&self, rows: &[RowData]) -> Vec<usize> {
        rows.iter()
            .map(|row| {
                if self
                    .expanded
                    .contains(&(row.path.clone(), row.heading.clone()))
                {
                    2
                } else {
                    1
                }
            })
            .collect()
    }

    pub fn expand_selected(&mut self, rows: &[RowData]) {
        if let Some(row) = self.selected_row(rows) {
            self.expanded.insert((row.path.clone(), row.heading.clone()));
        }
    }

    pub fn collapse_selected(&mut self, rows: &[RowData]) {
        if let Some(row) = self.selected_row(rows) {
            self.expanded.remove(&(row.path.clone(), row.heading.clone()));
        }
    }

    pub fn toggle_selected(&mut self, rows: &[RowData]) {
        if let Some(row) = self.selected_row(rows) {
            let key = (row.path.clone(), row.heading.clone());
            if !self.expanded.remove(&key) {
                self.expanded.insert(key);
            }
        }
    }

    fn selected_row<'a>(&self, rows: &'a [RowData]) -> Option<&'a RowData> {
        self.table_state.selected().and_then(|i| rows.get(i))
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

fn flatten(path: &Path, document: &Document) -> Vec<RowData> {
    document
        .iter()
        .map(|stats| RowData {
            path: path.to_path_buf(),
            heading: stats.heading().unwrap_or_default().to_string(),
            level: stats.level(),
            paragraphs: stats.paragraphs(),
        })
        .collect()
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

/// Scroll the viewport down one page (of terminal lines) while keeping the
/// cursor on the same screen line. Returns the new `(offset, selected)`.
///
/// `heights[i]` is the terminal-line height of row `i` (1 normally, 2 when
/// it's the selected or pinned row, since those draw a detail line). This
/// matters because the viewport is sized in *lines*, not rows: a page with a
/// 2-line row in it holds one fewer row than its line budget suggests, so the
/// old row-count math drifted by ~1 per expanded row. The viewport scrolls
/// until its last page is full; at the bottom the cursor clamps on.
fn page_down(heights: &[usize], offset: usize, selected: usize, page_lines: usize) -> (usize, usize) {
    let row_count = heights.len();
    if row_count == 0 || page_lines == 0 {
        return (offset, selected);
    }
    let screen_lines = lines_between(heights, offset, selected);
    let max_offset = last_page_top(heights, page_lines);
    let new_offset = advance_forward(heights, offset, page_lines).min(max_offset);
    let new_selected = row_at_line(heights, new_offset, screen_lines).min(row_count - 1);
    (new_offset, new_selected)
}

/// Scroll the viewport up one page (of terminal lines) keeping the cursor on
/// the same screen line.
fn page_up(heights: &[usize], offset: usize, selected: usize, page_lines: usize) -> (usize, usize) {
    let row_count = heights.len();
    if row_count == 0 || page_lines == 0 {
        return (offset, selected);
    }
    let screen_lines = lines_between(heights, offset, selected);
    let new_offset = advance_backward(heights, offset, page_lines);
    let new_selected = row_at_line(heights, new_offset, screen_lines).min(row_count - 1);
    (new_offset, new_selected)
}

/// Terminal lines occupied by rows `[from, to)`.
fn lines_between(heights: &[usize], from: usize, to: usize) -> usize {
    heights[from.min(heights.len())..to.min(heights.len())]
        .iter()
        .copied()
        .sum()
}

/// First row that no longer fully fits within `lines` starting at `from` —
/// the new viewport top after scrolling down one page. Never less than `from`.
fn advance_forward(heights: &[usize], from: usize, lines: usize) -> usize {
    let (mut acc, mut idx) = (0, from);
    while idx < heights.len() {
        let h = heights[idx];
        if acc + h > lines {
            break;
        }
        acc += h;
        idx += 1;
    }
    idx
}

/// First row such that `[it, from)` fits within `lines` — the new viewport top
/// after scrolling up one page.
fn advance_backward(heights: &[usize], from: usize, lines: usize) -> usize {
    let (mut acc, mut idx) = (0, from);
    while idx > 0 {
        let h = heights[idx - 1];
        if acc + h > lines {
            break;
        }
        acc += h;
        idx -= 1;
    }
    idx
}

/// Smallest top from which every remaining row fits within `lines`: the last
/// full page. Scrolling past it would only move the cursor, not the content.
fn last_page_top(heights: &[usize], lines: usize) -> usize {
    let (mut acc, mut idx) = (0, heights.len());
    while idx > 0 {
        let h = heights[idx - 1];
        if acc + h > lines {
            break;
        }
        acc += h;
        idx -= 1;
    }
    idx
}

/// Row containing the `line`-th screen line below `from` (0 = `from` itself).
fn row_at_line(heights: &[usize], from: usize, line: usize) -> usize {
    let (mut acc, mut idx) = (0, from);
    while idx < heights.len() {
        let h = heights[idx];
        if acc + h > line {
            break;
        }
        acc += h;
        idx += 1;
    }
    idx.min(heights.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(n: usize) -> Vec<usize> {
        vec![1; n]
    }

    #[test]
    fn page_down_keeps_cursor_on_same_screen_line() {
        // viewport of 10 lines, cursor at screen line 3 -> page lands at
        // offset 10, cursor still on screen line 3 (now item 13).
        assert_eq!(page_down(&flat(50), 0, 3, 10), (10, 13));
    }

    #[test]
    fn page_down_clamps_at_the_bottom() {
        // last full page already in view (max_offset = 40): no movement.
        assert_eq!(page_down(&flat(50), 40, 43, 10), (40, 43));
    }

    #[test]
    fn page_down_near_bottom_scroll_fully_but_keep_screen_line() {
        // offset 35 -> clamps to 40; cursor screen line 3 preserved (38 -> 43).
        assert_eq!(page_down(&flat(50), 35, 38, 10), (40, 43));
    }

    #[test]
    fn page_down_is_a_noop_when_everything_fits() {
        // fewer rows than a viewport: max_offset is 0, so nothing scrolls.
        assert_eq!(page_down(&flat(5), 0, 2, 10), (0, 2));
    }

    #[test]
    fn page_up_keeps_cursor_on_same_screen_line() {
        assert_eq!(page_up(&flat(50), 20, 23, 10), (10, 13));
    }

    #[test]
    fn page_up_clamps_at_the_top() {
        // offset 3 -> 0; cursor screen line 2 preserved (5 -> 2).
        assert_eq!(page_up(&flat(50), 3, 5, 10), (0, 2));
    }

    #[test]
    fn page_down_advances_less_when_top_row_is_pinned() {
        // A pinned row at the top is 2 lines tall, so the 10-line viewport
        // holds rows 0..8 (2 + 8*1 = 10) with row 9 spilling -> the next page
        // top is row 9, not 10. Cursor sits at screen line 0 and follows.
        let mut h = flat(50);
        h[0] = 2;
        assert_eq!(page_down(&h, 0, 0, 10), (9, 9));
    }

    #[test]
    fn page_down_consumes_extra_line_of_a_pinned_row() {
        // A pinned 2-line row mid-page (idx 5) makes a 10-line page hold one
        // fewer row: new top is row 9 (not 10), cursor on screen line 3 lands
        // at 12 (not 13). Contrast the flat-list (10, 13) above.
        let mut h = flat(50);
        h[5] = 2;
        assert_eq!(page_down(&h, 0, 3, 10), (9, 12));
    }

    #[test]
    fn page_up_consumes_extra_line_of_a_pinned_row() {
        // Symmetric: paging up across a 2-line row (idx 15) lands one row
        // lower than the flat (10, 13) case.
        let mut h = flat(50);
        h[15] = 2;
        assert_eq!(page_up(&h, 20, 23, 10), (11, 14));
    }

    #[test]
    fn heights_ignores_the_selected_row_and_counts_only_pins() {
        // The selected row renders 2 lines tall but the pager must treat it as
        // 1 — otherwise the height map changes on every move and PgUp/PgDn
        // drift a notch per cycle. Only pinned rows count as 2.
        let dir = tempfile::tempdir().unwrap();
        let (mut app, _path) = app_with_file(
            &dir,
            "ch.md",
            "# A\n\na b c\n\n# B\n\nd e f\n\n# C\n\ng h i\n",
        );
        let rows = app.rows();
        assert_eq!(rows.len(), 3);
        // Row 0 is selected, nothing pinned -> all rows height 1.
        assert_eq!(app.heights(&rows), vec![1, 1, 1]);

        // Pin row 1 (not the selected one): only it becomes height 2.
        app.expanded
            .insert((rows[1].path.clone(), rows[1].heading.clone()));
        assert_eq!(app.heights(&rows), vec![1, 2, 1]);
    }

    fn test_app(files: Vec<LoadedFile>, patterns: Vec<String>) -> App {
        let mut table_state = TableState::default();
        table_state.select(Some(0));
        App {
            files,
            filter: None,
            expanded: HashSet::new(),
            mode: Mode::Normal,
            table_state,
            status: None,
            should_quit: false,
            patterns,
            text_filter: TextFilter::new(),
        }
    }

    fn app_with_file(dir: &tempfile::TempDir, name: &str, text: &str) -> (App, PathBuf) {
        let path = dir.path().join(name);
        fs::write(&path, text).unwrap();
        let text_filter = TextFilter::new();
        let document = build_document(&text_filter, &fs::read_to_string(&path).unwrap());
        (
            test_app(
                vec![LoadedFile {
                    path: path.clone(),
                    document: Some(document),
                    from_pattern: false,
                }],
                Vec::new(),
            ),
            path,
        )
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
        assert_eq!(app.expanded.len(), 1);

        // A new file that sorts BEFORE the pinned one shifts every row
        // index; the pin (keyed by path, not index) must not move.
        fs::write(base.join("a.md"), "# Aye\n\ngamma\n").unwrap();
        app.sync_patterns();

        let rows = app.rows();
        assert_eq!(rows[0].heading, "Aye");
        assert_eq!(rows[1].heading, "Bee");
        assert!(app
            .expanded
            .contains(&(base.join("b.md"), "Bee".to_string())));
    }
}
