//! [`FileExplorer`]: a folder's contents as a list or a grid of tiles, under a trail of the
//! folders above it — to go into a folder, open a file, pick some, and drag them into
//! another folder or rename one.
//!
//! The explorer touches no file system and no server: the app lists the folder it is
//! asked for and hands the [`Entry`]s in, a page at a time if it likes, and the explorer
//! answers with [`ExplorerEvent`]s — what the person did — for the app to act on. asteria
//! lists its server's folders, sapphire-sync the folders it syncs; a move or a rename
//! goes through whatever the app does them with (asteria's are planned, then approved),
//! and the app hands the folder in again once it is done.
//!
//! ```no_run
//! use corundum_framework_gui::explorer::{Entry, ExplorerEvent, FileExplorer};
//! # fn list(_: &[String]) -> Vec<Entry> { Vec::new() }
//! # fn open(_: &[String], _: &str) {}
//! # fn rename(_: &[String], _: &str, _: &str) {}
//!
//! let mut explorer = FileExplorer::new("music", "Music").renamable(true);
//! # let ctx = egui::Context::default();
//! # let _ = ctx.run_ui(Default::default(), |ui| {
//! // Each frame:
//! for event in explorer.ui(ui) {
//!     match event {
//!         ExplorerEvent::Navigated(location) => {
//!             // Usually started here and handed in when it arrives.
//!             let entries = list(&location);
//!             explorer.set_entries(&location, entries, false);
//!         }
//!         ExplorerEvent::Opened(name) => open(explorer.location(), &name),
//!         ExplorerEvent::Rename { name, to } => rename(explorer.location(), &name, &to),
//!         _ => {}
//!     }
//! }
//! # });
//! ```

use std::collections::BTreeSet;
use std::time::SystemTime;

mod select;
mod view;

use select::Selection;

/// What an [`Entry`] is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EntryKind {
    /// A folder: opening it goes into it.
    Folder,
    /// A file: opening it is the app's.
    File,
}

/// One thing in a folder, as the app lists it. Its name is what the explorer knows it by,
/// so names are unique within a folder, as in a file system.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    /// The size in bytes, where the app knows it.
    pub size: Option<u64>,
    /// When it last changed, where the app knows it.
    pub modified: Option<SystemTime>,
    /// The icon to show; else one for its kind and extension ([`icon_for`]).
    pub icon: Option<egui::ImageSource<'static>>,
    /// A picture of it for the grid, a photo's own say; else the icon.
    pub thumbnail: Option<egui::ImageSource<'static>>,
}

impl Entry {
    /// A folder called `name`.
    pub fn folder(name: impl Into<String>) -> Entry {
        Entry::new(name, EntryKind::Folder)
    }

    /// A file called `name`.
    pub fn file(name: impl Into<String>) -> Entry {
        Entry::new(name, EntryKind::File)
    }

    fn new(name: impl Into<String>, kind: EntryKind) -> Entry {
        Entry {
            name: name.into(),
            kind,
            size: None,
            modified: None,
            icon: None,
            thumbnail: None,
        }
    }

    /// With its size, `bytes`.
    pub fn size(mut self, bytes: u64) -> Entry {
        self.size = Some(bytes);
        self
    }

    /// With the time it last changed.
    pub fn modified(mut self, at: SystemTime) -> Entry {
        self.modified = Some(at);
        self
    }

    /// With an icon of the app's.
    pub fn icon(mut self, icon: egui::ImageSource<'static>) -> Entry {
        self.icon = Some(icon);
        self
    }

    /// With a picture of it for the grid.
    pub fn thumbnail(mut self, thumbnail: egui::ImageSource<'static>) -> Entry {
        self.thumbnail = Some(thumbnail);
        self
    }

    /// Whether it is a folder.
    pub fn is_folder(&self) -> bool {
        self.kind == EntryKind::Folder
    }
}

