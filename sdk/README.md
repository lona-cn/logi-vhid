# logi-vhid native SDK

This SDK is for Windows 10 1703 or later, x86_64, using MSVC. It contains the
native DLL and C ABI header; C++ consumers use the same `extern "C"` API.
The SDK does not install a driver or Logitech G HUB. Actual input operations
require a suitable Logitech G HUB environment and separate hardware acceptance.

The DLL requires `VCRUNTIME140.dll` from the Microsoft Visual C++ v14 x64
runtime. C/C++ consumers may require additional runtime DLLs for their own build
settings. Install the supported x64 Redistributable appropriate for the build
toolset when it is not already present; the SDK and wheel do not bundle or
install it. See Microsoft's [supported VC++ runtime downloads](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
The automated gate runs on its configured Windows build machine; it does not
establish that an otherwise clean Windows installation has all prerequisites.

## SDK contents

- `bin/logi_vhid.dll`: runtime library; place beside the consuming executable.
- `lib/logi_vhid.dll.lib`: MSVC **DLL import library** used when linking.
- `include/logi_vhid.h`: C ABI, handle ownership, concurrency and error contracts.
- `examples/`: CMake project with safe C and C++ consumers.
- `LICENSE`: MIT license.

Link `logi_vhid.dll.lib`, not Cargo's similarly named `logi_vhid.lib` static
library. The release SDK deliberately does not include a static library.
The DLL in this SDK is byte-identical to the DLL bundled in the corresponding
Python wheel; both originate from one explicit x64 MSVC Cargo library build.

## Build and run safe consumers

Install CMake and the Visual Studio C/C++ x64 tools. From an extracted SDK,
using a developer command prompt:

```console
cmake -S examples -B native-build -A x64
cmake --build native-build --config Release
ctest --test-dir native-build -C Release --output-on-failure
```

CMake links the import library and copies the runtime DLL beside both
executables. The examples only call `logi_vhid_last_error()` and
`logi_vhid_pid_gate(NULL)` (or `nullptr`). They check the documented null-handle
error behavior. They never open a device, create a keyboard/mouse handle, move
the cursor, or send input. Passing these tests proves SDK linkage and safe ABI
execution, **not** physical keyboard/mouse behavior or G HUB compatibility.

For a consumer project, include `logi_vhid.h`, link the x64 import library, and
ship the DLL beside your executable. Follow the header's handle ownership and
serialization rules. Error strings belong to the DLL and must not be freed.

## Release maintenance (source checkout only)

Use Python >=3.12 in an isolated environment and install
`tools/requirements-release.txt` there. With Rust's
`x86_64-pc-windows-msvc` target and the MSVC build tools installed, run from the
repository root (replace the tag with `v` plus the exact Cargo package version):

```console
python tools/release.py build --tag v0.1.0 --dist dist
python tools/release.py verify --tag v0.1.0 --dist dist
```

`build` requires an empty output directory. It validates the tag before invoking
Cargo, obtains Cargo's actual target directory through metadata, builds the DLL
once with `cargo build --locked --release --target x86_64-pc-windows-msvc --lib`,
and passes the existing DLL directory to the wheel backend through
`LOGI_VHID_NATIVE_DIR`. The output contains one SDK ZIP, one
`py3-none-win_amd64` wheel, `SHA256SUMS`, and draft `RELEASE_NOTES.md`.

The tag must be exactly `v` followed by the Cargo package version. Supported
versions are `MAJOR.MINOR.PATCH` and optional `-alpha.N`, `-beta.N` or `-rc.N`;
numeric components have no leading zeros, and build metadata is not supported.
Wheel metadata uses the corresponding normalized Python version.
The draft notes become the Release body; the downloadable published assets are
the SDK ZIP, wheel and `SHA256SUMS`. Asset verification does not require a
separate notes file.

`verify` first validates the assets, then extracts the SDK into an owned temporary
directory, compiles and runs both consumers, and installs the wheel into a fresh
venv with `--no-index --no-deps`. It imports outside the source checkout with
`PYTHONPATH` and `LOGI_VHID_DLL` removed, checks that the bundled DLL was loaded,
and exercises only the same safe error paths. Temporary files are cleaned when
the subprocesses finish; no hardware operations are part of this gate.

For inspection on Windows or Linux without compiling or loading any DLL:

```console
python tools/release.py verify-assets --tag v0.1.0 --dist dist
```

This checks artifact names, archive paths, wheel metadata, the two manifest
hashes and DLL equality. `SHA256SUMS` contains only the SDK and wheel basenames,
never directory paths. Checksums establish integrity against that manifest;
they are not a signature or proof that an unknown publisher is trustworthy.

Before publishing, perform and record manual hardware acceptance: Windows and
G HUB versions, actual keyboard/mouse observations, and release/cleanup behavior.
The generated notes explicitly do not claim that those manual checks passed.
