//! `JsonlStore`: the journal in a `YYYY-MM.jsonl` file per month.

mod month;
mod open;

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use month::Month;

use crate::{
    BatchBegin, BatchEnd, BatchId, BatchRecords, BatchSummary, JournalStore, OpRecord, Outcome,
    Record, StoreError,
};

/// Where a record's line is: its file, its first byte, and its length with the `\n`.
#[derive(Debug, Clone, Copy)]
struct Span {
    file: usize,
    offset: u64,
    len: u32,
}

/// A batch in the index: its summary, its file, and where its op lines are.
#[derive(Debug)]
struct Indexed {
    summary: BatchSummary,
    file: usize,
    ops: Vec<Span>,
}

/// A month's file, and its length as this store wrote or read it.
#[derive(Debug)]
struct MonthFile {
    month: Month,
    path: PathBuf,
    len: u64,
    /// A failed write left bytes past `len` that could not be cut off yet.
    dirty: bool,
}

/// Opens a month file to write at chosen offsets. Not in append mode: on Windows an
/// append-only handle cannot cut the file back.
fn open_to_write(path: &Path, create: bool) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(create)
        .truncate(false)
        .open(path)
}

/// A store that keeps the journal in a directory, one `YYYY-MM.jsonl` file per month
/// (UTC), with one [`Record`] per line, and its index in memory.
///
/// For an app that wants a long history to audit. The directory is the server instance's
/// state directory, never the workspace. All of a batch's records go into the file of the
/// month it began in, or into the newest file if the clock has gone back since, so the
/// files read in month order give the batches in the order they began.
///
/// - **On open**, the files are read and the index is built: every batch's summary, and
///   where its operations' lines are; [`JournalStore::batch`] reads those lines back. A
///   torn last line, left by a crash in the middle of a write, is cut off; a line that does
///   not parse is skipped, with a warning. A batch without an end gets one, with
///   `interrupted: true`; its finished operations stay undoable within the undo window.
/// - **Nothing is ever deleted.** To keep less, the app removes old month files while the
///   store is closed; the operations and ends of a batch whose month is gone are then
///   ignored. The index grows by about a summary per batch.
/// - **One store per directory**: the store holds a lock on `journal.lock`, and a second
///   [`JsonlStore::open`] of the same directory fails.
/// - A record is written in one piece; the file is synced after each batch's end.
#[derive(Debug)]
pub struct JsonlStore {
    dir: PathBuf,
    /// Held for the store's life.
    _lock: File,
    /// In month order.
    files: Vec<MonthFile>,
    /// The newest of `files`, open to append.
    newest: Option<File>,
    /// The batches, in the order they began.
    order: Vec<BatchId>,
    /// Each batch's place in `order`.
    position: HashMap<BatchId, usize>,
    batches: HashMap<BatchId, Indexed>,
    #[cfg(test)]
    faults: Faults,
}

/// Failures a test sets up for the next write; each is taken once.
#[cfg(test)]
#[derive(Debug, Default)]
struct Faults {
    /// Write half the line, then fail.
    torn_write: bool,
    /// Fail to cut a failed write back.
    failing_cut: bool,
}

#[cfg(test)]
mod tests;

fn io_error(e: io::Error) -> StoreError {
    StoreError::Io(e.to_string())
}

impl JsonlStore {
    /// The directory the store keeps its files in.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The file a batch begun at `at` goes into: its month's, but never older than the
    /// newest file. The file is created if it is new.
    fn file_for(&mut self, at: i64) -> Result<usize, StoreError> {
        let month = Month::of(at);
        if let Some(last) = self.files.last()
            && month <= last.month
        {
            return Ok(self.files.len() - 1);
        }
        let path = self.dir.join(month.file_name());
        let file = open_to_write(&path, true).map_err(io_error)?;
        let len = file.metadata().map_err(io_error)?.len();
        self.files.push(MonthFile {
            month,
            path,
            len,
            dirty: false,
        });
        self.newest = Some(file);
        Ok(self.files.len() - 1)
    }

    /// Writes `record` as one line at the end of file `file`, and gives where it is. On a
    /// failed write the file is cut back to its length before it; if that fails too, the
    /// next write cuts first, and writes nothing if it cannot. An end is synced to the disk.
    fn write_line(&mut self, file: usize, record: &Record) -> Result<Span, StoreError> {
        let mut line = serde_json::to_vec(record).map_err(|e| StoreError::Io(e.to_string()))?;
        line.push(b'\n');
        let len = u32::try_from(line.len())
            .map_err(|_| StoreError::Io(format!("a record of {} bytes", line.len())))?;
        let offset = self.files[file].len;
        let mut opened;
        let handle = if file + 1 == self.files.len()
            && let Some(newest) = self.newest.as_mut()
        {
            newest
        } else {
            opened = open_to_write(&self.files[file].path, false).map_err(io_error)?;
            &mut opened
        };
        if self.files[file].dirty {
            handle.set_len(offset).map_err(io_error)?;
            self.files[file].dirty = false;
        }
        #[cfg(test)]
        let written = if std::mem::take(&mut self.faults.torn_write) {
            handle
                .seek(SeekFrom::Start(offset))
                .and_then(|_| handle.write_all(&line[..line.len() / 2]))
                .and_then(|()| Err(io::Error::other("a torn write (test)")))
        } else {
            Ok(())
        };
        #[cfg(not(test))]
        let written = Ok(());
        let written = handle
            .seek(SeekFrom::Start(offset))
            .and(written)
            .and_then(|()| handle.write_all(&line))
            .and_then(|()| {
                if matches!(record, Record::End(_)) {
                    handle.sync_data()
                } else {
                    Ok(())
                }
            });
        if let Err(e) = written {
            #[cfg(test)]
            let cut = if std::mem::take(&mut self.faults.failing_cut) {
                Err(io::Error::other("a failed cut (test)"))
            } else {
                handle.set_len(offset)
            };
            #[cfg(not(test))]
            let cut = handle.set_len(offset);
            if let Err(cut) = cut {
                tracing::error!(path = %self.files[file].path.display(), error = %cut, "a failed write could not be cut back; the next write tries again");
                self.files[file].dirty = true;
            }
            return Err(io_error(e));
        }
        self.files[file].len = offset + u64::from(len);
        Ok(Span { file, offset, len })
    }

