use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use ratatui::widgets::TableState;

use crate::cli::CommonArgs;
use crate::document::{Document, DocumentBuilder, Paragraphs};
use crate::filter::TextFilter;
use crate::Result;

pub struct LoadedFile {
    pub path: PathBuf,
    pub document: Document,
}

pub enum Mode {
    Normal,
    Filter { buffer: String, previous: Option<String> },
}

/// A single flattened row for display: one heading, wherever it lives.
pub struct RowData {
    pub file_index: usize,
    pub heading: String,
    pub level: i32,
    pub paragraphs: Paragraphs,
}

pub struct App {
    pub files: Vec<LoadedFile>,
    pub filter: Option<String>,
    pub expanded: HashSet<(usize, String)>,
    pub mode: Mode,
    pub table_state: TableState,
    pub status: Option<String>,
    pub should_quit: bool,
    text_filter: TextFilter,
}

impl App {
    pub fn load(common: &CommonArgs) -> Result<Self> {
        let text_filter = TextFilter::new();
        let mut files = Vec::new();
        for path in common.materialize_files() {
            let document = build_document(&text_filter, &path)?;
            files.push(LoadedFile { path, document });
        }

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
            text_filter,
        })
    }

    pub fn watched_paths(&self) -> impl Iterator<Item = &Path> {
        self.files.iter().map(|f| f.path.as_path())
    }

    /// Re-reads and re-counts a single file in place. Called when the
    /// watcher reports that file changed; every other file's `Document` is
    /// left untouched.
    pub fn reload(&mut self, path: &Path) -> Result<()> {
        if let Some(file) = self.files.iter_mut().find(|f| f.path == path) {
            file.document = build_document(&self.text_filter, &file.path)?;
        }
        Ok(())
    }

    /// The currently visible rows: a filtered subtree if `filter` matches
    /// something, otherwise every file chained in order. Mirrors the CLI's
    /// `StatFmt::apply_filter` fallback, including the "no match" warning.
    pub fn rows(&mut self) -> Vec<RowData> {
        if let Some(filter) = self.filter.clone() {
            let needle = filter.to_ascii_uppercase();
            for (i, file) in self.files.iter().enumerate() {
                if let Some(found) = file.document.get_heading(&needle) {
                    self.status = None;
                    return flatten(i, found);
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
            .enumerate()
            .flat_map(|(i, f)| flatten(i, &f.document))
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

    pub fn expand_selected(&mut self, rows: &[RowData]) {
        if let Some(row) = self.selected_row(rows) {
            self.expanded.insert((row.file_index, row.heading.clone()));
        }
    }

    pub fn collapse_selected(&mut self, rows: &[RowData]) {
        if let Some(row) = self.selected_row(rows) {
            self.expanded.remove(&(row.file_index, row.heading.clone()));
        }
    }

    pub fn toggle_selected(&mut self, rows: &[RowData]) {
        if let Some(row) = self.selected_row(rows) {
            let key = (row.file_index, row.heading.clone());
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

fn flatten(file_index: usize, document: &Document) -> Vec<RowData> {
    document
        .iter()
        .map(|stats| RowData {
            file_index,
            heading: stats.heading().unwrap_or_default().to_string(),
            level: stats.level(),
            paragraphs: stats.paragraphs(),
        })
        .collect()
}

fn build_document(filter: &TextFilter, path: &Path) -> Result<Document> {
    let text = fs::read_to_string(path)?;
    let mut builder = DocumentBuilder::new();
    builder.apply(filter.filter_text(&text));
    Ok(builder.finalize())
}
