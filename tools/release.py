"""Build and validate the Windows x64 SDK, wheel, MCP executable and publication assets.

verify-assets deliberately performs no extraction, native loading, or subprocesses.
"""
from __future__ import annotations

import argparse
from email.parser import BytesParser
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
import zipfile

from packaging.tags import Tag
from packaging.utils import canonicalize_name, parse_wheel_filename
from packaging.version import Version

import mcp_smoke

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-pc-windows-msvc"
DLL_MEMBER = "logi_vhid/_native/logi_vhid.dll"
SDK_MEMBERS = (
    "bin/logi_vhid.dll", "lib/logi_vhid.dll.lib", "include/logi_vhid.h",
    "LICENSE", "README.md", "examples/CMakeLists.txt", "examples/smoke.c",
    "examples/smoke.cpp",
)
MCP_SOURCE = Path("crates/logi-vhid-mcp")
MCP_ENTRY = "server/logi-vhid-mcp.exe"
BUNDLE_MEMBERS = {"manifest.json", MCP_ENTRY, "icon.png", "LICENSE", "README.md"}
REPOSITORY = "https://github.com/lona-cn/logi-vhid"
REGISTRY_SCHEMA = "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json"
REGISTRY_NAME = "io.github.lona-cn/logi-vhid-mcp"


def release_version(tag: str, root: Path) -> tuple[str, str]:
    with (root / "Cargo.toml").open("rb") as manifest:
        cargo_version = tomllib.load(manifest)["workspace"]["package"]["version"]
    if tag != f"v{cargo_version}":
        raise ValueError(f"tag must exactly equal v{cargo_version}, got {tag!r}")
    # Match the wheel backend's public release policy before any native build.
    if not re.fullmatch(
        r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
        r"(?:-(alpha|beta|rc)\.(0|[1-9][0-9]*))?",
        cargo_version,
    ):
        raise ValueError(
            f"unsupported Cargo release version {cargo_version!r}; use MAJOR.MINOR.PATCH "
            "or MAJOR.MINOR.PATCH-alpha.N/-beta.N/-rc.N without build metadata"
        )
    return cargo_version, str(Version(cargo_version))


def artifact_names(cargo_version: str, python_version: str) -> tuple[str, str, str, str, str]:
    return (
        f"logi-vhid-{cargo_version}-windows-x86_64-msvc.zip",
        f"logi_vhid-{python_version}-py3-none-win_amd64.whl",
        f"logi-vhid-mcp-{cargo_version}-windows-x86_64-msvc.exe",
        f"logi-vhid-mcp-{cargo_version}-windows-x86_64-msvc.mcpb",
        "server.json",
    )


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def safe_member(name: str) -> bool:
    # No release member needs '~'; reject it to prevent Windows 8.3 alias overwrites.
    if not name or any(ord(character) < 32 or character in '\\:<>\"|?*~' for character in name):
        return False
    parts = (name[:-1] if name.endswith("/") else name).split("/")
    for part in parts:
        if not part or part in (".", "..") or part.endswith((" ", ".")):
            return False
        stem = part.split(".", 1)[0].upper()
        if stem in {"CON", "PRN", "AUX", "NUL"} or re.fullmatch(r"(COM|LPT)[1-9¹²³]", stem):
            return False
    return True


def archive_members(archive: zipfile.ZipFile) -> set[str]:
    names = set()
    windows_names = set()
    for member in archive.infolist():
        windows_name = member.filename.rstrip("/").casefold()
        if not safe_member(member.filename) or windows_name in windows_names:
            raise ValueError(f"unsafe or duplicate archive member: {member.filename!r}")
        windows_names.add(windows_name)
        mode = member.external_attr >> 16
        if stat.S_ISLNK(mode) or (stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR)):
            raise ValueError(f"unsupported archive member type: {member.filename!r}")
        names.add(member.filename)
    return names


def json_object(data: bytes) -> dict:
    try:
        value = json.loads(data)
    except (ValueError, UnicodeDecodeError) as error:
        raise ValueError(f"invalid publication JSON: {error}") from error
    if not isinstance(value, dict):
        raise ValueError("publication JSON must be an object")
    return value


