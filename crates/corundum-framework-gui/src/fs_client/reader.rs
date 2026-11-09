//! A workspace file as a `Read + Seek`, over `fs.read`.

use std::io::{self, Read, Seek, SeekFrom};

use corundum_framework_fs_api::{Entry, MAX_READ_LEN, codes};

use super::{FsClient, FsError};
use crate::preview::Content;

/// A file of the workspace, read and seeked through `fs.read`: each read that is not in the
/// last chunk fetches the next [`MAX_READ_LEN`] bytes from its position.
///
/// Every read waits for the server, so use it off the UI thread: a preview's worker does
/// (see [`FsClient::content`]).
pub struct FsReader {
    client: FsClient,
    path: String,
    /// The file's length, as of the last answer.
    len: u64,
    /// The position of the next read.
    pos: u64,
    /// The last chunk fetched, and where in the file it starts.
    chunk: (u64, Vec<u8>),
}

impl FsReader {
    /// A reader of `path`, after a `fs.stat` for its length. Waits for the server.
    pub fn open(client: FsClient, path: String) -> io::Result<FsReader> {
        let entry = client.stat(&path).wait().map_err(io_error)?;
        Ok(FsReader {
            client,
            path,
            len: entry.size,
            pos: 0,
            chunk: (0, Vec::new()),
        })
    }
}

/// `error` as an I/O error: `NotFound` for a missing path, else `Other`, with its message.
fn io_error(error: FsError) -> io::Error {
    let message = match &error {
        FsError::Rpc { message, .. } => message.clone(),
        FsError::Transport(message) | FsError::Decode(message) => message.clone(),
        other => format!("{other:?}"),
    };
    let kind = if error.code() == codes::NOT_FOUND {
        io::ErrorKind::NotFound
    } else {
        io::ErrorKind::Other
    };
    io::Error::new(kind, message)
}

impl Read for FsReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let (start, bytes) = &self.chunk;
        let held = self
            .pos
            .checked_sub(*start)
            .filter(|&at| at < bytes.len() as u64);
        if held.is_none() {
            let (bytes, total) = self
                .client
                .read_with_total(&self.path, self.pos, MAX_READ_LEN)
                .wait()
                .map_err(io_error)?;
            self.len = total;
            self.chunk = (self.pos, bytes);
        }
        let (start, bytes) = &self.chunk;
        let Some(at) = self.pos.checked_sub(*start) else {
            return Ok(0);
        };
        let rest = bytes.get(at as usize..).unwrap_or_default();
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for FsReader {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => i128::from(self.len) + i128::from(n),
            SeekFrom::Current(n) => i128::from(self.pos) + i128::from(n),
        };
        self.pos = u64::try_from(target).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "a seek to before the start of the file",
            )
        })?;
        Ok(self.pos)
    }
}

impl FsClient {
    /// The content of the file `path`, whose entry is `entry`, for the previews: keyed by its
    /// path, size and modified time, named by the entry's name, and read through an
    /// [`FsReader`] the preview's worker opens.
    pub fn content(&self, path: &str, entry: &Entry) -> Content {
        let key = format!(
            "{path}\0{}\0{}",
            entry.size,
            entry.modified.map(|m| m.to_string()).unwrap_or_default()
        );
        let (client, path) = (self.clone(), path.to_owned());
        Content::reader(key, entry.name.clone(), move || {
            Ok(Box::new(FsReader::open(client.clone(), path.clone())?)
                as Box<dyn crate::preview::Reader>)
        })
    }
}
