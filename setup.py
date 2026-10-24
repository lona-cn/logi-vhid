"""Setuptools glue for a standard-library ctypes package and its native DLL."""

import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tomllib

from packaging.version import Version
from setuptools import Distribution, setup
from setuptools.command.bdist_wheel import bdist_wheel
from setuptools.command.build_py import build_py
from setuptools.errors import SetupError


ROOT = Path(__file__).resolve().parent
TARGET = "x86_64-pc-windows-msvc"
DLL_NAME = "logi_vhid.dll"


def python_version(cargo_version):
    """Accept release versions with an unambiguous public-index equivalent."""
    if not re.fullmatch(
        r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
        r"(?:-(alpha|beta|rc)\.(0|[1-9][0-9]*))?",
        cargo_version,
    ):
        raise SetupError(
            f"Unsupported Cargo release version {cargo_version!r}; use MAJOR.MINOR.PATCH "
            "or MAJOR.MINOR.PATCH-alpha.N/-beta.N/-rc.N without build metadata."
        )
    return str(Version(cargo_version))


def native_library():
    """Select a prebuilt DLL, or perform one supported Windows source build."""
    supplied = os.environ.get("LOGI_VHID_NATIVE_DIR")
    if supplied is not None:
        directory = Path(supplied)
        if not directory.is_absolute():
            raise SetupError("LOGI_VHID_NATIVE_DIR must be an absolute directory")
        library = directory / DLL_NAME
        if not library.is_file():
            raise SetupError(f"LOGI_VHID_NATIVE_DIR does not contain {DLL_NAME}: {directory}")
        return library

    if sys.platform != "win32" or platform.machine().lower() not in ("amd64", "x86_64"):
        raise SetupError(
            "Native source builds require Windows x86_64 and the MSVC Rust toolchain; "
            "install the Windows x86_64 wheel or set LOGI_VHID_NATIVE_DIR to a prebuilt DLL directory."
        )
    try:
        metadata = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        target_directory = Path(json.loads(metadata.stdout)["target_directory"])
        subprocess.run(
            ["cargo", "build", "--locked", "--release", "--target", TARGET, "-p", "logi-vhid", "--lib"],
            cwd=ROOT,
            check=True,
        )
    except FileNotFoundError as error:
        raise SetupError("Cargo is required for source installation; install the MSVC Rust toolchain.") from error
    except subprocess.CalledProcessError as error:
        raise SetupError(f"Cargo command failed while building {DLL_NAME}: {error}") from error
    library = target_directory / TARGET / "release" / DLL_NAME
    if not library.is_file():
        raise SetupError(f"Cargo build did not produce the expected DLL: {library}")
    return library


class NativeBuildPy(build_py):
    def run(self):
        super().run()
        library = native_library()
        destination = Path(self.build_lib) / "logi_vhid" / "_native" / DLL_NAME
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(library, destination)

    def get_outputs(self, include_bytecode=True):
        outputs = super().get_outputs(include_bytecode)
        outputs.append(str(Path(self.build_lib) / "logi_vhid" / "_native" / DLL_NAME))
        return outputs


class NativeDistribution(Distribution):
    def has_ext_modules(self):
        return True


class WindowsWheel(bdist_wheel):
    def finalize_options(self):
        super().finalize_options()
        self.root_is_pure = False

    def get_tag(self):
        return "py3", "none", "win_amd64"


if __name__ == "__main__":
    with (ROOT / "Cargo.toml").open("rb") as manifest:
        cargo_version = tomllib.load(manifest)["workspace"]["package"]["version"]
    setup(
        version=python_version(cargo_version),
        distclass=NativeDistribution,
        cmdclass={
            "build_py": NativeBuildPy,
            "bdist_wheel": WindowsWheel,
        },
    )
