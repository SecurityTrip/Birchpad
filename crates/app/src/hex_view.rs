//! Plugins > Hex Editor (ADR 0039): the active document's bytes, in its encoding, as a hex dump
//! in a new document; once the dump is edited, Apply Hex to Document reads it back into bytes,
//! decodes them in the document's encoding and replaces its text as one undo step.

use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};
use birchpad_core::{Edit, Rope, Selection, Transaction};
use birchpad_tools::hex;
use gpui_kit::{App, Context, Entity, EntityId, Global, WeakEntity, Window};

use crate::buffer::Buffer;
use crate::commands::CommandRegistry;
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

pub(crate) fn register_commands(registry: &mut CommandRegistry) {
    registry.workspace("hex.view", |this, (), window, cx| {
        this.view_in_hex(window, cx)
    });
    registry.workspace("hex.apply", |this, (), window, cx| {
        this.apply_hex(window, cx)
    });
}

/// The edit that turns `old` into `new`, touching only what differs between them.
pub(crate) fn replace_differing(old: &Rope, new: &str) -> Result<Transaction> {
    let old_text = old.to_string();
    let prefix = old_text
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old_text.len().min(new.len()), |((at, _), _)| at);
    let prefix = (0..=prefix)
        .rev()
        .find(|&at| old_text.is_char_boundary(at) && new.is_char_boundary(at))
        .unwrap_or(0);
    let old_rest = &old_text[prefix..];
    let new_rest = &new[prefix..];
    let mut suffix = old_rest
        .bytes()
        .rev()
        .zip(new_rest.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old_rest.is_char_boundary(old_rest.len() - suffix)
        || !new_rest.is_char_boundary(new_rest.len() - suffix)
    {
        suffix -= 1;
    }
    let range = prefix..old_text.len() - suffix;
    let inserted = &new_rest[..new_rest.len() - suffix];
    Ok(
        Transaction::from_edits(old, [Edit::replace(range.clone(), inserted)])?
            .with_selection(Selection::point(range.start + inserted.len())),
    )
}

impl Workspace {
    /// Opens the active document's bytes as a hex dump in a new document.
    fn view_in_hex(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        let view = self
            .active_view(cx)
            .ok_or_else(|| anyhow!("there is no document"))?;
        let source = view.read(cx).buffer.clone();
        let (bytes, eol) = {
            let doc = source.read(cx).doc();
            let format = doc.format();
            let bytes = birchpad_io::encode(doc.text(), format.encoding, format.bom)
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
            .ok_or_else(|| anyhow!("there is no document"))?;
        let id = view.read(cx).buffer.entity_id();
        let source = links(cx)
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow!("this document is not a hex view: use View in Hex first"))?;
        let Some(source) = source.upgrade() else {
            links(cx).remove(&id);
            bail!("the document this hex view shows is closed");
        };
        let bytes = hex::parse(&view.read(cx).text(cx).to_string())?;
        let encoding = source.read(cx).doc().format().encoding;
        let decoded = birchpad_io::decode(bytes, encoding);
        if let Some(problem) = decoded.problem {
            bail!(
                "the bytes are not {} text (at byte {})",
                encoding,
                problem.offset()
            );
        }
        let target = self
            .all_views(cx)
            .into_iter()
            .find(|candidate| candidate.read(cx).buffer == source)
            .ok_or_else(|| anyhow!("the document this hex view shows is closed"))?;
        apply_text(&target, &decoded.text, cx)?;
        self.activate_view(&target, window, cx);
        Ok(())
    }
}

fn apply_text(view: &Entity<crate::editor::EditorView>, text: &str, cx: &mut App) -> Result<()> {
    view.update(cx, |view, cx| {
        let transaction = replace_differing(view.text(cx), text)?;
        if transaction.changes().is_identity() {
            return Ok(());
        }
        if !view.apply_command_edit(transaction, cx) {
            bail!("the document cannot be changed");
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests;
