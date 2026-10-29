use std::time::{Duration, SystemTime};

use super::*;
use crate::icons;

fn music() -> Vec<Entry> {
    vec![
        Entry::file("track 10.mp3").size(3_000_000),
        Entry::folder("Live"),
        Entry::file("track 2.mp3").size(5_000_000),
        Entry::file("Cover.png").size(200_000),
        Entry::folder("albums"),
    ]
}

/// An explorer showing `entries` at its root.
fn explorer(entries: Vec<Entry>) -> FileExplorer {
    let mut explorer = FileExplorer::new("test", "Music")
        .movable(true)
        .renamable(true);
    assert!(explorer.set_entries(&[], entries, false));
    explorer
}

fn shown(explorer: &FileExplorer) -> Vec<&str> {
    explorer
        .order
        .iter()
        .map(|&i| explorer.entries[i].name.as_str())
        .collect()
}

/// Frames of `explorer` filling a `width` × 400 screen, each with its events; what it
/// said, in all.
struct Screen {
    ctx: egui::Context,
    width: f32,
    said: Vec<ExplorerEvent>,
}

impl Screen {
    fn new(width: f32) -> Screen {
        Screen {
            ctx: egui::Context::default(),
            width,
            said: Vec::new(),
        }
    }

    fn frame(&mut self, explorer: &mut FileExplorer, events: Vec<egui::Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(self.width, 400.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = self.ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| self.said.extend(explorer.ui(ui)));
        });
        output.textures_delta.clear();
    }

    /// Two frames with no input: laid out, then drawn.
    fn settle(&mut self, explorer: &mut FileExplorer) {
        self.frame(explorer, vec![]);
        self.frame(explorer, vec![]);
    }

    fn click(&mut self, explorer: &mut FileExplorer, at: egui::Pos2, modifiers: egui::Modifiers) {
        self.press(explorer, at, egui::PointerButton::Primary, modifiers);
    }

    fn press(
        &mut self,
        explorer: &mut FileExplorer,
        at: egui::Pos2,
        button: egui::PointerButton,
        modifiers: egui::Modifiers,
    ) {
        let event = |pressed| egui::Event::PointerButton {
            pos: at,
            button,
            pressed,
            modifiers,
        };
        self.frame(
            explorer,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::PointerMoved(at),
            ],
        );
        self.frame(explorer, vec![event(true)]);
        self.frame(explorer, vec![event(false)]);
        self.frame(
            explorer,
            vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
        );
    }

    /// A click on the entry `name`, where it was drawn.
    fn click_on(&mut self, explorer: &mut FileExplorer, name: &str, modifiers: egui::Modifiers) {
        let pos = at(explorer, name);
        self.click(explorer, pos, modifiers);
    }

    fn key(&mut self, explorer: &mut FileExplorer, key: egui::Key, modifiers: egui::Modifiers) {
        let event = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        };
        self.frame(
            explorer,
            vec![
                egui::Event::ModifiersChanged(modifiers),
                event(true),
                event(false),
            ],
        );
        self.frame(
            explorer,
            vec![egui::Event::ModifiersChanged(egui::Modifiers::NONE)],
        );
    }
}

/// Where the entry `name` was drawn in the last frame.
fn at(explorer: &FileExplorer, name: &str) -> egui::Pos2 {
    explorer
        .drawn
        .borrow()
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("{name} was not drawn"))
        .1
        .center()
}

fn picked(explorer: &FileExplorer) -> Vec<&str> {
    explorer
        .selected()
        .iter()
        .map(|e| e.name.as_str())
        .collect()
}

#[test]
fn folders_come_first_then_names_as_read() {
    let mut explorer = explorer(music());
    assert_eq!(
        shown(&explorer),
        ["albums", "Live", "Cover.png", "track 2.mp3", "track 10.mp3"]
    );
    explorer.set_sort(Sort {
        by: SortBy::Size,
        ascending: false,
    });
    assert_eq!(
        shown(&explorer),
        ["Live", "albums", "track 2.mp3", "track 10.mp3", "Cover.png"]
    );
}

