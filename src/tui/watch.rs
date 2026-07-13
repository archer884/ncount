use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

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
    pub fn new<'a>(paths: impl Iterator<Item = &'a Path>) -> Result<Self> {
        let tracked: HashSet<PathBuf> = paths.map(Path::to_path_buf).collect();

        let (tx, rx) = mpsc::channel();
        let mut debouncer = new_debouncer(DEBOUNCE, tx)?;

        let dirs: HashSet<&Path> = tracked.iter().filter_map(|p| p.parent()).collect();
        for dir in dirs {
            debouncer
                .watcher()
                .watch(dir, RecursiveMode::NonRecursive)?;
        }

        Ok(Self {
            tracked,
            rx,
            _debouncer: debouncer,
        })
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
