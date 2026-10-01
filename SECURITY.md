# Security Policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report vulnerabilities privately through GitHub:
**Security → Report a vulnerability** on the
[repository page](https://github.com/SecurityTrip/Birchpad/security/advisories/new).

We aim to acknowledge reports within 3 working days and to agree on a disclosure timeline with
the reporter.

## Scope

Of particular interest:

- the update mechanism (manifest and package signature verification, downgrade attacks);
- file handling (malformed encodings, huge or crafted files, path handling, symlinks);
- the plugin sandbox;
- administrator policies that can be bypassed by a non-administrator user.

## Supported versions

Birchpad is pre-alpha; only the latest `main` is supported until the first stable release.