def verify_bundle_manifest(manifest: dict, version: str) -> None:
    expected_server = {
        "type": "binary", "entry_point": MCP_ENTRY,
        "mcp_config": {
            "command": "${__dirname}/" + MCP_ENTRY,
            "args": ["--allow-input=${user_config.allow_input}"],
        },
    }
    if (manifest.get("manifest_version") != "0.3"
            or manifest.get("name") != "logi-vhid-mcp"
            or manifest.get("version") != version
            or manifest.get("server") != expected_server
            or manifest.get("compatibility") != {"platforms": ["win32"]}):
        raise ValueError("MCPB manifest identity, version, binary launch, or platform mismatch")
    user_config = manifest.get("user_config")
    if not isinstance(user_config, dict) or set(user_config) != {"allow_input"}:
        raise ValueError("MCPB must expose only the optional input capability setting")
    allow_input = user_config["allow_input"]
    if (not isinstance(allow_input, dict)
            or allow_input.get("type") != "boolean"
            or allow_input.get("required") is not False
            or allow_input.get("default") is not False
            or not isinstance(allow_input.get("title"), str) or not allow_input["title"].strip()
            or not isinstance(allow_input.get("description"), str) or not allow_input["description"].strip()
            or set(allow_input) - {"type", "title", "description", "required", "default"}):
        raise ValueError("MCPB must default to a disabled optional Boolean input setting")
    expected_metadata = {
        "repository": {"type": "git", "url": REPOSITORY},
        "homepage": REPOSITORY,
        "documentation": REPOSITORY + "/blob/master/crates/logi-vhid-mcp/README.md",
        "support": REPOSITORY + "/issues",
        "license": "MIT", "icon": "icon.png",
        "privacy_policies": [REPOSITORY + "/blob/master/docs/privacy.md"],
    }
    if any(manifest.get(key) != value for key, value in expected_metadata.items()):
        raise ValueError("MCPB license, privacy, documentation, support, or repository metadata mismatch")
    author = manifest.get("author")
    if (not isinstance(manifest.get("description"), str) or not manifest["description"].strip()
            or not isinstance(author, dict) or not isinstance(author.get("name"), str)
            or not author["name"].strip()):
        raise ValueError("MCPB is missing its description or author")


def registry_package(version: str, bundle: Path) -> dict:
    return {
        "registryType": "mcpb",
        "identifier": f"{REPOSITORY}/releases/download/v{version}/{bundle.name}",
        "version": version,
        "fileSha256": sha256(bundle),
        "transport": {"type": "stdio"},
    }


def verify_registry(metadata: dict, version: str, bundle: Path) -> None:
    if (metadata.get("$schema") != REGISTRY_SCHEMA
            or metadata.get("name") != REGISTRY_NAME
            or metadata.get("version") != version
            or metadata.get("repository") != {
                "url": REPOSITORY, "source": "github", "subfolder": MCP_SOURCE.as_posix(),
            }
            or metadata.get("websiteUrl") != REPOSITORY + "/tree/master/crates/logi-vhid-mcp"
            or metadata.get("packages") != [registry_package(version, bundle)]
            or "remotes" in metadata):
        raise ValueError("Registry metadata must identify this version, exact MCPB URL/hash, and stdio")
    for key in ("title", "description"):
        value = metadata.get(key)
        if not isinstance(value, str) or not 1 <= len(value.strip()) <= 100:
            raise ValueError(f"Registry metadata is missing a valid {key}")


def build_mcp_distribution(
    version: str, executable: Path, dist: Path, *, root: Path = ROOT,
) -> tuple[Path, Path]:
    """Package an existing executable; no compilation, execution, or network access."""
    source = Path(root) / MCP_SOURCE
    manifest = json_object((source / "manifest.json").read_bytes())
    manifest["version"] = version
    verify_bundle_manifest(manifest, version)
    _, _, _, bundle_name, registry_name = artifact_names(version, str(Version(version)))
    dist = Path(dist)
    dist.mkdir(parents=True, exist_ok=True)
    bundle, registry = dist / bundle_name, dist / registry_name
    with zipfile.ZipFile(bundle, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest, indent=2) + "\n")
        archive.write(executable, MCP_ENTRY)
        archive.write(source / "icon.png", "icon.png")
        archive.write(Path(root) / "LICENSE", "LICENSE")
        archive.write(source / "README.md", "README.md")
    metadata = json_object((source / "server.template.json").read_bytes())
    metadata.pop("$comment", None)
    metadata["version"] = version
    metadata["packages"] = [registry_package(version, bundle)]
    verify_registry(metadata, version, bundle)
    registry.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return bundle, registry


def verify_checksums(dist: Path, expected: set[str]) -> None:
    entries = {}
    for line in (dist / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-fA-F]{64})  ([^\r\n]+)", line)
        if not match:
            raise ValueError("malformed SHA256SUMS entry")
        digest, name = match.groups()
        if not safe_member(name) or "/" in name or name in entries or name not in expected:
            raise ValueError(f"unexpected or unsafe checksum name: {name!r}")
        entries[name] = digest.lower()
    if set(entries) != expected:
        raise ValueError("SHA256SUMS must list exactly the SDK, wheel, exe, MCPB, and server.json basenames")
    for name, digest in entries.items():
        path = dist / name
        if path.is_symlink() or not path.is_file() or sha256(path) != digest:
            raise ValueError(f"missing artifact or SHA256 mismatch: {name}")


