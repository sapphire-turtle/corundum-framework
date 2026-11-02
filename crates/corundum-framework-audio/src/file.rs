//! The opener for files on disk.

use std::path::PathBuf;

use crate::feed::{FailedFeed, spawn_feed};
use crate::{Feed, Opener, OutputConfig, TrackError};

/// Opens local files: the extension is the format hint, nothing is preloaded.
#[derive(Clone, Copy, Debug, Default)]
pub struct FileOpener;

impl FileOpener {
    pub fn new() -> FileOpener {
        FileOpener
    }
}

impl Opener<PathBuf> for FileOpener {
    fn open(&mut self, entry: &PathBuf, at_ms: u64, target: OutputConfig) -> Box<dyn Feed> {
        let file = match std::fs::File::open(entry) {
            Ok(f) => f,
            Err(e) => return Box::new(FailedFeed(TrackError::Unplayable(e.to_string()))),
        };
        let hint = entry
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_owned);
        spawn_feed(Box::new(file), None, hint, at_ms, target)
    }

    fn preload(&mut self, _next: Option<&PathBuf>) {}

    fn clear(&mut self) {}
}