#[test]
fn natural_order_reads_numbers_and_ignores_case() {
    use std::cmp::Ordering::*;
    assert_eq!(natural("a2", "a10"), Less);
    assert_eq!(natural("a010", "a9"), Greater);
    assert_eq!(natural("B", "a"), Greater);
    assert_eq!(natural("abc", "ABC"), "abc".cmp("ABC"));
    assert_eq!(natural("a", "a1"), Less);
}

#[test]
fn sizes_and_times_are_written_for_a_person() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(999), "999 B");
    assert_eq!(format_size(1_500), "1.5 KB");
    assert_eq!(format_size(23_400_000), "23 MB");
    assert_eq!(format_size(999_960), "1.0 MB");
    assert_eq!(format_time(SystemTime::UNIX_EPOCH), "1970-01-01 00:00");
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    assert_eq!(format_time(at), "2023-11-14 22:13");
    let before = SystemTime::UNIX_EPOCH - Duration::from_secs(86_400);
    assert_eq!(format_time(before), "1969-12-31 00:00");
}

#[test]
fn icons_follow_the_kind_and_extension() {
    assert_eq!(
        icon_for(&Entry::folder("a.png")).uri(),
        icons::folder().uri()
    );
    assert_eq!(
        icon_for(&Entry::file("a.PNG")).uri(),
        icons::picture().uri()
    );
    assert_eq!(icon_for(&Entry::file("a.flac")).uri(), icons::music().uri());
    assert_eq!(icon_for(&Entry::file("a.mkv")).uri(), icons::film().uri());
    assert_eq!(
        icon_for(&Entry::file("notes.md")).uri(),
        icons::file_text().uri()
    );
    assert_eq!(
        icon_for(&Entry::file("Makefile")).uri(),
        icons::file().uri()
    );
}

#[test]
fn a_late_listing_is_ignored() {
    let mut explorer = explorer(music());
    explorer.go_to(vec!["Live".into()]);
    assert!(explorer.is_loading());
    assert!(!explorer.set_entries(&[], music(), false));
    assert!(explorer.entries().is_empty());
    assert!(explorer.set_entries(&["Live".into()], vec![Entry::file("a")], true));
    assert!(explorer.append_entries(&["Live".into()], vec![Entry::file("b")], false));
    assert_eq!(shown(&explorer), ["a", "b"]);
    assert!(!explorer.set_failed(&[], "gone"));
}

#[test]
fn what_is_picked_stays_picked_across_a_new_listing() {
    let mut explorer = explorer(music());
    let names = explorer.shown_names();
    explorer.selection.click(&names, 2, false, false);
    explorer.selection.click(&names, 3, true, false);
    let mut fewer = music();
    fewer.retain(|e| e.name != "Cover.png");
    explorer.set_entries(&[], fewer, false);
    assert_eq!(picked(&explorer), ["track 2.mp3"]);
}

#[test]
fn a_rename_asks_for_an_allowed_new_name() {
    let mut explorer = explorer(music());
    for (to, asked) in [
        ("Live", false),
        ("a/b", false),
        ("..", false),
        ("  ", false),
        ("Cover.png", false),
        (" Front.png ", true),
    ] {
        explorer.start_rename("Cover.png".into());
        explorer.renaming.as_mut().unwrap().text = to.into();
        explorer.finish_rename(false);
        assert!(explorer.renaming.is_none(), "{to}");
        let events = std::mem::take(&mut explorer.events);
        assert_eq!(!events.is_empty(), asked, "{to}");
    }
    explorer.start_rename("Cover.png".into());
    explorer.renaming.as_mut().unwrap().text = "Live".into();
    explorer.finish_rename(true);
    assert!(
        explorer.renaming.is_some(),
        "an Enter on a taken name keeps the field"
    );
}

