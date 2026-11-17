"""Prepare the WinGet submission for a stable Birchpad release."""

import argparse
import hashlib
import re
from pathlib import Path


PACKAGE = "SecurityTrip.Birchpad"
REPOSITORY = "https://github.com/SecurityTrip/Birchpad"
MANIFEST_VERSION = "1.6.0"


def prepare(version, artifacts, output):
    if not re.fullmatch(r"(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*)){2}", version):
        raise ValueError("WinGet requires a stable release version (X.Y.Z)")
    installer = f"birchpad-{version}-windows-x64.msi"
    with (artifacts / installer).open("rb") as source:
        if not source.peek(1):
            raise ValueError("the Windows installer is empty")
        sha256 = hashlib.file_digest(source, "sha256").hexdigest().upper()

    directory = output / "manifests" / "s" / "SecurityTrip" / "Birchpad" / version
    directory.mkdir(parents=True, exist_ok=True)
    common = f'PackageIdentifier: {PACKAGE}\nPackageVersion: "{version}"\n'
    manifests = {
        "": ("version", "DefaultLocale: en-US\n"),
        ".locale.en-US": (
            "defaultLocale",
            "PackageLocale: en-US\n"
            "Publisher: The Birchpad Contributors\n"
            "PackageName: Birchpad\n"
            "PackageUrl: https://birchpad.ru/\n"
            "License: MIT OR Apache-2.0\n"
            f"LicenseUrl: {REPOSITORY}/tree/v{version}#license\n"
            f"ReleaseNotesUrl: {REPOSITORY}/releases/tag/v{version}\n"
            "ShortDescription: A cross-platform text and source code editor.\n"
            "Moniker: birchpad\n",
        ),
        ".installer": (
            "installer",
            "InstallerType: wix\n"
            "Scope: machine\n"
            "UpgradeBehavior: install\n"
            "AppsAndFeaturesEntries:\n"
            '  - UpgradeCode: "{A1F35DF8-07F9-400E-92E7-E761EC9BAE37}"\n'
            "Installers:\n"
            "  - Architecture: x64\n"
            f"    InstallerUrl: {REPOSITORY}/releases/download/v{version}/{installer}\n"
            f"    InstallerSha256: {sha256}\n",
        ),
    }
    for suffix, (kind, fields) in manifests.items():
        (directory / f"{PACKAGE}{suffix}.yaml").write_text(
            f"{common}{fields}ManifestType: {kind}\nManifestVersion: {MANIFEST_VERSION}\n",
            encoding="utf-8",
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("artifacts", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    try:
        prepare(args.version, args.artifacts, args.output)
    except (OSError, ValueError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
