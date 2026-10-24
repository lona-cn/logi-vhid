# logi-vhid native SDK

Windows 10 version 1703 or later, x86_64, MSVC. C++ consumers use the same
`extern "C"` ABI as C. The SDK does not install a driver or Logitech G HUB.
Opening a handle requires an available G HUB virtual bus driver; simply loading
the DLL and running the safe examples below does not.

The DLL requires `VCRUNTIME140.dll` from the Microsoft Visual C++ v14 x64
runtime. C/C++ consumers may need additional runtime DLLs for their own build
settings. If needed, install Microsoft's supported
[x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist).
Neither the SDK nor the wheel installs it. A smoke check on a configured build
machine does not establish prerequisites on a clean Windows installation.

## SDK contents

- `bin/logi_vhid.dll`: runtime library; place beside the consuming executable.
- `lib/logi_vhid.dll.lib`: MSVC **DLL import library** used when linking.
- `include/logi_vhid.h`: C ABI, parameters, return values and pointer lifetimes.
- `examples/`: CMake project with safe C and C++ consumers.
- `README.md` and `LICENSE`: this guide and the MIT license.

Link `logi_vhid.dll.lib`, not Cargo's similarly named `logi_vhid.lib` static
library. The release SDK deliberately does not include a static library.
The DLL in this SDK is byte-identical to the DLL bundled in the corresponding
Python wheel; both originate from one explicit x64 MSVC Cargo library build.

The repository root is a virtual Cargo workspace: the native library/demo live
in `crates/logi-vhid`, and the local stdio server lives in
`crates/logi-vhid-mcp`; the source C header remains `include/logi_vhid.h`.
The published SDK layout above and all C consumer names remain unchanged.

源码根目录是虚拟 Cargo workspace：原生库和 demo 位于 `crates/logi-vhid`，
本地 stdio 服务位于 `crates/logi-vhid-mcp`，C 头文件仍在 `include/logi_vhid.h`。
上述 SDK 目录结构及 C 调用方名称不变。

## Independent MCP executable / 独立 MCP 程序

