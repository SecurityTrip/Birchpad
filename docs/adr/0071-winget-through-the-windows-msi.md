# ADR 0071 — WinGet through the Windows MSI

Stable releases prepare a WinGet manifest for `SecurityTrip.Birchpad` using the Windows x64
MSI. WinGet manages upgrades with Windows Installer's existing upgrade code. The per-user
installer remains on Birchpad's own update channel (ADR 0020).

The Release workflow hashes the exact MSI that goes into the draft, after any signing. It
keeps the manifest as a separate workflow artifact ready for the community repository.
Submission follows publication because WinGet validates the installer at its public URL.

The format and submission procedure follow
[Microsoft's manifest documentation](https://learn.microsoft.com/en-us/windows/package-manager/package/manifest).
