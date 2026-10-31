//! [`KeysPage`]: the keys, and the buttons that change them.

use super::{KeyInfo, KeysAnswer, KeysBackend, KeysRequest, Token};
use crate::tr;

/// A change waiting for the user to confirm it.
enum Confirm {
    Retire(KeyInfo),
    Rotate(KeyInfo),
}

/// The API keys page: the keys (retired ones behind a toggle), a form to add one, and per
/// key Retire, Restore and Rotate. Retire and Rotate ask first. A new token is shown in a
/// dialog, once, and forgotten when the dialog closes.
pub struct KeysPage {
    backend: Box<dyn KeysBackend>,
    keys: Vec<KeyInfo>,
    /// Whether the list was asked for.
    asked: bool,
    error: Option<String>,
    token: Option<(KeyInfo, Token)>,
    confirm: Option<Confirm>,
    name: String,
    description: String,
    show_retired: bool,
}

impl KeysPage {
    /// The page over `backend`. It asks for the list when first drawn.
    pub fn new(backend: Box<dyn KeysBackend>) -> KeysPage {
        KeysPage {
            backend,
            keys: Vec::new(),
            asked: false,
            error: None,
            token: None,
            confirm: None,
            name: String::new(),
            description: String::new(),
            show_retired: false,
        }
    }

    /// The token the dialog shows, while it is open.
    pub fn shown_token(&self) -> Option<&str> {
        self.token.as_ref().map(|(_, t)| t.expose())
    }

    /// Close the token dialog, forgetting the token.
    pub fn close_token(&mut self) {
        self.token = None;
    }

    /// The keys as last listed.
    #[cfg(test)]
    pub(crate) fn keys(&self) -> &[KeyInfo] {
        &self.keys
    }

    /// Send the change the user confirmed.
    fn confirmed(&mut self) {
        match self.confirm.take() {
            Some(Confirm::Retire(k)) => self.backend.send(KeysRequest::Retire(k.id)),
            Some(Confirm::Rotate(k)) => self.backend.send(KeysRequest::Rotate(k.id)),
            None => {}
        }
    }

    /// Take the backend's answers.
    fn receive(&mut self) {
        for answer in self.backend.poll() {
            match answer {
                KeysAnswer::List(keys) => {
                    self.keys = keys;
                    self.error = None;
                }
                KeysAnswer::Changed(_) => self.backend.send(KeysRequest::List),
                KeysAnswer::WithToken(key, token) => {
                    self.token = Some((key, token));
                    self.backend.send(KeysRequest::List);
                }
                KeysAnswer::Failed(message) => self.error = Some(message),
            }
        }
    }

    /// Draw the page.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.receive();
        if !self.asked {
            self.asked = true;
            self.backend.send(KeysRequest::List);
        }

