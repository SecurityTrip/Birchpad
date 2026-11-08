# ADR 0064: Update API and install statistics

- Status: accepted
- Date: 2026-11-08
- Builds on [ADR 0020](0020-signed-updates-and-nightly-builds.md) (the signed manifest, which
  stays the only thing an update is trusted by).

## Context

Birchpad reads its update manifest from GitHub (ADR 0020): a release asset, which says nothing
back about how many copies of Birchpad there are, of which versions, on which channels and
platforms, and whether their PlantUML previews draw with the PlantUML Birchpad installs
(ADR 0045, ADR 0063) or another one. Those numbers decide what to support and what to test: when
an older version can be dropped from the release checklist, whether ARM64 builds are used,
whether a new PlantUML package reaches the copies that installed an earlier one. GitHub's
download counts cannot say it: a manifest is read once a day by every copy, a package is
downloaded once per update, and neither tells one copy from another.

The service behind `api.birchpad.ru` (its contract is Update API v1, kept with the service)
serves the same manifest and records one row per copy of Birchpad that asks.

## Decision

### A mirror of the signed manifest, not a source of truth

- Birchpad `POST`s to `https://api.birchpad.ru/v1/updates/check`. The answer (`200`) is the
  envelope `birchpad-updates.json` byte for byte, as the release workflows published it on
  GitHub. Birchpad checks it with the same `manifest::verify` as the manifest from GitHub: the
  same compiled-in keys, the same refusal of a manifest older than the newest seen (`state.toml`)
  and of an expired one. Neither the service, the proxy in front of it nor the connection can
  make Birchpad install anything the maintainer did not sign, nor hold it on an old manifest.
- Nothing else from the service is read: no unsigned field, no header, no redirect to another
  manifest.

### GitHub when the service fails

- Any failure of the service (no connection, a status other than `200`, a body larger than
  `MAX_MANIFEST_SIZE`, a manifest that does not verify, is older than the newest seen or has
  expired) makes the check read the manifest from GitHub (`DEFAULT_MANIFEST`) as before, and
  verify it as before. If that fails too, the check reports GitHub's error, as before: the
  service is never the reason a check fails. The service's error is only logged, on a check
  asked for and in the background alike, when GitHub then answered.
- `updates.url` set (an administrator's mirror) means that mirror alone, by `GET`, as before:
  no request to the service, no statistics. An empty `updates.url` is not set.
- `updates.mode = "off"`: nothing at all, as before.

### What is sent, and what is kept

- The body of the request is a JSON object with the fields of the contract, camelCase, empty
  ones left out: `installId`, `version`, `channel` (`updates.channel`), `platform`
  (`windows-x64`, `windows-arm64`, `linux-x64`, `macos-arm64`, `macos-x64`, left out elsewhere),
  `plantuml` (`component` for the PlantUML Birchpad installed, `custom` for another one, from
  `preview.plantuml` or `PATH`, `none` for none found) and `plantumlVersion` (the component's
  version, with `component` only). Which PlantUML it is is worked out as the previews choose it.
- `installId` is a random UUID (version 4) the copy makes for itself at its first check with
  statistics on and keeps in `state.toml` (`[updates] install-id`). It is not derived from
  anything on the computer, and it names a copy of Birchpad, not a person: two users of one
  computer, or a portable copy beside an installed one, are two ids.
- The service keeps one row per `installId`: the version first seen and the current one, the
  channel, the platform, the PlantUML and its version, the times of the first and the last
  check. It does not keep IP addresses, the `User-Agent` or a history of checks.

### Turning it off

- `updates.statistics = false` (Preferences > Updates > Send install statistics), or the
  `UpdateStatistics` policy (`REG_DWORD` 0, which users cannot override): the body is `{}`,
  which carries nothing a download of the manifest from GitHub does not (the address and the
  `User-Agent`), and the `installId` is deleted from `state.toml`. Turned on again, a new id is
  made, which nothing ties to the old one.
- `updates.mode = "off"` (or the `UpdateMode` policy): no request at all.
- The statistics are on by default, and said so in [PRIVACY.md](../../PRIVACY.md): what is
  sent, what is kept, and how to turn it off.

## Consequences

- One request a day per copy goes to a server of the project rather than to GitHub; GitHub is
  asked only when it fails, so a copy keeps updating while the service is down or gone.
- The service is one more thing to keep running, but never one updates depend on: its worst
  failure is a slower check (its timeout, then GitHub).
- The numbers count copies that check for updates: copies with `updates.mode = "off"`, with an
  administrator's mirror or behind a network that blocks the service are not counted, and a copy
  whose `state.toml` is deleted counts again.
- Privacy statements no longer say "no telemetry": they say what this is, and that it is off
  with one setting.
- The API is versioned (`/v1/`): a change to what is sent is a new version, and the old one
  keeps working for the copies already out.