/// The icon for `entry` when the app gives none: a folder, or a file by its extension — a
/// picture, a song, a video, a document — else a plain file.
pub fn icon_for(entry: &Entry) -> egui::ImageSource<'static> {
    use crate::platform::{AUDIO_EXTENSIONS, IMAGE_EXTENSIONS, VIDEO_EXTENSIONS};
    if entry.is_folder() {
        return crate::icons::folder();
    }
    let extension = entry
        .name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let is = |list: &[&str]| list.contains(&extension.as_str());
    if is(IMAGE_EXTENSIONS) || extension == "svg" {
        crate::icons::picture()
    } else if is(AUDIO_EXTENSIONS) {
        crate::icons::music()
    } else if is(VIDEO_EXTENSIONS) {
        crate::icons::film()
    } else if is(TEXT_EXTENSIONS) {
        crate::icons::file_text()
    } else {
        crate::icons::file()
    }
}

/// Documents and text, shown with a page of text.
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "pdf", "doc", "docx", "odt", "rtf", "csv", "json", "toml", "yaml", "yml", "log",
];

/// What the explorer is sorted by.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SortBy {
    #[default]
    Name,
    Size,
    Modified,
}

/// The order the explorer lists a folder in. Folders come first whatever it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sort {
    pub by: SortBy,
    pub ascending: bool,
}

impl Default for Sort {
    fn default() -> Sort {
        Sort {
            by: SortBy::Name,
            ascending: true,
        }
    }
}

/// How the explorer shows a folder. A window narrower than
/// [`COMPACT_WIDTH`](crate::COMPACT_WIDTH) always shows a list, of rows tall enough for a
/// finger.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum View {
    /// Rows, with the size and the time each last changed in columns.
    #[default]
    List,
    /// Tiles, each with its thumbnail.
    Grid,
}

/// An item of the app's in the menu an entry's right click (or long press) opens.
#[derive(Clone, Debug)]
pub struct MenuItem {
    /// What [`ExplorerEvent::Menu`] says was chosen.
    pub id: String,
    pub label: String,
    pub icon: Option<egui::ImageSource<'static>>,
}

impl MenuItem {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> MenuItem {
        MenuItem {
            id: id.into(),
            label: label.into(),
            icon: None,
        }
    }

    /// With an icon before its label.
    pub fn icon(mut self, icon: egui::ImageSource<'static>) -> MenuItem {
        self.icon = Some(icon);
        self
    }
}

/// What the person did in the explorer, for the app to act on. Names are of entries in the
/// folder shown, [`FileExplorer::location`]; a location is the folders under the root,
/// outermost first.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ExplorerEvent {
    /// The explorer went to this folder — opened, or picked in the trail — and shows that
    /// it is loading: list it and hand it in with [`FileExplorer::set_entries`].
    Navigated(Vec<String>),
    /// This file was opened (a double click, a tap, Enter).
    Opened(String),
    /// The person reached the end of what was handed in, and [`FileExplorer::set_entries`]
    /// said there is more: hand in the next page with [`FileExplorer::append_entries`].
    LoadMore,
    /// The person asked for the folder again (Refresh, or Retry after a failure): list it
    /// and hand it in with [`FileExplorer::set_entries`].
    Refresh,
    /// This item of the app's menu ([`FileExplorer::menu`]) was chosen, for these entries.
    Menu { id: String, names: Vec<String> },
    /// These entries were dragged into the folder `to` ([`FileExplorer::movable`]).
    Move { names: Vec<String>, to: Vec<String> },
    /// The entry `name` was renamed `to` ([`FileExplorer::renamable`]). The explorer shows
    /// the old name until the folder is handed in again.
    Rename { name: String, to: String },
}

/// What a drag carries: the names, of which explorer, from which folder.
#[derive(Clone, Debug)]
struct Dragged {
    explorer: egui::Id,
    from: Vec<String>,
    names: Vec<String>,
}

/// The app's menu items for the entries a menu was opened on, worded in the context's
/// language.
type MenuFn = dyn Fn(&egui::Context, &[&Entry]) -> Vec<MenuItem>;

/// How a time is written in the list.
type TimeFn = dyn Fn(SystemTime) -> String;

/// Where the folder's listing stands.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Status {
    /// Asked for, and not yet handed in (or the next page not yet).
    Loading,
    /// Handed in.
    Ready,
    /// The app could not list it, and said why.
    Failed(String),
}

