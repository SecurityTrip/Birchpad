import tempfile
import unittest
from pathlib import Path

from winget_manifest import prepare


class ManifestTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.artifacts = Path(temporary.name)
        self.output = self.artifacts / "submission"

    def test_stable_release_uses_download_url_and_actual_msi_hash(self):
        version = "0.8.2"
        installer = f"birchpad-{version}-windows-x64.msi"
        (self.artifacts / installer).write_bytes(b"abc")
        prepare(version, self.artifacts, self.output)
        directory = self.output / "manifests/s/SecurityTrip/Birchpad" / version
        files = {file.name: file.read_text() for file in directory.iterdir()}
        self.assertEqual(set(files), {
            "SecurityTrip.Birchpad.yaml",
            "SecurityTrip.Birchpad.locale.en-US.yaml",
            "SecurityTrip.Birchpad.installer.yaml",
        })
        for manifest in files.values():
            self.assertIn('PackageVersion: "0.8.2"\n', manifest)
        manifest = files["SecurityTrip.Birchpad.installer.yaml"]
        self.assertIn(f"/releases/download/v0.8.2/{installer}\n", manifest)
        self.assertIn(
            "InstallerSha256: BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD\n",
            manifest,
        )

    def test_prerelease_and_malformed_versions_write_nothing(self):
        for version in ("0.8.2-nightly.1", "0.8.2-rc.1", "00.8.2", "0.8.2\n", "0.8"):
            with self.subTest(version=version), self.assertRaises(ValueError):
                prepare(version, self.artifacts, self.output)
        self.assertFalse(self.output.exists())

    def test_missing_and_empty_msi_do_not_produce_manifests(self):
        with self.assertRaises(FileNotFoundError):
            prepare("0.0.0", self.artifacts, self.output)
        (self.artifacts / "birchpad-0.0.0-windows-x64.msi").touch()
        with self.assertRaisesRegex(ValueError, "empty"):
            prepare("0.0.0", self.artifacts, self.output)
        self.assertFalse(self.output.exists())