def verify_assets(tag: str, dist: Path, *, root: Path = ROOT) -> tuple[Path, Path, Path, Path, Path]:
    cargo_version, python_version = release_version(tag, Path(root))
    dist = Path(dist).resolve()
    sdk_name, wheel_name, mcp_name, bundle_name, registry_name = artifact_names(cargo_version, python_version)
    for suffix, expected in (
        (".whl", wheel_name), (".zip", sdk_name), (".exe", mcp_name),
        (".mcpb", bundle_name), (".json", registry_name),
    ):
        if sorted(path.name for path in dist.iterdir() if path.suffix.casefold() == suffix) != [expected]:
            raise ValueError(f"expected exactly one {suffix} artifact named {expected}")
    sdk, wheel, mcp, bundle, registry = (
        dist / name for name in (sdk_name, wheel_name, mcp_name, bundle_name, registry_name)
    )
    try:
        verify_checksums(dist, {sdk_name, wheel_name, mcp_name, bundle_name, registry_name})
        with zipfile.ZipFile(bundle) as bundle_zip:
            if archive_members(bundle_zip) != BUNDLE_MEMBERS:
                raise ValueError("MCPB must contain exactly its manifest, executable, icon, license, and README")
            for name in BUNDLE_MEMBERS:
                member = bundle_zip.getinfo(name)
                if member.is_dir() or stat.S_ISDIR(member.external_attr >> 16) or not bundle_zip.read(name):
                    raise ValueError(f"MCPB requires a nonempty regular file: {name}")
            verify_bundle_manifest(json_object(bundle_zip.read("manifest.json")), cargo_version)
            executable = mcp.read_bytes()
            if not executable or bundle_zip.read(MCP_ENTRY) != executable:
                raise ValueError("MCPB and standalone MCP executable must be nonempty and byte-identical")
            if bundle_zip.testzip() is not None:
                raise ValueError("MCPB contains corrupt members")
        verify_registry(json_object(registry.read_bytes()), cargo_version, bundle)
        name, version, build, tags = parse_wheel_filename(wheel.name)
        if (canonicalize_name(name) != "logi-vhid" or str(version) != python_version
                or build or tags != {Tag("py3", "none", "win_amd64")}):
            raise ValueError("wheel filename has wrong project, version, or platform")
        with zipfile.ZipFile(sdk) as sdk_zip, zipfile.ZipFile(wheel) as wheel_zip:
            sdk_members = archive_members(sdk_zip)
            wheel_members = archive_members(wheel_zip)
            if not set(SDK_MEMBERS).issubset(sdk_members):
                raise ValueError("SDK is missing required files")
            info = f"logi_vhid-{python_version}.dist-info"
            metadata_paths = {path for path in wheel_members if path.endswith(".dist-info/METADATA")}
            if metadata_paths != {f"{info}/METADATA"}:
                raise ValueError("wheel contains missing or unexpected distribution metadata")
            required = {"logi_vhid/__init__.py", DLL_MEMBER, f"{info}/WHEEL", f"{info}/RECORD"}
            if not required.issubset(wheel_members):
                raise ValueError("wheel is missing package, native library, or metadata")
            dlls = {path for path in wheel_members if path.lower().endswith(".dll")}
            if dlls != {DLL_MEMBER}:
                raise ValueError("wheel must contain exactly its package-local DLL")
            metadata = BytesParser().parsebytes(wheel_zip.read(f"{info}/METADATA"))
            wheel_metadata = BytesParser().parsebytes(wheel_zip.read(f"{info}/WHEEL"))
            if (metadata.get_all("Name") != ["logi-vhid"]
                    or metadata.get_all("Version") != [python_version]
                    or metadata.get_all("Requires-Python") != [">=3.12"]):
                raise ValueError("wheel project, version, or Python requirement metadata mismatch")
            if (wheel_metadata.get_all("Tag") != ["py3-none-win_amd64"]
                    or wheel_metadata.get_all("Root-Is-Purelib") != ["false"]
                    or wheel_metadata.get_all("Wheel-Version") != ["1.0"]):
                raise ValueError("wheel platform metadata mismatch")
            sdk_dll = sdk_zip.read("bin/logi_vhid.dll")
            if not sdk_dll or sdk_dll != wheel_zip.read(DLL_MEMBER):
                raise ValueError("SDK and wheel DLLs are empty or not byte-identical")
            # Read every member to check each archive's CRC without extracting it.
            for archive in (sdk_zip, wheel_zip):
                if archive.testzip() is not None:
                    raise ValueError("archive contains corrupt members")
    except (OSError, zipfile.BadZipFile, KeyError) as error:
        raise ValueError(f"invalid release assets: {error}") from error
    return sdk, wheel, mcp, bundle, registry


