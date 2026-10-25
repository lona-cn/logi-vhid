"""Release artifact invariants; no compiler, DLL loading, or device access."""
from __future__ import annotations

import hashlib
import importlib.util
import json
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
            '[workspace.package]\nversion = "1.2.3"\n', encoding="utf-8"
        )
        self.dist = self.root / "dist"
        self.dist.mkdir()
        self.sdk = self.dist / "logi-vhid-1.2.3-windows-x86_64-msvc.zip"
        self.wheel = self.dist / "logi_vhid-1.2.3-py3-none-win_amd64.whl"
        self.mcp = self.dist / "logi-vhid-mcp-1.2.3-windows-x86_64-msvc.exe"
        self.bundle = self.dist / "logi-vhid-mcp-1.2.3-windows-x86_64-msvc.mcpb"
        self.registry = self.dist / "server.json"
        self.dll = b"isolated native artifact fixture\x00"
        self.write_sdk()
        self.write_wheel()
        self.mcp.write_bytes(b"isolated MCP executable fixture\x00")
        self.write_bundle()
        self.write_registry()
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

    def bundle_manifest(self):
        repository = "https://github.com/lona-cn/logi-vhid"
        return {
            "manifest_version": "0.3", "name": "logi-vhid-mcp", "version": "1.2.3",
            "description": "Windows desktop input and coordinate queries",
            "author": {"name": "lona-cn"},
            "repository": {"type": "git", "url": repository},
            "homepage": repository,
            "documentation": repository + "/blob/master/crates/logi-vhid-mcp/README.md",
            "support": repository + "/issues", "license": "MIT", "icon": "icon.png",
            "privacy_policies": [repository + "/blob/master/docs/privacy.md"],
            "server": {
                "type": "binary", "entry_point": "server/logi-vhid-mcp.exe",
                "mcp_config": {
                    "command": "${__dirname}/server/logi-vhid-mcp.exe",
                    "args": ["--allow-input=${user_config.allow_input}"],
                },
            },
            "compatibility": {"platforms": ["win32"]},
            "user_config": {"allow_input": {
                "type": "boolean", "title": "Allow keyboard and mouse input",
                "description": "Disabled by default; host approval is still required.",
                "default": False, "required": False,
            }},
        }

    def write_bundle(self, *, manifest_changes=None, extra=None):
        manifest = self.bundle_manifest()
        manifest.update(manifest_changes or {})
        members = {
            "manifest.json": json.dumps(manifest).encode(),
            "server/logi-vhid-mcp.exe": self.mcp.read_bytes(),
            "icon.png": b"isolated icon fixture",
            "LICENSE": b"MIT fixture",
            "README.md": b"MCP-specific documentation fixture",
        }
        members.update(extra or {})
        with zipfile.ZipFile(self.bundle, "w") as archive:
            for name, data in members.items():
                archive.writestr(name, data)

    def write_registry(self, *, package_changes=None):
        package = {
            "registryType": "mcpb",
            "identifier": "https://github.com/lona-cn/logi-vhid/releases/download/v1.2.3/"
                          "logi-vhid-mcp-1.2.3-windows-x86_64-msvc.mcpb",
            "version": "1.2.3",
            "fileSha256": hashlib.sha256(self.bundle.read_bytes()).hexdigest(),
            "transport": {"type": "stdio"},
        }
        package.update(package_changes or {})
        metadata = {
            "$schema": "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json",
            "name": "io.github.lona-cn/logi-vhid-mcp", "title": "logi-vhid MCP",
            "description": "Windows desktop input and coordinate queries",
            "repository": {"url": "https://github.com/lona-cn/logi-vhid",
                           "source": "github", "subfolder": "crates/logi-vhid-mcp"},
            "websiteUrl": "https://github.com/lona-cn/logi-vhid/tree/master/crates/logi-vhid-mcp",
            "version": "1.2.3", "packages": [package],
        }
        self.registry.write_text(json.dumps(metadata), encoding="utf-8")

    def write_checksums(self):
        lines = []
        for artifact in (self.sdk, self.wheel, self.mcp, self.bundle, self.registry):
            digest = hashlib.sha256(artifact.read_bytes()).hexdigest()
            lines.append(f"{digest}  {artifact.name}\n")
        (self.dist / "SHA256SUMS").write_text("".join(lines), encoding="utf-8")

    def verify(self):
        return release.verify_assets("v1.2.3", self.dist, root=self.root)

    def test_matching_assets_are_accepted_without_native_tools(self):
        with (
            patch.object(release.subprocess, "run", side_effect=AssertionError("native tool invoked")),
            patch.object(release.subprocess, "Popen", side_effect=AssertionError("executable started")),
        ):
            self.assertEqual(self.verify(), (self.sdk, self.wheel, self.mcp, self.bundle, self.registry))

    def test_unexpected_corrupt_mcp_bundle_is_not_ignored(self):
        (self.dist / "logi-vhid-mcp-9.9.9-windows-x86_64-msvc.mcpb").write_bytes(
            b"not a valid native MCP bundle"
        )
        with self.assertRaises(ValueError):
            self.verify()

    def test_native_bundle_generation_uses_release_version_and_mcp_readme(self):
        source = self.root / "crates/logi-vhid-mcp"
        source.mkdir(parents=True)
        manifest = self.bundle_manifest()
        manifest["version"] = "0.0.1"
        (source / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        template = json.loads(self.registry.read_text(encoding="utf-8"))
        template.pop("version")
        template.pop("packages")
        template["$comment"] = "Build input only, not a publishable server.json."
        (source / "server.template.json").write_text(json.dumps(template), encoding="utf-8")
        (source / "README.md").write_bytes(b"MCP-specific generated package instructions")
        (source / "icon.png").write_bytes(b"generated icon fixture")
        (self.root / "LICENSE").write_bytes(b"MIT generated fixture")
        release.build_mcp_distribution("1.2.3", self.mcp, self.dist, root=self.root)
        self.write_checksums()
        self.verify()
        with zipfile.ZipFile(self.bundle) as archive:
            self.assertEqual(archive.read("README.md"), b"MCP-specific generated package instructions")
        self.assertNotIn("$comment", json.loads(self.registry.read_text(encoding="utf-8")))

    def test_bundle_missing_or_corrupt_is_rejected(self):
        self.bundle.unlink()
        with self.assertRaises(ValueError):
            self.verify()
        self.bundle.write_bytes(b"not a ZIP")
        self.write_registry()
        self.write_checksums()
        with self.assertRaises(ValueError):
            self.verify()

    def test_bundle_manifest_must_preserve_identity_and_safe_launch(self):
        for change in (
            {"version": "1.2.4"},
            {"description": ""},
            {"license": "Proprietary"},
            {"server": {"type": "node", "entry_point": "server/logi-vhid-mcp.exe",
                        "mcp_config": {"command": "node", "args": []}}},
            {"compatibility": {"platforms": ["linux"]}},
            {"user_config": {"allow_input": {
                "type": "boolean", "title": "Allow input", "description": "Unsafe default",
                "default": True, "required": False}}},
        ):
            with self.subTest(change=change):
                self.write_bundle(manifest_changes=change)
                self.write_registry()
                self.write_checksums()
                with self.assertRaises(ValueError):
                    self.verify()

    def test_bundle_executable_must_match_the_independent_executable(self):
        self.write_bundle(extra={"server/logi-vhid-mcp.exe": b"replaced executable"})
        self.write_registry()
        self.write_checksums()
        with self.assertRaises(ValueError):
            self.verify()

    def test_bundle_rejects_unsafe_or_unexpected_members(self):
        for member in ("../escaped.txt", "SERVER/LOGI-VHID-MCP.EXE", "launcher.py"):
            with self.subTest(member=member):
                self.write_bundle(extra={member: b"unexpected content"})
                self.write_registry()
                self.write_checksums()
                with self.assertRaises(ValueError):
                    self.verify()
        self.assertFalse((self.root / "escaped.txt").exists())

    def test_registry_must_bind_the_exact_bundle_and_stdio_transport(self):
        for change in (
            {"fileSha256": "0" * 64},
            {"identifier": "https://github.com/lona-cn/logi-vhid/releases/download/v1.2.4/other.mcpb"},
            {"transport": {"type": "streamable-http", "url": "https://example.org/mcp"}},
        ):
            with self.subTest(change=change):
                self.write_registry(package_changes=change)
                self.write_checksums()
                with self.assertRaises(ValueError):
                    self.verify()

    def test_registry_missing_corrupt_or_incomplete_is_rejected(self):
        original = self.registry.read_bytes()
        self.registry.unlink()
        with self.assertRaises(ValueError):
            self.verify()
        for payload in (b"{", b"[]", b'{"version":"1.2.3"}'):
            with self.subTest(payload=payload):
                self.registry.write_bytes(payload)
                self.write_checksums()
                with self.assertRaises(ValueError):
                    self.verify()
        self.registry.write_bytes(original)

    def test_registry_rejects_wrong_version_or_missing_static_metadata(self):
        original = json.loads(self.registry.read_text(encoding="utf-8"))
        for key, value in (("version", "1.2.4"), ("repository", None), ("description", "")):
            with self.subTest(field=key):
                metadata = dict(original)
                metadata[key] = value
                self.registry.write_text(json.dumps(metadata), encoding="utf-8")
                self.write_checksums()
                with self.assertRaises(ValueError):
                    self.verify()

    def test_bundle_requires_nonempty_documentation_and_metadata(self):
        for member in ("manifest.json", "LICENSE", "README.md", "icon.png"):
            with self.subTest(member=member):
                self.write_bundle(extra={member: b""})
                self.write_registry()
                self.write_checksums()
                with self.assertRaises(ValueError):
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
                    f'[workspace.package]\nversion = "{cargo}"\n',
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
                    f'[workspace.package]\nversion = "{version}"\n',
                    encoding="utf-8",
                )
                with self.assertRaises(ValueError):
                    release.release_version(f"v{version}", self.root)
                with patch.object(release.subprocess, "run", side_effect=AssertionError("build started")) as run:
                    with self.assertRaises(ValueError):
                        release.build_release(f"v{version}", self.dist, root=self.root)
                    run.assert_not_called()

    def test_prerelease_assets_keep_cargo_version_in_mcp_name(self):
        self.assertEqual(
            release.artifact_names("1.2.3-rc.4", "1.2.3rc4"),
            (
                "logi-vhid-1.2.3-rc.4-windows-x86_64-msvc.zip",
                "logi_vhid-1.2.3rc4-py3-none-win_amd64.whl",
                "logi-vhid-mcp-1.2.3-rc.4-windows-x86_64-msvc.exe",
                "logi-vhid-mcp-1.2.3-rc.4-windows-x86_64-msvc.mcpb",
                "server.json",
            ),
        )

    def test_missing_mcp_executable_is_rejected(self):
        self.mcp.unlink()
        with self.assertRaises(ValueError):
            self.verify()

    def test_extra_mcp_executable_is_rejected(self):
        for name in ("extra.exe", "extra.EXE"):
            with self.subTest(name=name):
                extra = self.dist / name
                extra.write_bytes(b"unexpected executable")
                try:
                    with self.assertRaises(ValueError):
                        self.verify()
                finally:
                    extra.unlink()

    def test_wrongly_named_mcp_executable_is_rejected(self):
        self.mcp.rename(self.dist / "logi-vhid-mcp-1.2.4-windows-x86_64-msvc.exe")
        with self.assertRaises(ValueError):
            self.verify()

    def test_mcp_executable_corruption_is_rejected(self):
        self.mcp.write_bytes(b"changed executable after checksumming")
        with self.assertRaises(ValueError):
            self.verify()

    def test_mcp_checksum_entry_is_required(self):
        checksums = self.dist / "SHA256SUMS"
        lines = checksums.read_text(encoding="utf-8").splitlines(keepends=True)
        checksums.write_text(
            "".join(line for line in lines if not line.endswith(self.mcp.name + "\n")),
            encoding="utf-8",
        )
        with self.assertRaises(ValueError):
            self.verify()

    def test_mcp_executable_symlink_is_rejected(self):
        executable = self.dist / "executable-payload"
        self.mcp.rename(executable)
        try:
            self.mcp.symlink_to(executable)
        except OSError as error:
            self.skipTest(f"symlink creation unavailable: {error}")
        with self.assertRaises(ValueError):
            self.verify()

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