        ui.heading(tr!(ui, "corundum-keys-title"));
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }

        self.add_ui(ui);
        ui.separator();
        self.list_ui(ui);
        self.confirm_ui(ui);
        self.token_ui(ui);
    }

    fn add_ui(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("corundum_keys_add")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label(tr!(ui, "corundum-keys-name"));
                ui.text_edit_singleline(&mut self.name);
                ui.end_row();
                ui.label(tr!(ui, "corundum-keys-description"));
                ui.text_edit_singleline(&mut self.description);
                ui.end_row();
            });
        let name = self.name.trim();
        if ui
            .add_enabled(
                !name.is_empty(),
                egui::Button::new(tr!(ui, "corundum-keys-add")),
            )
            .clicked()
        {
            let description = self.description.trim();
            self.backend.send(KeysRequest::Add {
                name: name.to_owned(),
                description: (!description.is_empty()).then(|| description.to_owned()),
            });
            self.name.clear();
            self.description.clear();
        }
    }

    fn list_ui(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(
            &mut self.show_retired,
            tr!(ui, "corundum-keys-show-retired"),
        );
        let shown: Vec<KeyInfo> = self
            .keys
            .iter()
            .filter(|k| self.show_retired || !k.is_retired())
            .cloned()
            .collect();
        if shown.is_empty() {
            ui.weak(tr!(ui, "corundum-keys-none"));
            return;
        }
        for key in shown {
            ui.horizontal(|ui| {
                ui.strong(&key.name);
                if let Some(d) = &key.description {
                    ui.weak(d);
                }
                ui.weak(key.created_at.format("%Y-%m-%d").to_string());
                if key.is_retired() {
                    ui.weak(tr!(ui, "corundum-keys-retired"));
                    if ui.button(tr!(ui, "corundum-keys-restore")).clicked() {
                        self.backend.send(KeysRequest::Restore(key.id.clone()));
                    }
                } else {
                    if ui.button(tr!(ui, "corundum-keys-rotate")).clicked() {
                        self.confirm = Some(Confirm::Rotate(key.clone()));
                    }
                    if ui.button(tr!(ui, "corundum-keys-retire")).clicked() {
                        self.confirm = Some(Confirm::Retire(key.clone()));
                    }
                }
            });
        }
    }

    fn confirm_ui(&mut self, ui: &mut egui::Ui) {
        let Some(confirm) = &self.confirm else {
            return;
        };
        let text = match confirm {
            Confirm::Retire(k) => tr!(ui, "corundum-keys-confirm-retire", name = k.name.as_str()),
            Confirm::Rotate(k) => tr!(ui, "corundum-keys-confirm-rotate", name = k.name.as_str()),
        };
        let mut answer = None;
        let modal =
            egui::Modal::new(egui::Id::unique("corundum_keys_confirm")).show(ui.ctx(), |ui| {
                ui.label(text);
                ui.horizontal(|ui| {
                    if ui.button(tr!(ui, "corundum-keys-confirm")).clicked() {
                        answer = Some(true);
                    }
                    if ui.button(tr!(ui, "corundum-keys-cancel")).clicked() {
                        answer = Some(false);
                    }
                });
            });
        if modal.should_close() && answer.is_none() {
            answer = Some(false);
        }
        match answer {
            Some(true) => self.confirmed(),
            Some(false) => self.confirm = None,
            None => {}
        }
    }

    fn token_ui(&mut self, ui: &mut egui::Ui) {
        let Some((key, token)) = &self.token else {
            return;
        };
        let mut close = false;
        let modal =
            egui::Modal::new(egui::Id::unique("corundum_keys_token")).show(ui.ctx(), |ui| {
                ui.strong(&key.name);
                ui.label(tr!(ui, "corundum-keys-token-once"));
                ui.add(egui::Label::new(egui::RichText::new(token.expose()).monospace()).wrap());
                ui.horizontal(|ui| {
                    if ui.button(tr!(ui, "corundum-keys-copy")).clicked() {
                        ui.ctx().copy_text(token.expose().to_owned());
                    }
                    if ui.button(tr!(ui, "corundum-keys-close")).clicked() {
                        close = true;
                    }
                });
            });
        // Not closed by a click outside: the token would be lost before it was copied.
        let _ = modal;
        if close {
            self.close_token();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::keys::KeyStore;
    use crate::test_support::render;

    #[derive(Default)]
    struct Shared {
        sent: Vec<KeysRequest>,
        queued: Vec<KeysAnswer>,
    }

    struct Fake(Rc<RefCell<Shared>>);

    impl KeysBackend for Fake {
        fn send(&mut self, request: KeysRequest) {
            self.0.borrow_mut().sent.push(request);
        }
        fn poll(&mut self) -> Vec<KeysAnswer> {
            std::mem::take(&mut self.0.borrow_mut().queued)
        }
    }

    fn page() -> (KeysPage, Rc<RefCell<Shared>>) {
        let shared = Rc::new(RefCell::new(Shared::default()));
        (KeysPage::new(Box::new(Fake(shared.clone()))), shared)
    }

    /// A real key's info and token, from a throwaway store.
    fn key(name: &str) -> (KeyInfo, Token) {
        let dir = tempfile::tempdir().unwrap();
        let (k, t) = KeyStore::open(dir.path())
            .unwrap()
            .add(name, None, None)
            .unwrap();
        (k.info(), t)
    }

    #[test]
    fn first_frame_asks_for_the_list() {
        let (mut p, shared) = page();
        render([400.0, 600.0], |ui| p.ui(ui));
        assert_eq!(shared.borrow().sent, vec![KeysRequest::List]);
    }

    #[test]
    fn token_is_shown_once() {
        let (mut p, shared) = page();
        render([400.0, 600.0], |ui| p.ui(ui));
        let (info, token) = key("p");
        shared
            .borrow_mut()
            .queued
            .push(KeysAnswer::WithToken(info, token.clone()));
        render([400.0, 600.0], |ui| p.ui(ui));
        assert_eq!(p.shown_token(), Some(token.expose()));
        p.close_token();
        assert_eq!(p.shown_token(), None);
        render([400.0, 600.0], |ui| p.ui(ui));
        assert_eq!(p.shown_token(), None);
        assert_eq!(
            shared.borrow().sent,
            vec![KeysRequest::List, KeysRequest::List],
            "the list is asked again after a change"
        );
    }

    #[test]
    fn failed_answer_is_shown_and_list_kept() {
        let (mut p, shared) = page();
        let (a, _) = key("a");
        shared.borrow_mut().queued.push(KeysAnswer::List(vec![a]));
        render([400.0, 600.0], |ui| p.ui(ui));
        shared
            .borrow_mut()
            .queued
            .push(KeysAnswer::Failed("x".into()));
        render([400.0, 600.0], |ui| p.ui(ui));
        assert_eq!(p.keys().len(), 1);
        assert_eq!(p.error.as_deref(), Some("x"));
    }

    #[test]
    fn retire_asks_before_sending() {
        let (mut p, shared) = page();
        let (a, _) = key("a");
        p.confirm = Some(Confirm::Retire(a.clone()));
        render([400.0, 600.0], |ui| p.ui(ui));
        assert!(
            !shared
                .borrow()
                .sent
                .iter()
                .any(|r| matches!(r, KeysRequest::Retire(_))),
            "sent before the user confirmed"
        );
        p.confirmed();
        assert!(
            shared
                .borrow()
                .sent
                .contains(&KeysRequest::Retire(a.id.clone()))
        );
        assert!(p.confirm.is_none());
    }
}
