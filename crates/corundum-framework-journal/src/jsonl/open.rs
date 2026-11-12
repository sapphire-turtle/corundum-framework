//! Opening a [`JsonlStore`]: its lock, its files, and the index rebuilt from them.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::month::Month;
use super::{JsonlStore, MonthFile, Span, io_error};
use crate::{BatchEnd, JournalStore, Record, StoreError};

/// The lock file's name.
const LOCK: &str = "journal.lock";

impl JsonlStore {
    /// Opens the store in `dir`, creating the directory if it is missing, and reads its
    /// month files.
    ///
    /// Fails with [`StoreError::Io`] if the directory cannot be made or read, or if another
    /// store has it open.
    pub fn open(dir: impl AsRef<Path>) -> Result<JsonlStore, StoreError> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(io_error)?;
        let lock = lock(&dir)?;
        let mut store = JsonlStore {
            dir,
            _lock: lock,
            files: Vec::new(),
            newest: None,
            order: Vec::new(),
            position: HashMap::new(),
            batches: HashMap::new(),
            #[cfg(test)]
            faults: Default::default(),
        };
        for (month, path) in month_files(&store.dir)? {
            cut_torn_line(&path)?;
            store.files.push(MonthFile {
                month,
                path,
                len: 0,
                dirty: false,
            });
            store.replay(store.files.len() - 1)?;
        }
        if let Some(last) = store.files.last() {
            let newest = super::open_to_write(&last.path, false).map_err(io_error)?;
            store.newest = Some(newest);
        }
        store.end_unfinished()?;
        Ok(store)
    }

    /// Reads file `file` into the index.
    fn replay(&mut self, file: usize) -> Result<(), StoreError> {
        let path = self.files[file].path.clone();
        let mut reader = BufReader::new(File::open(&path).map_err(io_error)?);
        let mut line = Vec::new();
        let mut offset = 0u64;
        let mut number = 0u64;
        loop {
            line.clear();
            let n = reader.read_until(b'\n', &mut line).map_err(io_error)?;
            if n == 0 {
                break;
            }
            let span = Span {
                file,
                offset,
                len: u32::try_from(n).map_err(|_| corrupt(&path, offset, "a line too long"))?,
            };
            if line.last() != Some(&b'\n') {
                // `cut_torn_line` cut it; the file has grown since.
                return Err(corrupt(&path, offset, "a torn last line"));
            }
            offset += n as u64;
            number += 1;
            if line.trim_ascii().is_empty() {
                continue;
            }
            match serde_json::from_slice::<Record>(&line) {
                Ok(record) => self.index(&record, span),
                Err(e) => {
                    tracing::warn!(path = %path.display(), line = number, error = %e, "a journal line that does not parse skipped");
                }
            }
        }
        self.files[file].len = offset;
        Ok(())
    }

    /// Ends every batch that has no end, as interrupted, in the order they began.
    fn end_unfinished(&mut self) -> Result<(), StoreError> {
        let unfinished: Vec<_> = self
            .order
            .iter()
            .filter_map(|id| self.batches.get(id))
            .filter(|b| !b.summary.complete)
            .map(|b| BatchEnd {
                batch_id: b.summary.begin.batch_id,
                done: b.summary.done,
                skipped: b.summary.skipped,
                interrupted: true,
            })
            .collect();
        if unfinished.is_empty() {
            return Ok(());
        }
        tracing::warn!(batches = unfinished.len(), dir = %self.dir.display(), "the journal had unfinished batches; they end interrupted");
        for end in unfinished {
            self.append(&Record::End(end))?;
        }
        Ok(())
    }

    /// Adds a record read back from `span` to the index.
    fn index(&mut self, record: &Record, span: Span) {
        match record {
            Record::Begin(b) => {
                if !self.batches.contains_key(&b.batch_id) {
                    self.index_begin(b, span.file);
                }
            }
            Record::Op(o) => self.index_op(o, span),
            Record::End(e) => self.index_end(e),
        }
    }
}

/// Cuts off the last line of the file at `path` if it does not end in `\n`: a write torn by
/// a crash.
fn cut_torn_line(path: &Path) -> Result<(), StoreError> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(io_error)?;
    let len = file.metadata().map_err(io_error)?.len();
    // Read back from the end, a block at a time, to the last `\n`.
    let mut end = len;
    let mut block = [0u8; 4096];
    let keep = loop {
        if end == 0 {
            break 0;
        }
        let start = end.saturating_sub(block.len() as u64);
        let chunk = &mut block[..(end - start) as usize];
        file.seek(SeekFrom::Start(start))
            .and_then(|_| file.read_exact(chunk))
            .map_err(io_error)?;
        if let Some(i) = chunk.iter().rposition(|&b| b == b'\n') {
            break start + i as u64 + 1;
        }
        end = start;
    };
    if keep < len {
        tracing::warn!(path = %path.display(), bytes = len - keep, "a torn last line of the journal cut off");
        file.set_len(keep).map_err(io_error)?;
    }
    Ok(())
}

fn corrupt(path: &Path, offset: u64, what: &str) -> StoreError {
    StoreError::Corrupt(format!("{} at byte {offset}: {what}", path.display()))
}

/// Takes the directory's lock, for the store's life.
fn lock(dir: &Path) -> Result<File, StoreError> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(LOCK))
        .map_err(io_error)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(StoreError::Io(format!(
            "the journal in {} is open elsewhere",
            dir.display()
        ))),
        Err(TryLockError::Error(e)) => Err(io_error(e)),
    }
}

/// The month files in `dir`, in month order. Other entries are left alone.
fn month_files(dir: &Path) -> Result<Vec<(Month, PathBuf)>, StoreError> {
    let mut months = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        let Some(month) = entry.file_name().to_str().and_then(Month::parse) else {
            continue;
        };
        if entry.file_type().map_err(io_error)?.is_file() {
            months.push((month, entry.path()));
        }
    }
    months.sort();
    Ok(months)
}
