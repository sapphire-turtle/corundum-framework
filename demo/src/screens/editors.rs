//! The Editors: a Markdown note under its toolbar, and a code editor over JSON.
//!
//! Both are alumina's editors, re-exported by corundum-framework-gui with its theme: the
//! toolbar speaks the language and wears corundum's icons.

use corundum_framework::gui::code::CodeEditor;
use corundum_framework::gui::i18n::Message;
use corundum_framework::gui::markdown::{self, MarkdownEditor};
use corundum_framework::gui::{egui, tr};

use super::{heading, section};

/// A note with headings, a list, a task list, a link and a fenced block.
const NOTE: &str = "# Trip notes\n\nA **short** note, in *Markdown*.\n\n## To pack\n\n- passport\n- charger\n- a good book\n\n## To do\n\n- [x] book the hotel\n- [ ] buy the tickets\n\nThe [corundum repository](https://github.com/fluo10/corundum-framework) has the framework.\n\n```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n";

/// A small JSON document, edited with no highlighting.
const JSON: &str =
    "{\n  \"name\": \"corundum\",\n  \"tags\": [\"gui\", \"egui\"],\n  \"stable\": false\n}\n";

/// What the screen keeps between frames.
pub(crate) struct State {
    note: String,
    json: String,
}

impl Default for State {
    fn default() -> State {
        State {
            note: NOTE.to_owned(),
            json: JSON.to_owned(),
        }
    }
}

pub(super) fn ui(ui: &mut egui::Ui, state: &mut State, log: &mut Message) {
    heading(ui, "demo-screen-editors");
    ui.label(tr!(ui, "demo-editors-intro"));

    section(ui, "demo-editors-markdown");
    let id = ui.make_persistent_id("editors-markdown");
    markdown::toolbar(ui.ctx(), &mut state.note, id).show(ui);
    let output = MarkdownEditor::new(&mut state.note)
        .id(id)
        .desired_rows(14)
        .show(ui);
    if let Some(url) = output.link_clicked {
        *log = Message::new("demo-status-opened").arg("name", url);
    }

    section(ui, "demo-editors-code");
    ui.label(tr!(ui, "demo-editors-code-note"));
    CodeEditor::new(&mut state.json)
        .id(ui.make_persistent_id("editors-code"))
        .desired_rows(8)
        .show(ui);
}
