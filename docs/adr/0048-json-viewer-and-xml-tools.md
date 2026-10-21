# ADR 0048: JSON Viewer's tree and XML Tools' validation

- Status: accepted
- Date: 2026-10-21
- Changes [ADR 0039](0039-built-in-tools.md): JSON Viewer gets its panel, XML Tools its XPath,
  validation and escaping.

## Context

ADR 0039 gave JSON Viewer and XML Tools their formatting commands. What their users reach for
next is JSON Viewer's tree panel (find a value by its path, copy it) and sorting keys, and XML
Tools' XPath evaluation, validation against a DTD or an XML Schema, escaping and unescaping,
and closing tags as they are typed. Built from the plugins' documentation and behavior, and
from the W3C recommendations for XPath 1.0, XML 1.0 and XML Schema 1.0; no code of the plugins
or of an XPath or schema library under the GPL.

## Decision

- **JSON as a flat tree.** `json_tree` reads a text with the JSON lexer of ADR 0039 into a list
  of nodes in document order (key or index, kind, range, parent, children), so that neither
  building nor dropping recurses at the 10 000 levels JSON may nest. Paths are written
  `$.name`, `$["any name"]` and `$[0]`, and read with or without `$`. Sorting keys writes the
  text again from the tree, formatted, members by name (stably), arrays unchanged.
- **JSON Viewer is a side panel** (Plugins > JSON Viewer > Show JSON Viewer) that follows the
  active document and reads it again when it changes. A click selects the value in the text;
  folded values stay folded by path through edits. Copy Value (a string unescaped), Copy Key
  and Copy Path work on the value chosen in the panel, or, with the panel closed, on the
  value at the caret.
- **XPath, a part of 1.0, on roxmltree.** Location paths with `/` and `//`, the child,
  attribute, self and parent steps, unions, predicates with positions, comparisons, `and`,
  `or`, `not()` and the functions most used in predicates. Prefixes are not resolved to
  namespaces: names match by their local part, which is what a document with one namespace
  needs. The dialog lists the nodes and selects one when clicked; a value (`count(...)`) is
  shown as such.
- **Validation of our own.** Content models (DTD and schema alike) are matched by the sets of
  positions each particle can reach, which needs no automaton and handles occurrences and
  `all`. DTDs: element and attribute declarations of the internal subset and of the file the
  `SYSTEM` identifier names; parameter entities are not expanded. Schemas: elements, named and
  anonymous types, model groups, group and attribute group references, simple and complex
  content derivations, and the facets of simple types; `import`, `include`, substitution
  groups, identity constraints and `xsi:type` are not followed. Validate Now uses the DTD the
  document declares, else the schema its root names (a file next to it), else asks for one.
  Problems are listed in the document's order, at most 100; the first is selected.
- **Escaping** writes `& < > " '` as entities; unescaping reads those and character references.
- **Tag Auto-Close** in XML Tools is the existing Auto-Completion setting `completion.close-tags`
  (closing tags as their `>` is typed), so that one switch serves both menus.

## Consequences

- Schemas that rely on what is not followed validate less than a full processor would: a
  document can pass that a full processor refuses, never the other way round for what is
  checked.
- XPath expressions with namespace prefixes select by local names, so two namespaces with the
  same local names cannot be told apart.
- The new readers (JSON paths, XPath expressions, DTDs and schemas) are fuzzed with the others.