`logi-vhid-mcp-<Cargo version>-windows-x86_64-msvc.exe` is a separate GitHub
Release asset, **not** a member of this SDK ZIP or the Python wheel. It calls
the Rust library directly, not this C ABI. For local stdio configuration and
the six tools, see the [MCP guide](https://github.com/lona-cn/logi-vhid/blob/master/README.md#local-stdio-mcp).
Use an absolute executable path and `args: []` for the default query-only
service. Only with user permission should the host use `args: ["--allow-input"]`;
that startup flag does not replace approval for the exact operation. Screenshots
remain external, and the server does not automatically replace another
computer-control tool. Do not run competing device-input clients in parallel:
the virtual keyboard/mouse state is shared.

`logi-vhid-mcp-<Cargo 版本>-windows-x86_64-msvc.exe` 是独立的 GitHub Release
附件，**不在** SDK ZIP 或 Python wheel 内。它直接调用 Rust 库，不经过 C ABI。
本地 stdio 配置和六个工具见 [MCP 说明](https://github.com/lona-cn/logi-vhid/blob/master/README.md#本地-stdio-mcp)。
默认使用 exe 绝对路径及 `args: []`，仅允许查询；只有用户明确允许输入时，
宿主才应使用 `args: ["--allow-input"]`，启动开关不能代替对具体操作的批准。
截图仍由外部工具负责，也不会自动替换既有 computer 控制工具。虚拟键鼠状态
由客户端共享，请勿并行运行另一套设备输入客户端。

## Build and run safe consumers

Install CMake >=3.20, the Visual Studio C/C++ x64 build tools and Windows SDK.
From an **extracted SDK**, use a developer command prompt with a Visual Studio
generator (the `-A x64` option is not valid for Ninja):

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

For your application, add the SDK's `include` directory to the compiler's include
path, link `lib/logi_vhid.dll.lib`, and place `bin/logi_vhid.dll` beside your
executable. The supplied CMake project expects the extracted SDK layout; running
it directly from a source checkout will not find the packaged `bin/` and `lib/`.

## C API conventions

The C ABI is a subset of the Rust API: it exposes relative mouse movement,
click/double-click, one-key presses, US-layout ASCII input, Win32 Unicode input,
absolute positioning and a PID-gate query. It does not expose Rust's sequences,
layout selection, held-key/button operations, scrolling or window/monitor helpers.

| Function group | Return value / requirement |
| --- | --- |
| `logi_vhid_open()` / `logi_vhid_open_options(keyboard, mouse)` | Owned handle, or `NULL` on failure; flags are 0 for disabled and nonzero for enabled |
| `logi_vhid_close(handle)` | No return value; `NULL` is a no-op; close a non-NULL handle exactly once |
| Input functions | 0 on success, -1 on error (including `NULL` handle) |
| `logi_vhid_pid_gate(handle)` | 1 active, 0 inactive, -1 error; does not change registrations |
| `logi_vhid_device_path(handle)` | Library-owned UTF-8 control path, valid until close; `NULL` handle returns `NULL` without setting an error |
| `logi_vhid_last_error()` | Library-owned UTF-8 text for the calling thread; initially empty, not cleared by success |

Read/copy the error after a reported failure and before the next library error
on that thread. Never modify or free either returned string. All non-NULL
handles must be live, and every operation on the same handle, including queries
and close, must be serialized by the caller.

The virtual keyboard/mouse are shared, not isolated per handle. Opening creates
or accepts existing devices and waits for HID presence, not actual input
delivery. Both flags may be 0 to open only the bus. Unicode and Win32 cursor
modes do not require the corresponding HID flag, but still require a live bus
handle in this API.

Relative `dx`/`dy` are HID counts affected by Windows pointer settings, not
guaranteed pixel distances. Absolute `x`/`y` are physical screen pixels, may be
negative on multiple monitors, and are clamped to the virtual desktop bounds.
Mode 0 uses the virtual mouse and verifies arrival without fallback; mode 1 uses
Win32 `SendInput`, and mode 2 uses `SetCursorPos`. The latter two check API
success, not the final position.

Buttons are 0 left, 1 right, 2 middle, 3 back and 4 forward. Key usages are from
HID Keyboard/Keypad page 0x07, not Win32 virtual-key or character codes.
`logi_vhid_type_ascii` accepts printable ASCII plus tab, CR and LF, uses the US
layout, and collapses CRLF to one Enter. The target layout must already match.
Both text functions accept NUL-terminated UTF-8; `logi_vhid_type_unicode` converts
it to Win32 Unicode events, including surrogate pairs for non-BMP characters.
It does not convert CR/LF/tab into HID Enter/Tab keys. Text buffers must remain
readable and unchanged for the full synchronous call; they are not retained.

## Minimal input example

**This example moves and clicks the mouse on the current desktop.** It is not
one of the packaged safe smoke checks. Run it only when these effects are safe:

```c
#include "logi_vhid.h"
#include <stdio.h>

int main(void) {
    LogiVhidHandle *device = logi_vhid_open_options(0, 1);
    if (device == NULL) {
        fprintf(stderr, "%s\n", logi_vhid_last_error());
        return 1;
    }
    int result = logi_vhid_move(device, 10, -5);
    if (result == 0) {
        result = logi_vhid_click(device, 0);
    }
    if (result != 0) {
        fprintf(stderr, "%s\n", logi_vhid_last_error());
    }
    logi_vhid_close(device);
    return result == 0 ? 0 : 1;
}
```

An input call can fail after partial delivery; already delivered movement, clicks
and text are not undone. Temporary release attempts can also fail. Closing a
handle **only closes the control handle**: it does not send release reports or
remove the virtual devices. This C API has no general release-all function.
Do not rely on close or process exit to recover input left held after a failure.

Rust additionally provides `Sequence::run_controlled`, `Error::Cancelled`,
`Error::TimedOut` and `release_all_inputs()` with separate keyboard/mouse cleanup
results. These are not additions to the C ABI; Rust's existing `release_all()`
still releases keyboard keys only. Cooperative checks cannot interrupt a
synchronous driver I/O call, and neither stop nor forced termination guarantees
release after a blocked call or process failure.

Rust 还提供 `Sequence::run_controlled`、`Error::Cancelled`、`Error::TimedOut`
及分别报告键盘、鼠标清理结果的 `release_all_inputs()`；这些没有扩展到 C ABI。
Rust 原有 `release_all()` 仍只释放键盘。合作式检查不能中断同步驱动 I/O，
调用阻塞或进程异常时，stop 或强杀都不能保证完成释放。

## Build packages from source

The commands above consume an already extracted SDK. To build that ZIP, its
matching Python wheel and the independent MCP executable from a checkout, see the
[release maintenance guide](https://github.com/lona-cn/logi-vhid/blob/master/docs/releasing.md)
([简体中文](https://github.com/lona-cn/logi-vhid/blob/master/docs/releasing.md#简体中文)).
That guide covers the empty output-directory requirement, version/tag mapping,
local verification, manual hardware checks and publishing.