def clean_environment() -> dict[str, str]:
    environment = os.environ.copy()
    for key in ("PYTHONPATH", "PYTHONHOME", "LOGI_VHID_DLL", "LOGI_VHID_NATIVE_DIR"):
        environment.pop(key, None)
    return environment


def require_windows() -> None:
    if sys.platform != "win32":
        raise ValueError("native build/verification requires Windows x64 with MSVC")


def release_notes(version: str) -> str:
    return f"""# logi-vhid {version}

Target: Windows 10 1703+ x86_64, MSVC ({TARGET}). Python >=3.12.

Assets: native C/C++ SDK ZIP, ctypes py3-none-win_amd64 wheel, versioned
logi-vhid-mcp Windows x64 executable, native MCPB, and Registry server.json.
SHA256SUMS covers exactly these five assets. The SDK and wheel contain the same
DLL; the MCPB contains the exact standalone executable that the safe smoke runs.
The MCPB needs no Node/Python launcher. Its optional input argument list defaults
to empty (input disabled); only an explicit --allow-input item enables capability.
The host must still obtain approval for each operation. Registry metadata names
the exact release MCPB URL and SHA-256; publication and namespace authorization
remain manual and are not claimed by creating these assets.
This distribution does not include a driver or G HUB installer.
The native DLL requires the Microsoft Visual C++ v14 x64 runtime, not bundled here.
This gate does not claim validation on a clean Windows installation.
Input calls affect the current desktop. Handles share virtual devices; closing
one does not send key/button release reports or remove those devices.

## Acceptance before publication

The automatic release gate verifies artifact metadata, paths, checksums,
DLL/executable equality, MCPB safe defaults, and Registry package binding;
on Windows it installs the wheel in a fresh environment and
compiles/runs C and C++ SDK consumers. The C/C++ and wheel smoke checks use only
null-handle and error-query paths. The MCP safe stdio smoke starts without
--allow-input, performs initialize/discovery and desktop information queries,
verifies that a valid key request is rejected with input_disabled and no
submission, calls stop without an input device, then closes stdin and checks
normal shutdown. No smoke check initializes HID devices or submits actual input.
Desktop information may explicitly report that no interactive desktop is available.

Before publishing, the maintainer must perform the intended hardware acceptance
on a suitable Windows machine and record the Windows version, G HUB version,
actual keyboard/mouse input observations, and release/cleanup observations in
the release description. No hardware acceptance result is claimed by this draft.
"""