#[test]
fn clicks_pick_and_a_double_click_goes_in() {
    let (mut screen, mut explorer) = (Screen::new(900.0), explorer(music()));
    screen.settle(&mut explorer);
    let none = egui::Modifiers::NONE;
    screen.click_on(&mut explorer, "Cover.png", none);
    screen.click_on(&mut explorer, "track 10.mp3", egui::Modifiers::SHIFT);
    assert_eq!(
        picked(&explorer),
        ["Cover.png", "track 2.mp3", "track 10.mp3"]
    );
    screen.click_on(&mut explorer, "track 2.mp3", egui::Modifiers::COMMAND);
    assert_eq!(picked(&explorer), ["Cover.png", "track 10.mp3"]);
    assert!(screen.said.is_empty());

    let live = at(&explorer, "Live");
    let event = |pressed| egui::Event::PointerButton {
        pos: live,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: none,
    };
    screen.frame(&mut explorer, vec![egui::Event::PointerMoved(live)]);
    // Half a second on (a frame is 1/60 s), so the clicks before are not counted with it.
    for _ in 0..30 {
        screen.frame(&mut explorer, vec![]);
    }
    for pressed in [true, false, true, false] {
        screen.frame(&mut explorer, vec![event(pressed)]);
    }
    assert_eq!(screen.said, [ExplorerEvent::Navigated(vec!["Live".into()])]);
    assert_eq!(explorer.location(), ["Live"]);
    assert!(explorer.is_loading());
    assert!(picked(&explorer).is_empty());
}

#[test]
fn the_keys_move_open_and_go_up() {
    let (mut screen, mut explorer) = (Screen::new(900.0), explorer(music()));
    explorer.go_to(vec!["Music".into()]);
    explorer.set_entries(&["Music".into()], music(), false);
    screen.settle(&mut explorer);
    let none = egui::Modifiers::NONE;
    screen.click_on(&mut explorer, "albums", none);
    screen.key(&mut explorer, egui::Key::ArrowDown, none);
    screen.key(&mut explorer, egui::Key::ArrowDown, egui::Modifiers::SHIFT);
    assert_eq!(picked(&explorer), ["Live", "Cover.png"]);
    screen.key(&mut explorer, egui::Key::End, none);
    assert_eq!(picked(&explorer), ["track 10.mp3"]);
    screen.key(&mut explorer, egui::Key::Enter, none);
    assert_eq!(screen.said, [ExplorerEvent::Opened("track 10.mp3".into())]);
    screen.key(&mut explorer, egui::Key::A, egui::Modifiers::COMMAND);
    assert_eq!(picked(&explorer).len(), 5);
    screen.key(&mut explorer, egui::Key::Escape, none);
    assert!(picked(&explorer).is_empty());
    screen.key(&mut explorer, egui::Key::Backspace, none);
    assert_eq!(screen.said[1], ExplorerEvent::Navigated(vec![]));
}

#[test]
fn f2_renames_in_place_keeping_the_extension() {
    let (mut screen, mut explorer) = (Screen::new(900.0), explorer(music()));
    screen.settle(&mut explorer);
    let none = egui::Modifiers::NONE;
    screen.click_on(&mut explorer, "track 2.mp3", none);
    screen.key(&mut explorer, egui::Key::F2, none);
    // The field opens, and is focused with the stem picked, over a frame or two.
    screen.settle(&mut explorer);
    screen.frame(&mut explorer, vec![egui::Event::Text("Intro".into())]);
    screen.key(&mut explorer, egui::Key::Enter, none);
    assert_eq!(
        screen.said,
        [ExplorerEvent::Rename {
            name: "track 2.mp3".into(),
            to: "Intro.mp3".into(),
        }]
    );
    assert!(explorer.renaming.is_none());
}

