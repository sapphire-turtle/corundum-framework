//! [`scope_tabs`]: a tab per place a setting can be kept.

use super::layout::marked;

/// One tab of [`scope_tabs`]: a place a setting can be kept — this device or the whole
/// workgroup, the client or the server. `S` is the host's own type naming them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScopeTab<S> {
    /// The scope the tab opens.
    pub scope: S,
    /// Its name, in the installed language.
    pub label: String,
    /// Whether it can be opened now: a workspace's tab without a workspace cannot.
    pub enabled: bool,
    /// Whether it holds changes not saved yet; the tab is marked.
    pub dirty: bool,
}

impl<S> ScopeTab<S> {
    /// An enabled tab with nothing unsaved.
    pub fn new(scope: S, label: impl Into<String>) -> ScopeTab<S> {
        ScopeTab {
            scope,
            label: label.into(),
            enabled: true,
            dirty: false,
        }
    }

    /// Whether it can be opened now.
    pub fn enabled(mut self, enabled: bool) -> ScopeTab<S> {
        self.enabled = enabled;
        self
    }

    /// Whether it holds changes not saved yet.
    pub fn dirty(mut self, dirty: bool) -> ScopeTab<S> {
        self.dirty = dirty;
        self
    }
}

/// The row of `tabs`, `open` the selected one, with `hint` after them (why a tab is
/// disabled, say). An open tab that is not among the enabled ones yields to the first that
/// is. `true` when `open` changed, by a click or by yielding: what was shown for the old
/// tab (a field's error, say) no longer applies.
pub fn scope_tabs<S: Copy + PartialEq>(
    ui: &mut egui::Ui,
    open: &mut S,
    tabs: &[ScopeTab<S>],
    hint: Option<&str>,
) -> bool {
    let before = *open;
    if !tabs.iter().any(|t| t.enabled && t.scope == *open)
        && let Some(first) = tabs.iter().find(|t| t.enabled)
    {
        *open = first.scope;
    }
    ui.horizontal_wrapped(|ui| {
        for tab in tabs {
            if ui
                .add_enabled(
                    tab.enabled,
                    egui::Button::selectable(*open == tab.scope, marked(&tab.label, tab.dirty)),
                )
                .clicked()
            {
                *open = tab.scope;
            }
        }
        if let Some(hint) = hint {
            ui.small(hint);
        }
    });
    *open != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::render;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Scope {
        Device,
        Workgroup,
        Workspace,
    }

    fn tabs(workspace: bool) -> Vec<ScopeTab<Scope>> {
        vec![
            ScopeTab::new(Scope::Device, "Device").dirty(true),
            ScopeTab::new(Scope::Workgroup, "Workgroup"),
            ScopeTab::new(Scope::Workspace, "Workspace").enabled(workspace),
        ]
    }

    #[test]
    fn an_enabled_open_tab_stays() {
        let mut open = Scope::Workgroup;
        let mut changed = false;
        render([600.0, 400.0], |ui| {
            changed |= scope_tabs(ui, &mut open, &tabs(true), None)
        });
        assert_eq!(open, Scope::Workgroup);
        assert!(!changed);
    }

    #[test]
    fn a_disabled_open_tab_yields_to_the_first_enabled() {
        let mut open = Scope::Workspace;
        let mut changed = false;
        render([600.0, 400.0], |ui| {
            changed |= scope_tabs(ui, &mut open, &tabs(false), Some("No workspace"))
        });
        assert_eq!(open, Scope::Device);
        assert!(changed);
    }

    #[test]
    fn with_no_tab_enabled_the_open_one_stays() {
        let mut open = Scope::Workspace;
        let none: Vec<_> = tabs(false).into_iter().map(|t| t.enabled(false)).collect();
        render([600.0, 400.0], |ui| {
            scope_tabs(ui, &mut open, &none, None);
        });
        assert_eq!(open, Scope::Workspace);
    }
}