/// An entry being renamed: its name, and the text in its field.
struct Renaming {
    name: String,
    text: String,
    /// Whether the field has been focused, its name's stem selected.
    started: bool,
}

/// A folder's contents, to browse: a trail of the folders above it with a way up, the
/// entries as a list (name, size, time changed; sorted by a click on a column's heading)
/// or a grid of thumbnails, and a menu on each entry. Hold one for the view and draw it
/// with [`ui`](Self::ui) each frame; act on the events it returns.
///
/// - **Picking**: a click picks one entry; Ctrl (⌘) adds or removes one, Shift a range.
///   With the explorer focused, the arrows move (Shift extends), Home and End go to the
///   ends, Ctrl+A picks all, Esc none, Enter opens, Backspace or Alt+↑ goes up, F2
///   renames.
/// - **Opening**: a double click (a tap, on a phone) goes into a folder or opens a file.
/// - **Menu**: a right click (a long press) opens Open, Rename and the app's own items.
/// - **Moving** ([`movable`](Self::movable)): picked entries dragged onto a folder, or
///   onto a folder in the trail, ask to be moved there. Not on a phone's layout, where a
///   drag scrolls.
/// - **Renaming** ([`renamable`](Self::renamable)): F2 or the menu edit the name in place;
///   Enter (or a click elsewhere) asks for the rename, Esc leaves it.
pub struct FileExplorer {
    id: egui::Id,
    root: String,
    location: Vec<String>,
    entries: Vec<Entry>,
    /// The entries' places in `entries`, in the order shown.
    order: Vec<usize>,
    status: Status,
    /// Whether the app has more of the folder to hand in.
    more: bool,
    sort: Sort,
    view: View,
    selection: Selection,
    renaming: Option<Renaming>,
    movable: bool,
    renamable: bool,
    menu: Option<Box<MenuFn>>,
    time: Box<TimeFn>,
    /// The cursor moved by a key, to scroll it into view.
    reveal: bool,
    events: Vec<ExplorerEvent>,
    /// The entries drawn in the last frame, and where, for the tests.
    #[cfg(test)]
    drawn: std::cell::RefCell<Vec<(String, egui::Rect)>>,
}

impl FileExplorer {
    /// An explorer at its root, named `root` in the trail ("Music", "Sync"), waiting for
    /// the root's entries. `id_salt` tells it apart from other explorers in the window.
    pub fn new(id_salt: impl egui::AsIdSalt, root: impl Into<String>) -> FileExplorer {
        FileExplorer {
            id: egui::Id::unique(("corundum_explorer", id_salt)),
            root: root.into(),
            location: Vec::new(),
            entries: Vec::new(),
            order: Vec::new(),
            status: Status::Loading,
            more: false,
            sort: Sort::default(),
            view: View::default(),
            selection: Selection::default(),
            renaming: None,
            movable: false,
            renamable: false,
            menu: None,
            time: Box::new(format_time),
            reveal: false,
            events: Vec::new(),
            #[cfg(test)]
            drawn: Default::default(),
        }
    }

    /// Let entries be dragged into folders: [`ExplorerEvent::Move`].
    pub fn movable(mut self, movable: bool) -> FileExplorer {
        self.movable = movable;
        self
    }

    /// Let an entry be renamed in place: [`ExplorerEvent::Rename`].
    pub fn renamable(mut self, renamable: bool) -> FileExplorer {
        self.renamable = renamable;
        self
    }

    /// Add the app's items to an entry's menu, after Open and Rename: `items` is given the
    /// context (to word them with [`tr!`](crate::tr)) and the entries the menu is for
    /// (those picked), and says what to offer for them — say, Add to queue for songs
    /// alone. A choice is an [`ExplorerEvent::Menu`].
    pub fn menu(
        mut self,
        items: impl Fn(&egui::Context, &[&Entry]) -> Vec<MenuItem> + 'static,
    ) -> FileExplorer {
        self.menu = Some(Box::new(items));
        self
    }