def build_release(tag: str, dist: Path, *, root: Path = ROOT) -> None:
    root = Path(root).resolve()
    cargo_version, python_version = release_version(tag, root)
    require_windows()
    dist = Path(dist).resolve()
    if dist.exists() and any(dist.iterdir()):
        raise ValueError("build requires an empty distribution directory")
    dist.mkdir(parents=True, exist_ok=True)
    environment = clean_environment()
    metadata = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=root, env=environment, check=True, capture_output=True, text=True,
    )
    target_directory = Path(json.loads(metadata.stdout)["target_directory"])
    if not target_directory.is_absolute():
        raise ValueError("cargo metadata target_directory must be absolute")
    subprocess.run(
        ["cargo", "build", "--locked", "--release", "--target", TARGET, "-p", "logi-vhid", "--lib"],
        cwd=root, env=environment, check=True,
    )
    subprocess.run(
        ["cargo", "build", "--locked", "--release", "--target", TARGET,
         "-p", "logi-vhid-mcp", "--bin", "logi-vhid-mcp"],
        cwd=root, env=environment, check=True,
    )
    native = target_directory / TARGET / "release"
    dll, import_library = native / "logi_vhid.dll", native / "logi_vhid.dll.lib"
    if not dll.is_file() or not import_library.is_file():
        raise ValueError("Cargo did not produce logi_vhid.dll and its .dll.lib import library")
    mcp = native / "logi-vhid-mcp.exe"
    if not mcp.is_file():
        raise ValueError("Cargo did not produce logi-vhid-mcp.exe")
    names = artifact_names(cargo_version, python_version)
    sdk_name, wheel_name, mcp_name, bundle_name, registry_name = names
    with tempfile.TemporaryDirectory(prefix="logi-vhid-build-", dir=dist.parent) as temporary:
        staging = Path(temporary)
        shutil.copy2(mcp, staging / mcp_name)
        build_mcp_distribution(cargo_version, staging / mcp_name, staging, root=root)
        with zipfile.ZipFile(staging / sdk_name, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.write(dll, "bin/logi_vhid.dll")
            archive.write(import_library, "lib/logi_vhid.dll.lib")
            archive.write(root / "include/logi_vhid.h", "include/logi_vhid.h")
            archive.write(root / "LICENSE", "LICENSE")
            archive.write(root / "sdk/README.md", "README.md")
            for member in ("CMakeLists.txt", "smoke.c", "smoke.cpp"):
                archive.write(root / "sdk/examples" / member, f"examples/{member}")
        wheel_environment = environment.copy()
        wheel_environment["LOGI_VHID_NATIVE_DIR"] = str(native)
        subprocess.run(
            [sys.executable, "-m", "build", "--wheel", "--no-isolation", "--outdir", str(staging)],
            cwd=root, env=wheel_environment, check=True,
        )
        (staging / "SHA256SUMS").write_text(
            "".join(f"{sha256(staging / name)}  {name}\n" for name in names),
            encoding="utf-8",
        )
        (staging / "RELEASE_NOTES.md").write_text(release_notes(cargo_version), encoding="utf-8")
        verify_assets(tag, staging, root=root)
        for name in (*names, "SHA256SUMS", "RELEASE_NOTES.md"):
            shutil.move(str(staging / name), str(dist / name))


PYTHON_SMOKE = r"""
import pathlib
import sys
import logi_vhid
package = pathlib.Path(logi_vhid.__file__).resolve()
assert package.is_relative_to(pathlib.Path(sys.prefix).resolve()), package
expected = package.parent / '_native' / 'logi_vhid.dll'
assert pathlib.Path(logi_vhid._lib._name).resolve() == expected.resolve()
assert logi_vhid._lib.logi_vhid_last_error() is not None
assert logi_vhid._lib.logi_vhid_pid_gate(None) == -1
assert logi_vhid._lib.logi_vhid_last_error()
print('Installed wheel bundled-DLL safe smoke passed')
"""


def verify_release(tag: str, dist: Path, *, root: Path = ROOT) -> None:
    sdk, wheel, mcp, bundle, registry = verify_assets(tag, dist, root=root)
    require_windows()
    environment = clean_environment()
    with tempfile.TemporaryDirectory(prefix="logi-vhid-verify-") as temporary:
        workspace = Path(temporary)
        if workspace.resolve().is_relative_to(Path(root).resolve()):
            raise ValueError("verification temporary directory must be outside the source checkout")
        extracted = workspace / "sdk"
        extracted.mkdir()
        with zipfile.ZipFile(sdk) as archive:
            # Repeat path/type validation immediately before extraction.
            archive_members(archive)
            archive.extractall(extracted)
        native_build = workspace / "native-build"
        for command in (
            ["cmake", "-S", str(extracted / "examples"), "-B", str(native_build), "-A", "x64"],
            ["cmake", "--build", str(native_build), "--config", "Release"],
            ["ctest", "--test-dir", str(native_build), "-C", "Release", "--output-on-failure"],
        ):
            subprocess.run(command, cwd=workspace, env=environment, check=True)
        venv = workspace / "venv"
        subprocess.run(
            [sys.executable, "-I", "-m", "venv", str(venv)],
            cwd=workspace, env=environment, check=True,
        )
        python = venv / "Scripts/python.exe"
        subprocess.run(
            [str(python), "-I", "-m", "pip", "--isolated", "install", "--no-index", "--no-deps", str(wheel)],
            cwd=workspace, env=environment, check=True,
        )
        subprocess.run(
            [str(python), "-I", "-c", PYTHON_SMOKE], cwd=workspace, env=environment, check=True,
        )
        mcp_smoke.run_smoke(mcp, env=environment, cwd=workspace)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("build", "verify", "verify-assets"))
    parser.add_argument("--tag", required=True)
    parser.add_argument("--dist", required=True, type=Path)
    arguments = parser.parse_args(argv)
    commands = {"build": build_release, "verify": verify_release, "verify-assets": verify_assets}
    try:
        commands[arguments.command](arguments.tag, arguments.dist)
    except (ValueError, OSError, subprocess.CalledProcessError, mcp_smoke.SmokeError) as error:
        print(f"release: {error}", file=sys.stderr)
        return 1
    print(f"{arguments.command}: OK")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