#[test]
fn picked_entries_dragged_onto_a_folder_ask_to_move() {
    let (mut screen, mut explorer) = (Screen::new(900.0), explorer(music()));
    screen.settle(&mut explorer);
    let none = egui::Modifiers::NONE;
    screen.click_on(&mut explorer, "Cover.png", none);
    screen.click_on(&mut explorer, "track 2.mp3", egui::Modifiers::COMMAND);
    let (from, to) = (at(&explorer, "Cover.png"), at(&explorer, "albums"));
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: none,
    };
    screen.frame(&mut explorer, vec![egui::Event::PointerMoved(from)]);
    screen.frame(&mut explorer, vec![button(from, true)]);
    // Far enough, in steps, for egui to take it as a drag.
    for step in 1..=5 {
        let pos = from + (to - from) * (step as f32 / 5.0);
        screen.frame(&mut explorer, vec![egui::Event::PointerMoved(pos)]);
    }
    screen.frame(&mut explorer, vec![button(to, false)]);
    screen.frame(&mut explorer, vec![]);
    assert_eq!(
        screen.said,
        [ExplorerEvent::Move {
            names: vec!["Cover.png".into(), "track 2.mp3".into()],
            to: vec!["albums".into()],
        }]
    );
}

#[test]
fn a_folder_does_not_take_itself() {
    let (mut screen, mut explorer) = (Screen::new(900.0), explorer(music()));
    screen.settle(&mut explorer);
    let (from, to) = (
        at(&explorer, "Live"),
        at(&explorer, "Live") + egui::vec2(40.0, 0.0),
    );
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    screen.frame(
        &mut explorer,
        vec![egui::Event::PointerMoved(from), button(from, true)],
    );
    for step in 1..=5 {
        let pos = from + (to - from) * (step as f32 / 5.0);
        screen.frame(&mut explorer, vec![egui::Event::PointerMoved(pos)]);
    }
    screen.frame(&mut explorer, vec![button(to, false)]);
    screen.frame(&mut explorer, vec![]);
    assert!(screen.said.is_empty(), "{:?}", screen.said);
}

#[test]
fn not_movable_nothing_moves() {
    let (mut screen, mut explorer) = (Screen::new(900.0), explorer(music()).movable(false));
    screen.settle(&mut explorer);
    let (from, to) = (at(&explorer, "Cover.png"), at(&explorer, "albums"));
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    screen.frame(
        &mut explorer,
        vec![egui::Event::PointerMoved(from), button(from, true)],
    );
    for step in 1..=5 {
        let pos = from + (to - from) * (step as f32 / 5.0);
        screen.frame(&mut explorer, vec![egui::Event::PointerMoved(pos)]);
    }
    screen.frame(&mut explorer, vec![button(to, false)]);
    assert!(
        !screen
            .said
            .iter()
            .any(|e| matches!(e, ExplorerEvent::Move { .. })),
        "{:?}",
        screen.said
    );
}

#[test]
fn on_a_phone_a_tap_opens_and_rows_are_tall() {
    let (mut screen, mut explorer) = (Screen::new(400.0), explorer(music()));
    explorer.set_view(View::Grid);
    screen.settle(&mut explorer);
    let rows = explorer.drawn.borrow().clone();
    assert_eq!(rows.len(), 5, "a list, whatever the view");
    assert!(rows.iter().all(|(_, r)| r.height() >= 44.0), "{rows:?}");
    screen.click_on(&mut explorer, "track 2.mp3", egui::Modifiers::NONE);
    assert_eq!(screen.said, [ExplorerEvent::Opened("track 2.mp3".into())]);
}

#[test]
fn the_grid_lays_tiles_across() {
    let (mut screen, mut explorer) = (Screen::new(900.0), explorer(music()));
    explorer.set_view(View::Grid);
    screen.settle(&mut explorer);
    let tiles = explorer.drawn.borrow().clone();
    assert_eq!(tiles.len(), 5);
    let top = tiles[0].1.top();
    assert!(
        tiles.iter().all(|(_, r)| r.top() == top),
        "one row: {tiles:?}"
    );
    // Right steps across in a grid.
    screen.click_on(&mut explorer, "albums", egui::Modifiers::NONE);
    screen.key(&mut explorer, egui::Key::ArrowRight, egui::Modifiers::NONE);
    assert_eq!(picked(&explorer), ["Live"]);
}

