//! A Markdown editor for egui with a formatting toolbar: [alumina-markdown], re-exported so an
//! app depends on corundum alone.
//!
//! The editor's colours follow the visuals (`MarkdownStyle::from_visuals`), so corundum's
//! theme applies to it. Use [`toolbar`] for corundum's look: the buttons are its Lucide icons
//! ([`icons`](crate::icons)), and their names are in the context's language. The editor itself
//! is alumina's, unchanged.
//!
//! [alumina-markdown]: https://crates.io/crates/alumina-markdown

pub use alumina_markdown::*;

use crate::{icons, tr};

/// The toolbar's buttons as corundum's Lucide icons.
///
/// The icons need the image loaders (`icons::install_loaders`, which corundum's shell
/// already installs); without them the buttons show no icon.
pub fn toolbar_icons() -> ToolbarIcons {
    let mut i = ToolbarIcons::default();
    i.strong = Some(icons::bold());
    i.emphasis = Some(icons::italic());
    i.strikethrough = Some(icons::strikethrough());
    i.code = Some(icons::code());
    i.link = Some(icons::link());
    i.heading = Some(icons::heading());
    i.bullet_list = Some(icons::list());
    i.numbered_list = Some(icons::list_ordered());
    i.task_list = Some(icons::list_todo());
    i.quote = Some(icons::text_quote());
    i.code_block = Some(icons::square_code());
    i
}

/// The toolbar's labels (shown without an icon) in the context's language.
pub fn toolbar_labels(ctx: &egui::Context) -> ToolbarLabels {
    let mut l = ToolbarLabels::default();
    l.strong = tr!(ctx, "corundum-markdown-strong-label");
    l.emphasis = tr!(ctx, "corundum-markdown-emphasis-label");
    l.strikethrough = tr!(ctx, "corundum-markdown-strikethrough-label");
    l.code = tr!(ctx, "corundum-markdown-code-label");
    l.link = tr!(ctx, "corundum-markdown-link-label");
    l.heading = tr!(ctx, "corundum-markdown-heading-label");
    l.bullet_list = tr!(ctx, "corundum-markdown-bullet-list-label");
    l.numbered_list = tr!(ctx, "corundum-markdown-numbered-list-label");
    l.task_list = tr!(ctx, "corundum-markdown-task-list-label");
    l.quote = tr!(ctx, "corundum-markdown-quote-label");
    l.code_block = tr!(ctx, "corundum-markdown-code-block-label");
    l
}

/// The buttons' accessible names and hover text in the context's language.
pub fn toolbar_names(ctx: &egui::Context) -> ToolbarNames {
    let mut n = ToolbarNames::default();
    n.strong = tr!(ctx, "corundum-markdown-strong-name");
    n.emphasis = tr!(ctx, "corundum-markdown-emphasis-name");
    n.strikethrough = tr!(ctx, "corundum-markdown-strikethrough-name");
    n.code = tr!(ctx, "corundum-markdown-code-name");
    n.link = tr!(ctx, "corundum-markdown-link-name");
    n.heading = tr!(ctx, "corundum-markdown-heading-name");
    n.bullet_list = tr!(ctx, "corundum-markdown-bullet-list-name");
    n.numbered_list = tr!(ctx, "corundum-markdown-numbered-list-name");
    n.task_list = tr!(ctx, "corundum-markdown-task-list-name");
    n.quote = tr!(ctx, "corundum-markdown-quote-name");
    n.code_block = tr!(ctx, "corundum-markdown-code-block-name");
    n
}

/// A toolbar for the editor with `editor_id`, with all three above applied.
///
/// The icons need the image loaders (`icons::install_loaders`, which corundum's shell
/// already installs); without them the buttons show no icon.
pub fn toolbar<'t>(
    ctx: &egui::Context,
    text: &'t mut String,
    editor_id: egui::Id,
) -> MarkdownToolbar<'t> {
    MarkdownToolbar::new(text, editor_id)
        .labels(toolbar_labels(ctx))
        .names(toolbar_names(ctx))
        .icons(toolbar_icons())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, LanguageSettings};

    fn ctx_in(tag: &str) -> egui::Context {
        let ctx = egui::Context::default();
        crate::i18n::install(
            &ctx,
            &LanguageSettings {
                language: Language::Explicit(tag.into()),
            },
        );
        ctx
    }

    #[test]
    fn every_toolbar_button_has_an_icon() {
        let i = toolbar_icons();
        let all = [
            &i.strong,
            &i.emphasis,
            &i.strikethrough,
            &i.code,
            &i.link,
            &i.heading,
            &i.bullet_list,
            &i.numbered_list,
            &i.task_list,
            &i.quote,
            &i.code_block,
        ];
        assert_eq!(all.len(), 11);
        assert!(all.iter().all(|icon| icon.is_some()));
    }

    #[test]
    fn the_toolbar_speaks_the_language() {
        assert_eq!(toolbar_names(&ctx_in("ja")).strong, "太字");
        assert_eq!(toolbar_names(&ctx_in("ja")).code_block, "コードブロック");
        assert_eq!(toolbar_names(&ctx_in("en")).strong, "Bold");
        assert_eq!(toolbar_labels(&ctx_in("ja")).code_block, "{ }");
    }

    #[test]
    fn the_markdown_strings_exist_in_both_languages() {
        const FIELDS: [&str; 11] = [
            "strong",
            "emphasis",
            "strikethrough",
            "code",
            "link",
            "heading",
            "bullet-list",
            "numbered-list",
            "task-list",
            "quote",
            "code-block",
        ];
        for tag in ["en", "ja"] {
            let ctx = ctx_in(tag);
            for field in FIELDS {
                for part in ["label", "name"] {
                    let id = format!("corundum-markdown-{field}-{part}");
                    assert_ne!(crate::i18n::tr(&ctx, &id), id, "{tag} lacks {id}");
                }
            }
        }
    }
}
