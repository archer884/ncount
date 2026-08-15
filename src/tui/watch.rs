use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{DebounceEventResult, Debouncer, new_debouncer};

use crate::Result;

const DEBOUNCE: Duration = Duration::from_millis(300);

pub struct Watch {
    tracked: HashSet<PathBuf>,
    rx: mpsc::Receiver<DebounceEventResult>,
    // Held only to keep the background watcher thread alive; dropping this
    // stops the watch.
    _debouncer: Debouncer<notify::RecommendedWatcher>,
}

impl Watch {
    /// `paths` are the tracked files; their parent directories are watched.
    /// `extra_dirs` are additional directories to watch (the literal
    /// prefixes of live glob patterns — needed so a pattern that matches
    /// nothing yet still sees its first match arrive).
    pub fn new<'a>(
        paths: impl Iterator<Item = &'a Path>,
        extra_dirs: impl Iterator<Item = &'a Path>,
    ) -> Result<Self> {
        let tracked: HashSet<PathBuf> = paths.map(Path::to_path_buf).collect();

        let (tx, rx) = mpsc::channel();
        let mut debouncer = new_debouncer(DEBOUNCE, tx)?;

        let mut dirs: HashSet<PathBuf> = tracked
            .iter()
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .collect();
        dirs.extend(extra_dirs.map(Path::to_path_buf));
        for dir in dirs {
            debouncer
                .watcher()
                .watch(&dir, RecursiveMode::NonRecursive)?;
        }

        Ok(Self {
            tracked,
            rx,
            _debouncer: debouncer,
        })
    }

    /// Replace the tracked file set after the app's membership changes (a
    /// live glob pattern gained or lost files), so `changed` reports
    /// events for exactly the current files.
    pub fn set_tracked(&mut self, paths: impl Iterator<Item = PathBuf>) {
        self.tracked = paths.collect();
    }

    /// Paths, among the ones we're tracking, that changed since the last
    /// call. Never blocks.
    pub fn changed(&self) -> Vec<PathBuf> {
        let mut changed = Vec::new();
        while let Ok(result) = self.rx.try_recv() {
            let Ok(events) = result else { continue };
            for event in events {
                if self.tracked.contains(&event.path) && !changed.contains(&event.path) {
                    changed.push(event.path);
                }
            }
        }
        changed
    }
}
