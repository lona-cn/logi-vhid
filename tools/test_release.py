"""Release artifact invariants; no compiler, DLL loading, or device access."""
from __future__ import annotations

import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile


SPEC = importlib.util.spec_from_file_location("release_tool", Path(__file__).with_name("release.py"))
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "Cargo.toml").write_text(
            '[package]\nname = "logi-vhid"\nversion = "1.2.3"\n', encoding="utf-8"
        )
        self.dist = self.root / "dist"
        self.dist.mkdir()
        self.sdk = self.dist / "logi-vhid-1.2.3-windows-x86_64-msvc.zip"
        self.wheel = self.dist / "logi_vhid-1.2.3-py3-none-win_amd64.whl"
        self.dll = b"isolated native artifact fixture\x00"
        self.write_sdk()
        self.write_wheel()
        self.write_checksums()

    def write_sdk(self, extra=None):
        members = {
            "bin/logi_vhid.dll": self.dll,
            "lib/logi_vhid.dll.lib": b"import library fixture",
            "include/logi_vhid.h": b"/* header fixture */",
            "LICENSE": b"MIT fixture",
            "README.md": b"SDK fixture",
            "examples/CMakeLists.txt": b"# fixture",
            "examples/smoke.c": b"/* fixture */",
            "examples/smoke.cpp": b"/* fixture */",
        }
        members.update(extra or {})
        with zipfile.ZipFile(self.sdk, "w") as archive:
            for name, data in members.items():
                archive.writestr(name, data)

    def write_wheel(self, *, version="1.2.3", platform="win_amd64", dll=None):
        info = f"logi_vhid-{version}.dist-info"
        members = {
            "logi_vhid/__init__.py": b"# isolated package fixture\n",
            "logi_vhid/_native/logi_vhid.dll": self.dll if dll is None else dll,
            f"{info}/METADATA": f"Metadata-Version: 2.3\nName: logi-vhid\nVersion: {version}\nRequires-Python: >=3.12\n\n".encode(),
            f"{info}/WHEEL": f"Wheel-Version: 1.0\nGenerator: test-fixture\nRoot-Is-Purelib: false\nTag: py3-none-{platform}\n\n".encode(),
            f"{info}/RECORD": b"",
        }
        with zipfile.ZipFile(self.wheel, "w") as archive:
            for name, data in members.items():
                archive.writestr(name, data)

    def write_checksums(self):
        lines = []
        for artifact in (self.sdk, self.wheel):
            digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
            lines.append(f"{digest}  {artifact.name}\n")
        (self.dist / "SHA256SUMS").write_text("".join(lines), encoding="utf-8")

    def verify(self):
        return release.verify_assets("v1.2.3", self.dist, root=self.root)

    def test_matching_assets_are_accepted_without_native_tools(self):
        with patch.object(release.subprocess, "run", side_effect=AssertionError("native tool invoked")):
            self.verify()

    def test_tag_mismatch_is_rejected_before_any_build_process(self):
        with self.assertRaises(ValueError):
            release.release_version("v9.9.9", self.root)
        with patch.object(release.subprocess, "run", side_effect=AssertionError("build started")) as run:
            with self.assertRaises(ValueError):
                release.build_release("v9.9.9", self.dist, root=self.root)
            run.assert_not_called()

    def test_supported_cargo_prereleases_use_normalized_python_versions(self):
        for cargo, python in (
            ("1.2.3-alpha.0", "1.2.3a0"),
            ("1.2.3-beta.2", "1.2.3b2"),
            ("1.2.3-rc.4", "1.2.3rc4"),
        ):
            with self.subTest(version=cargo):
                (self.root / "Cargo.toml").write_text(
                    f'[package]\nname = "logi-vhid"\nversion = "{cargo}"\n',
                    encoding="utf-8",
                )
                self.assertEqual(
                    release.release_version(f"v{cargo}", self.root), (cargo, python)
                )

    def test_unsupported_release_versions_are_rejected_before_build(self):
        for version in (
            "1.2.3-preview.1", "1.2.3-alpha", "1.2.3-alpha.01",
            "01.2.3", "1.2.3+build.1", "1.2.3-dev.1", "1.2.3-post.1",
        ):
            with self.subTest(version=version):
                (self.root / "Cargo.toml").write_text(
                    f'[package]\nname = "logi-vhid"\nversion = "{version}"\n',
                    encoding="utf-8",
                )
                with self.assertRaises(ValueError):
                    release.release_version(f"v{version}", self.root)
                with patch.object(release.subprocess, "run", side_effect=AssertionError("build started")) as run:
                    with self.assertRaises(ValueError):
                        release.build_release(f"v{version}", self.dist, root=self.root)
                    run.assert_not_called()

    def test_artifact_corruption_is_rejected(self):
        with self.sdk.open("ab") as artifact:
            artifact.write(b"corruption after checksumming")
        with self.assertRaises(ValueError):
            self.verify()

    def test_checksum_manifest_rejects_non_basename_paths(self):
        original = (self.dist / "SHA256SUMS").read_text(encoding="utf-8")
        for unsafe in ("../" + self.sdk.name, "/" + self.sdk.name, "C:\\outside\\" + self.sdk.name, "sub/" + self.sdk.name, "sub\\" + self.sdk.name):
            with self.subTest(path=unsafe):
                (self.dist / "SHA256SUMS").write_text(original.replace(self.sdk.name, unsafe), encoding="utf-8")
                with self.assertRaises(ValueError):
                    self.verify()

    def test_checksum_manifest_rejects_duplicate_and_missing_entries(self):
        original = (self.dist / "SHA256SUMS").read_text(encoding="utf-8")
        for invalid in (original + original.splitlines()[0] + "\n", original.splitlines()[0] + "\n", "not-a-hash  " + self.sdk.name + "\n"):
            with self.subTest(manifest=invalid):
                (self.dist / "SHA256SUMS").write_text(invalid, encoding="utf-8")
                with self.assertRaises(ValueError):
                    self.verify()

    def test_wheel_metadata_version_mismatch_is_rejected(self):
        self.write_wheel(version="1.2.4")
        self.write_checksums()
        with self.assertRaises(ValueError):
            self.verify()

    def test_wheel_platform_mismatch_is_rejected(self):
        self.write_wheel(platform="any")
        self.write_checksums()
        with self.assertRaises(ValueError):
            self.verify()

    def test_wheel_and_sdk_dll_must_be_identical(self):
        self.write_wheel(dll=b"a different DLL artifact")
        self.write_checksums()
        with self.assertRaises(ValueError):
            self.verify()

    def test_unexpected_second_wheel_is_rejected(self):
        (self.dist / "logi_vhid-1.2.3-py3-none-any.whl").write_bytes(self.wheel.read_bytes())
        with self.assertRaises(ValueError):
            self.verify()

    def test_sdk_archive_traversal_is_rejected_without_extracting(self):
        self.write_sdk({"../escaped.txt": b"must not extract"})
        self.write_checksums()
        with self.assertRaises(ValueError):
            self.verify()
        self.assertFalse((self.root / "escaped.txt").exists())

    def test_sdk_case_alias_cannot_replace_the_dll_on_windows(self):
        self.write_sdk({"BIN/LOGI_VHID.DLL": b"unchecked replacement DLL"})
        self.write_checksums()
        with self.assertRaises(ValueError):
            self.verify()

    def test_archive_paths_reject_windows_aliases_and_device_names(self):
        for unsafe in (
            "bin./other.dll", "bin/other.dll ", "aux.txt", "folder//",
            "bin/LOGI_V~1.DLL",
        ):
            with self.subTest(path=unsafe):
                self.write_sdk({unsafe: b"unsafe Windows path"})
                self.write_checksums()
                with self.assertRaises(ValueError):
                    self.verify()

    def test_wheel_rejects_windows_control_characters_and_device_names(self):
        for unsafe in (
            "logi_vhid/extra\x01.py", "logi_vhid/extra\x1f.py",
            "logi_vhid/COM¹.log", "logi_vhid/LPT³.log",
        ):
            with self.subTest(path=unsafe):
                self.write_wheel()
                with zipfile.ZipFile(self.wheel, "a") as archive:
                    archive.writestr(unsafe, b"unsafe Windows path")
                self.write_checksums()
                with self.assertRaises(ValueError):
                    self.verify()


if __name__ == "__main__":
    unittest.main()
