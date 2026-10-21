# ADR 0051: The online catalog of plugins and dictionaries

- Status: accepted
- Date: 2026-10-21
- Changes [ADR 0038](0038-plugin-manager-and-allowlist.md): Plugins Admin installs and updates
  plugins from an online list too, as it said a catalog would, under the `OnlineFeatures`
  policy.

## Context

Notepad++'s Plugins Admin installs plugins from an online list (available, updates,
installed), checking each download against the list's hash; DSpellCheck downloads
dictionaries. Birchpad installed plugins from files only, and read dictionaries the user
already had. Built from Plugins Admin's and DSpellCheck's documentation and behavior; Notepad++'s
list itself (`nppPluginList`) holds DLLs Birchpad cannot run and is not used.

## Decision

- **One catalog for both**, `packaging/catalog/catalog.json` in Birchpad's repository, read from
  GitHub (`network.catalog` names another, for a mirror or a company's own): format 1; plugins
  with their id, name, version, description, author, plugin API and a ZIP; dictionaries with
  their language, name, `.aff` and `.dic`. Every file is pinned by its size (at most 64 MiB) and
  SHA-256 and must be served over https. A catalog of a newer format says so.
- **Only when asked, only when allowed.** The list is read when the user clicks Refresh List in
  Plugins Admin or opens Download Languages...; nothing is read while `network.online-features`
  (the `OnlineFeatures` policy) is off. Downloads go through the transport of updates, with its
  proxy.
- **Checked before kept.** A plugin's ZIP is checked against the catalog, unpacked into a work
  folder, and installed as a plugin from a file is (ADR 0038): the allowlist and the plugin API
  apply, and a package whose `plugin.toml` is another plugin than listed is removed. A
  dictionary's files arrive as `.part` files and replace those of the same language only once
  both are checked and read as a Hunspell dictionary.
- **Updates** are offered for installed plugins the catalog has in a newer version (by semantic
  version, else when the versions differ).
- The catalog starts with the built-in US English dictionary; plugins and dictionaries are added
  to it by pull request, with where they come from and their license.

## Consequences

- The catalog is as trustworthy as Birchpad's repository: hashes protect against damaged or
  swapped downloads, not against a change to the catalog itself, which goes through review.
- Companies that forbid online features get none of this; those that mirror set
  `network.catalog`.
