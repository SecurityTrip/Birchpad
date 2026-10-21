//! Plugins > Hex Editor (ADR 0039): the active document's bytes, in its encoding, as a hex dump
//! in a new document; once the dump is edited, Apply Hex to Document reads it back into bytes,
//! decodes them in the document's encoding and replaces its text as one undo step.

use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use birchpad_tools::hex;
use gpui_kit::{App, Context, Entity, EntityId, Global, WeakEntity, Window};

use crate::buffer::Buffer;
use crate::commands::{CommandRegistry, Register as _};
use crate::workspace::Workspace;

/// Hex documents and the documents whose bytes they show.
#[derive(Default)]
struct HexLinks(HashMap<EntityId, WeakEntity<Buffer>>);

impl Global for HexLinks {}

fn links(cx: &mut App) -> &mut HashMap<EntityId, WeakEntity<Buffer>> {
    if !cx.has_global::<HexLinks>() {
        cx.set_global(HexLinks::default());
    }
    &mut cx.global_mut::<HexLinks>().0
}

#[derive(serde::Deserialize)]
struct FindArgs {
    /// Hex digits, or text in quotes; none goes to the hex view's field.
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    backward: bool,
}

pub fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("hex.view", |this, (), window, cx| {
        this.view_in_hex(window, cx)
    });
    registry.workspace("hex.apply", |this, (), window, cx| {
        this.apply_hex(window, cx)
    });
    registry.workspace("hex.editor", |this, (), window, cx| {
        let view = this
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("hex-view.there-is-no-document")))?;
        this.active_pane()
            .clone()
            .update(cx, |pane, cx| pane.toggle_hex(&view, window, cx));
        this.refresh_menus(cx);
        Ok(())
    });
    registry.workspace("hex.switch-column", |this, (), _, cx| {
        let editor = this
            .active_hex_editor(cx)
            .ok_or_else(|| anyhow!("{}", t!("hex-editor.not-in-hex")))?;
        editor.update(cx, |editor, cx| editor.switch_column(cx));
        Ok(())
    });
    registry.workspace("hex.find", |this, args: FindArgs, window, cx| {
        let editor = this
            .active_hex_editor(cx)
            .ok_or_else(|| anyhow!("{}", t!("hex-editor.not-in-hex")))?;
        match args.pattern {
            Some(pattern) => editor.update(cx, |editor, cx| {
                editor.find_next(&pattern, args.backward, cx)
            }),
            None => {
                let field = editor.read(cx).find.clone();
                field.update(cx, |input, cx| input.focus(window, cx));
                Ok(())
            }
        }
    });
}

impl Workspace {
    /// The active document's hex view, while it shows.
    pub fn active_hex_editor(&self, cx: &App) -> Option<Entity<crate::hex_editor::HexEditor>> {
        let view = self.active_view(cx)?;
        self.active_pane().read(cx).hex_editor(&view).cloned()
    }
}

impl Workspace {
    /// Opens the active document's bytes as a hex dump in a new document.
    fn view_in_hex(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let view = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("hex-view.there-is-no-document")))?;
        let source = view.read(cx).buffer.clone();
        let (bytes, eol) = {
            let doc = source.read(cx).doc();
            let format = doc.format();
            let bytes = birchpad_io::encode_document(doc.text(), format)
                .map_err(|error| anyhow!("{error}"))?;
            (bytes, doc.line_ending())
        };
        let dump = hex::dump(&bytes, eol.as_str());
        self.open_text_in_new_document(&dump, window, cx)?;
        if let Some(hex_view) = self.active_view(cx) {
            let id = hex_view.read(cx).buffer.entity_id();
            links(cx).insert(id, source.downgrade());
        }
        Ok(())
    }

    /// Reads the active hex dump back into its document.
    fn apply_hex(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let view = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("{}", t!("hex-view.there-is-no-document")))?;
        let id = view.read(cx).buffer.entity_id();
        let source = links(cx).get(&id).cloned().ok_or_else(|| {
            anyhow!(
                "{}",
                t!("hex-view.this-document-is-not-a-hex-view-use-view-in-hex")
            )
        })?;
        let Some(source) = source.upgrade() else {
            links(cx).remove(&id);
            bail!(
                "{}",
                t!("hex-view.the-document-this-hex-view-shows-is-closed")
            );
        };
        let bytes = hex::parse(&view.read(cx).text(cx).to_string())?;
        let encoding = source.read(cx).doc().format().encoding;
        let decoded = birchpad_io::decode(bytes, encoding);
        if let Some(problem) = decoded.problem {
            bail!(
                "{}",
                t!(
                    "hex-view.the-bytes-are-not-text-at-byte",
                    encoding = encoding,
                    problem = problem.offset()
                )
            );
        }
        let target = self
            .all_views(cx)
            .into_iter()
            .find(|candidate| candidate.read(cx).buffer == source)
            .ok_or_else(|| {
                anyhow!(
                    "{}",
                    t!("hex-view.the-document-this-hex-view-shows-is-closed")
                )
            })?;
        apply_text(&target, &decoded.text, cx)?;
        self.activate_view(&target, window, cx);
        Ok(())
    }
}

fn apply_text(view: &Entity<crate::editor::EditorView>, text: &str, cx: &mut App) -> Result<()> {
    view.update(cx, |view, cx| {
        let transaction = hex::replace_differing(view.text(cx), text)?;
        if transaction.changes().is_identity() {
            return Ok(());
        }
        if !view.apply_command_edit(transaction, cx) {
            bail!("{}", t!("hex-view.the-document-cannot-be-changed"));
        }
        Ok(())
    })
}
