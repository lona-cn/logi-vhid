"""Native packaging boundaries; no compiler or DLL loading is used."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

from setuptools.errors import SetupError


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("logi_vhid_build", ROOT / "setup.py")
BUILD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILD)


class VersionTests(unittest.TestCase):
    def test_public_release_versions_are_normalized(self):
        for cargo, expected in (
            ("1.2.3", "1.2.3"),
            ("1.2.3-alpha.1", "1.2.3a1"),
            ("1.2.3-beta.2", "1.2.3b2"),
            ("1.2.3-rc.4", "1.2.3rc4"),
        ):
            with self.subTest(cargo=cargo):
                self.assertEqual(BUILD.python_version(cargo), expected)

    def test_non_public_or_ambiguous_versions_are_rejected(self):
        for cargo in ("1.2", "01.2.3", "1.2.3+local", "1.2.3-dev.1", "1.2.3-preview.1", "1.2.3-rc.01"):
            with self.subTest(cargo=cargo):
                with self.assertRaises(SetupError):
                    BUILD.python_version(cargo)


class NativePackagingTests(unittest.TestCase):

    def test_invalid_prebuilt_directories_fail_instead_of_building(self):
        with tempfile.TemporaryDirectory() as directory:
            for supplied in ("relative-directory", directory, ""):
                with self.subTest(supplied=supplied):
                    with mock.patch.dict(os.environ, {"LOGI_VHID_NATIVE_DIR": supplied}):
                        with mock.patch.object(BUILD.subprocess, "run", side_effect=AssertionError("Cargo must not run")):
                            with self.assertRaises(SetupError):
                                BUILD.native_library()

    def test_source_build_selects_cargo_metadata_target_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            target_directory = Path(directory) / "externally-configured-target"
            library = target_directory / "x86_64-pc-windows-msvc" / "release" / "logi_vhid.dll"
            library.parent.mkdir(parents=True)
            library.write_bytes(b"controlled source build output")
            environment = {key: value for key, value in os.environ.items()
                           if key != "LOGI_VHID_NATIVE_DIR"}

            def cargo(args, **kwargs):
                if args == ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"]:
                    return subprocess.CompletedProcess(args, 0, json.dumps({
                        "target_directory": str(target_directory),
                    }))
                if args == ["cargo", "build", "--locked", "--release", "--target", "x86_64-pc-windows-msvc", "--lib"]:
                    return subprocess.CompletedProcess(args, 0)
                raise AssertionError(f"Unexpected compiler invocation: {args}")

            with mock.patch.dict(os.environ, environment, clear=True):
                with mock.patch.object(BUILD.sys, "platform", "win32"):
                    with mock.patch.object(BUILD.platform, "machine", return_value="AMD64"):
                        with mock.patch.object(BUILD.subprocess, "run", side_effect=cargo):
                            self.assertEqual(BUILD.native_library(), library)

    def test_unsupported_source_build_host_fails_before_cargo(self):
        environment = {key: value for key, value in os.environ.items()
                       if key != "LOGI_VHID_NATIVE_DIR"}
        with mock.patch.dict(os.environ, environment, clear=True):
            with mock.patch.object(BUILD.sys, "platform", "linux"):
                with mock.patch.object(BUILD.subprocess, "run", side_effect=AssertionError("Cargo must not run")):
                    with self.assertRaises(SetupError):
                        BUILD.native_library()


if __name__ == "__main__":
    unittest.main()