#[test]
fn a_heading_sorts_and_again_turns_round() {
    let (mut screen, mut explorer) = (Screen::new(900.0), explorer(music()));
    screen.settle(&mut explorer);
    // The headings sit just above the first row, in its columns.
    let first = explorer.drawn.borrow()[0].1;
    let size = view::columns(first)[2].center() - egui::vec2(0.0, first.height());
    screen.click(&mut explorer, size, egui::Modifiers::NONE);
    assert_eq!(
        explorer.sort(),
        Sort {
            by: SortBy::Size,
            ascending: true
        }
    );
    screen.click(&mut explorer, size, egui::Modifiers::NONE);
    assert_eq!(
        explorer.sort(),
        Sort {
            by: SortBy::Size,
            ascending: false
        }
    );
}

#[test]
fn more_is_asked_for_once_the_end_shows() {
    let (mut screen, mut explorer) = (Screen::new(900.0), FileExplorer::new("more", "Root"));
    explorer.set_entries(&[], vec![Entry::file("a")], true);
    screen.settle(&mut explorer);
    assert_eq!(screen.said, [ExplorerEvent::LoadMore]);
    assert!(explorer.is_loading());
    screen.settle(&mut explorer);
    assert_eq!(screen.said.len(), 1, "asked once while loading");
    explorer.append_entries(&[], vec![Entry::file("b")], false);
    screen.settle(&mut explorer);
    assert_eq!(screen.said.len(), 1);
}

#[test]
fn the_app_s_menu_items_are_for_what_is_picked() {
    let explorer = explorer(music()).menu(|_, entries| {
        if entries.iter().all(|e| e.name.ends_with(".mp3")) {
            vec![MenuItem::new("queue", "Add to queue")]
        } else {
            Vec::new()
        }
    });
    let (menu, ctx) = (explorer.menu.as_ref().unwrap(), egui::Context::default());
    let songs: Vec<_> = explorer
        .entries
        .iter()
        .filter(|e| e.name.ends_with(".mp3"))
        .collect();
    assert_eq!(menu(&ctx, &songs).len(), 1);
    assert!(menu(&ctx, &[&explorer.entries[1]]).is_empty());

    let shown = explorer.shown_names();
    let picked = BTreeSet::from(["track 2.mp3".to_owned(), "track 10.mp3".to_owned()]);
    assert_eq!(
        names_for(&picked, &shown, "track 10.mp3"),
        ["track 2.mp3", "track 10.mp3"]
    );
    assert_eq!(names_for(&picked, &shown, "Live"), ["Live"]);
}

#[test]
fn a_right_click_picks_the_entry_and_a_menu_item_says_so() {
    let mut explorer = explorer(music()).menu(|_, _| vec![MenuItem::new("queue", "Add to queue")]);
    let mut screen = Screen::new(900.0);
    screen.settle(&mut explorer);
    let cover = at(&explorer, "Cover.png");
    screen.press(
        &mut explorer,
        cover,
        egui::PointerButton::Secondary,
        egui::Modifiers::NONE,
    );
    assert_eq!(picked(&explorer), ["Cover.png"]);
    // The menu opens by the pointer: Open, Rename, a separator, then the app's item. Try
    // each place down it, opening it again after a miss.
    for n in 0..8 {
        let item = cover + egui::vec2(24.0, 10.0 + 20.0 * n as f32);
        screen.click(&mut explorer, item, egui::Modifiers::NONE);
        if screen
            .said
            .iter()
            .any(|e| matches!(e, ExplorerEvent::Menu { .. }))
        {
            break;
        }
        explorer.renaming = None;
        screen.said.clear();
        screen.press(
            &mut explorer,
            cover,
            egui::PointerButton::Secondary,
            egui::Modifiers::NONE,
        );
    }
    assert_eq!(
        screen.said,
        [ExplorerEvent::Menu {
            id: "queue".into(),
            names: vec!["Cover.png".into()],
        }]
    );
}