    /// Adds a batch to the index; its id is new.
    fn index_begin(&mut self, begin: &BatchBegin, file: usize) {
        let id = begin.batch_id;
        if let Some(target) = begin.undo_of.or(begin.redo_of)
            && let Some(t) = self.batches.get_mut(&target)
        {
            t.summary.undone_by = Some(id);
        }
        self.position.insert(id, self.order.len());
        self.order.push(id);
        self.batches.insert(
            id,
            Indexed {
                summary: BatchSummary {
                    begin: begin.clone(),
                    done: 0,
                    skipped: 0,
                    complete: false,
                    interrupted: false,
                    undone_by: None,
                },
                file,
                ops: Vec::new(),
            },
        );
    }

    /// Adds an operation of a batch in the index.
    fn index_op(&mut self, op: &OpRecord, span: Span) {
        if let Some(b) = self.batches.get_mut(&op.batch_id) {
            match op.outcome {
                Outcome::Done(_) => b.summary.done += 1,
                Outcome::Skipped(_) => b.summary.skipped += 1,
            }
            b.ops.push(span);
        }
    }

    /// Ends a batch in the index.
    fn index_end(&mut self, end: &BatchEnd) {
        if let Some(b) = self.batches.get_mut(&end.batch_id) {
            b.summary.complete = true;
            b.summary.interrupted = end.interrupted;
        }
    }

    /// The file of a batch in the index.
    fn file_of(&self, id: BatchId) -> Option<usize> {
        self.batches.get(&id).map(|b| b.file)
    }

    /// Reads the operation at `span`.
    fn read_op(&self, file: &mut File, span: Span) -> Result<OpRecord, StoreError> {
        let path = &self.files[span.file].path;
        let mut line = vec![0; span.len as usize];
        file.seek(SeekFrom::Start(span.offset))
            .and_then(|_| file.read_exact(&mut line))
            .map_err(io_error)?;
        match serde_json::from_slice(&line) {
            Ok(Record::Op(op)) => Ok(op),
            _ => Err(StoreError::Corrupt(format!(
                "no operation at byte {} of {}",
                span.offset,
                path.display()
            ))),
        }
    }
}

impl JournalStore for JsonlStore {
    fn append(&mut self, record: &Record) -> Result<(), StoreError> {
        match record {
            Record::Begin(b) => {
                if self.batches.contains_key(&b.batch_id) {
                    tracing::debug!(batch = ?b.batch_id, "duplicate begin dropped");
                    return Ok(());
                }
                let file = self.file_for(b.at)?;
                self.write_line(file, record)?;
                self.index_begin(b, file);
            }
            Record::Op(o) => {
                let Some(file) = self.file_of(o.batch_id) else {
                    tracing::debug!(batch = ?o.batch_id, "op for an unknown batch dropped");
                    return Ok(());
                };
                let span = self.write_line(file, record)?;
                self.index_op(o, span);
            }
            Record::End(e) => {
                let Some(file) = self.file_of(e.batch_id) else {
                    tracing::debug!(batch = ?e.batch_id, "end for an unknown batch dropped");
                    return Ok(());
                };
                self.write_line(file, record)?;
                self.index_end(e);
            }
        }
        Ok(())
    }

    fn batches_rev(
        &self,
        cursor: Option<BatchId>,
        limit: usize,
    ) -> Result<Vec<BatchSummary>, StoreError> {
        let end = match cursor {
            None => self.order.len(),
            Some(c) => match self.position.get(&c) {
                Some(&i) => i,
                None => return Ok(Vec::new()),
            },
        };
        Ok(self.order[..end]
            .iter()
            .rev()
            .take(limit)
            .filter_map(|id| self.batches.get(id))
            .map(|b| b.summary.clone())
            .collect())
    }

    fn batch(&self, id: BatchId) -> Result<Option<BatchRecords>, StoreError> {
        let Some(b) = self.batches.get(&id) else {
            return Ok(None);
        };
        let mut ops = Vec::with_capacity(b.ops.len());
        let mut open: Option<(usize, File)> = None;
        for &span in &b.ops {
            let file = match &mut open {
                Some((i, f)) if *i == span.file => f,
                _ => {
                    let f = File::open(&self.files[span.file].path).map_err(io_error)?;
                    &mut open.insert((span.file, f)).1
                }
            };
            ops.push(self.read_op(file, span)?);
        }
        Ok(Some(BatchRecords {
            summary: b.summary.clone(),
            ops,
        }))
    }

    fn summary(&self, id: BatchId) -> Result<Option<BatchSummary>, StoreError> {
        Ok(self.batches.get(&id).map(|b| b.summary.clone()))
    }
}