    /// Write the times entries changed with `format`. By default they are written as
    /// `YYYY-MM-DD HH:MM` in UTC, as the explorer knows no time zone; an app that does
    /// writes them in the person's own.
    pub fn time_format(mut self, format: impl Fn(SystemTime) -> String + 'static) -> FileExplorer {
        self.time = Box::new(format);
        self
    }

    /// The folder shown: the folders under the root, outermost first.
    pub fn location(&self) -> &[String] {
        &self.location
    }

    /// The entries handed in, in the order the app gave them.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entries picked, in the order shown.
    pub fn selected(&self) -> Vec<&Entry> {
        self.order
            .iter()
            .map(|&i| &self.entries[i])
            .filter(|e| self.selection.contains(&e.name))
            .collect()
    }

    /// Whether the folder shown is still being listed.
    pub fn is_loading(&self) -> bool {
        self.status == Status::Loading
    }

    pub fn sort(&self) -> Sort {
        self.sort
    }

    /// List the folder in `sort`'s order.
    pub fn set_sort(&mut self, sort: Sort) {
        self.sort = sort;
        self.reorder();
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn set_view(&mut self, view: View) {
        self.view = view;
    }

    /// Go to `location`, as the person would, but without an event: the app asked for it,
    /// so it lists the folder itself. The explorer shows that it is loading meanwhile.
    pub fn go_to(&mut self, location: Vec<String>) {
        self.location = location;
        self.entries.clear();
        self.order.clear();
        self.selection.clear();
        self.renaming = None;
        self.status = Status::Loading;
        self.more = false;
    }

    /// Hand in the entries of the folder at `location`, replacing those shown; `more`
    /// when there is a next page ([`ExplorerEvent::LoadMore`]). What is picked stays
    /// picked where it still is. Ignored, and `false`, when the explorer has gone to
    /// another folder since — a listing that came back late.
    pub fn set_entries(&mut self, location: &[String], entries: Vec<Entry>, more: bool) -> bool {
        if location != self.location.as_slice() {
            return false;
        }
        self.entries = entries;
        self.status = Status::Ready;
        self.more = more;
        if let Some(r) = &self.renaming
            && !self.entries.iter().any(|e| e.name == r.name)
        {
            self.renaming = None;
        }
        self.reorder();
        true
    }

    /// Hand in the next page of the folder at `location`, after those shown. Ignored, and
    /// `false`, when the explorer has gone to another folder since.
    pub fn append_entries(&mut self, location: &[String], entries: Vec<Entry>, more: bool) -> bool {
        if location != self.location.as_slice() {
            return false;
        }
        self.entries.extend(entries);
        self.status = Status::Ready;
        self.more = more;
        self.reorder();
        true
    }

    /// Say that the folder at `location` could not be listed, and why; the explorer offers
    /// to try again ([`ExplorerEvent::Refresh`]). Ignored, and `false`, when the explorer
    /// has gone to another folder since.
    pub fn set_failed(&mut self, location: &[String], error: impl Into<String>) -> bool {
        if location != self.location.as_slice() {
            return false;
        }
        self.status = Status::Failed(error.into());
        self.more = false;
        true
    }

    /// Sort the entries again, and let go of picked names no longer among them.
    fn reorder(&mut self) {
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        let sort = self.sort;
        let entries = &self.entries;
        order.sort_by(|&a, &b| compare(&entries[a], &entries[b], sort));
        self.order = order;
        let shown = self.shown_names();
        self.selection.retain(&shown);
    }

    /// The names shown, in order.
    fn shown_names(&self) -> Vec<String> {
        self.order
            .iter()
            .map(|&i| self.entries[i].name.clone())
            .collect()
    }

    /// The entry shown at `at`.
    fn shown(&self, at: usize) -> &Entry {
        &self.entries[self.order[at]]
    }

    /// Go into `location`, as the person asked.
    fn navigate(&mut self, location: Vec<String>) {
        self.go_to(location.clone());
        self.events.push(ExplorerEvent::Navigated(location));
    }

    /// Open the entry shown at `at`: into a folder, or the app opens a file.
    fn open(&mut self, at: usize) {
        let entry = self.shown(at);
        if entry.is_folder() {
            let mut location = self.location.clone();
            location.push(entry.name.clone());
            self.navigate(location);
        } else {
            let name = entry.name.clone();
            self.events.push(ExplorerEvent::Opened(name));
        }
    }

    /// Go to the folder above, unless at the root.
    fn up(&mut self) {
        if let Some((_, above)) = self.location.split_last() {
            self.navigate(above.to_vec());
        }
    }

    fn refresh(&mut self) {
        self.status = Status::Loading;
        self.events.push(ExplorerEvent::Refresh);
    }

    /// Begin renaming the entry called `name`.
    fn start_rename(&mut self, name: String) {
        if self.renamable {
            self.renaming = Some(Renaming {
                text: name.clone(),
                name,
                started: false,
            });
        }
    }

    /// Ask to rename as the field says, if it says something new and allowed; either way
    /// the field closes, unless `keep_if_invalid` and what it says cannot be a name.
    fn finish_rename(&mut self, keep_if_invalid: bool) {
        let Some(renaming) = self.renaming.take() else {
            return;
        };
        let to = renaming.text.trim().to_owned();
        if to == renaming.name {
            return;
        }
        if self.name_allowed(&to) {
            self.events.push(ExplorerEvent::Rename {
                name: renaming.name,
                to,
            });
        } else if keep_if_invalid {
            self.renaming = Some(renaming);
        }
    }

    /// Whether an entry could be renamed `name`: not empty, no path separator, not `.` or
    /// `..`, and no other entry's.
    fn name_allowed(&self, name: &str) -> bool {
        !name.is_empty()
            && !name.contains(['/', '\\'])
            && name != "."
            && name != ".."
            && !self.entries.iter().any(|e| e.name == name)
    }
}

/// `a` against `b` in `sort`'s order: folders first, then by the column, then by name.
fn compare(a: &Entry, b: &Entry, sort: Sort) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let folders = b.is_folder().cmp(&a.is_folder());
    if folders != Ordering::Equal {
        return folders;
    }
    let by = match sort.by {
        SortBy::Name => Ordering::Equal,
        SortBy::Size => a.size.cmp(&b.size),
        SortBy::Modified => a.modified.cmp(&b.modified),
    };
    let order = by.then_with(|| natural(&a.name, &b.name));
    if sort.ascending {
        order
    } else {
        order.reverse()
    }
}

