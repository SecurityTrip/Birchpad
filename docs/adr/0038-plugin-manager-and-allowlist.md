# ADR 0038: Plugin manager and the enterprise allowlist

- Status: accepted
- Date: 2026-10-12

## Context

Notepad++'s Plugins Admin installs plugins from an online list into the program's folder.
Enterprises need to control which plugins run; the policies `PluginInstall` and
`AllowedPlugins` were reserved for that since Phase 0.

## Decision

- **A plugin is a folder** with `plugin.toml` (id, name, version, description, author, the
  plugin API version `api`, the component file `entry`, the `events` it hears) and its
  component. Installed plugins live in `plugins/<id>/` next to the settings; the folder must
  be named after the id.
- **Plugins Admin** (Plugins > Plugins Admin...) lists the installed plugins with their
  version and description; it installs one from its `plugin.toml` (copying its folder, at most
  64 MiB; installing an id again updates it, through a staging folder so a failed copy keeps
  the old version), removes one, and turns one on or off (`plugins/disabled.toml`). Folders
  that hold no readable plugin are listed with the reason.
- **The policy applies to installing and to running:** `plugins.install = "allow"` (the
  default) lets every plugin run; `"allowlist"` only the ids in `plugins.allowed`; `"deny"`
  none. A plugin copied into the folder by hand that the policy does not allow is listed but
  not loaded.
- **No online catalog yet:** plugins are installed from files. A catalog will respect the
  `OnlineFeatures` policy.

## Consequences

- Built-in tools (ADR 0039) are part of Birchpad, not plugins: the policy does not turn them
  off.
- A plugin's id is public: allowlists name it, and renaming it makes a different plugin.