/// `a` against `b` as a person reads names: without regard to case, and with runs of
/// digits as numbers, so that `track 2` comes before `track 10`.
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut a_chars, mut b_chars) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |chars: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = chars.next_if(char::is_ascii_digit) {
                        digits.push(c);
                    }
                    digits
                };
                let (x, y) = (take(&mut a_chars), take(&mut b_chars));
                let (x_trim, y_trim) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = x_trim
                    .len()
                    .cmp(&y_trim.len())
                    .then_with(|| x_trim.cmp(y_trim));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(_), Some(_)) => {
                let x = a_chars.next().into_iter().flat_map(char::to_lowercase);
                let y = b_chars.next().into_iter().flat_map(char::to_lowercase);
                let order = x.cmp(y);
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// `bytes` for a person: `512 B`, `1.5 KB`, `23 MB`, in thousands, as the desktops' file
/// managers write them.
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    if bytes < 1000 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1000.0;
    let mut unit = 0;
    while value >= 999.95 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// `at` as `YYYY-MM-DD HH:MM`, in UTC: the explorer's default ([`FileExplorer::time_format`]).
pub fn format_time(at: SystemTime) -> String {
    let seconds = match at.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let (days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        of_day / 3600,
        of_day % 3600 / 60
    )
}

/// The date `days` after 1970-01-01, in the proleptic Gregorian calendar (Howard
/// Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The names of the entries picked, in order, unless `name` is not among them: then
/// `name` alone, as a drag or a menu on an entry not picked is for that one.
fn names_for(selected: &BTreeSet<String>, shown: &[String], name: &str) -> Vec<String> {
    if selected.contains(name) {
        shown
            .iter()
            .filter(|n| selected.contains(*n))
            .cloned()
            .collect()
    } else {
        vec![name.to_owned()]
    }
}

#[cfg(test)]
mod tests;
