# logi-vhid

[English](#english) · [简体中文](#简体中文)

## English

A Windows x64 Rust library for keyboard and mouse input through Logitech G HUB's virtual HID bus. It also provides a CLI, a C ABI, a Python `ctypes` package and a local stdio MCP server. The driver and G HUB installer are not included.

[Rust](#rust) · [CLI](#command-line) · [MCP](#local-stdio-mcp) · [C/C++](#cc-sdk) · [Python](#python) · [Coordinates](#coordinates-and-input-modes) · [Troubleshooting](#troubleshooting) · [Testing](#testing)

### Requirements

- Windows 10 version 1703 or later, x64, with an interactive desktop for input operations.
- The Logitech G HUB virtual bus driver must be installed and available when opening a device. Compatibility with every G HUB version is not established.
- Building from source requires Rust >=1.96, the x64 MSVC toolchain, Visual Studio C++ build tools and the Windows SDK. The commands below assume an active `x86_64-pc-windows-msvc` toolchain.
- Python requires Python >=3.12, Windows x64 and the Microsoft Visual C++ v14 x64 runtime. Prebuilt wheels need no Rust toolchain.

The native DLL requires `VCRUNTIME140.dll`. If it is missing, install Microsoft's supported [x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist). Neither the SDK nor the wheel installs system components.

**Input examples affect the current desktop.** Choose a safe foreground application before running them. Opening a device creates/verifies shared virtual HID devices; it is not a read-only query. Closing a handle does not automatically release held keys/buttons or remove those devices.

### Rust

The root `Cargo.toml` is a virtual workspace: `crates/logi-vhid` contains the library and demo, and `crates/logi-vhid-mcp` contains the server. They share the root `Cargo.lock`, version and target directory; the C header stays at `include/logi_vhid.h`.

From a consumer project, use a local checkout without depending on a published release (adjust the path to your checkout):

```powershell
cargo add logi-vhid --path ../logi-vhid/crates/logi-vhid
```

For a version available on crates.io, use `cargo add logi-vhid` instead. From this repository, build the library and CLI and generate the complete API reference:

```powershell
cargo build -p logi-vhid --release
cargo doc -p logi-vhid --no-deps --open
```

The outputs include `logi-vhid-demo.exe`, `logi_vhid.dll`, its DLL import library `logi_vhid.dll.lib`, and the separate Rust static library `logi_vhid.lib`. The default directory is `target/release/`; Cargo configuration, `CARGO_TARGET_DIR` and an explicit `--target` change it. Use `cargo metadata --format-version 1 --no-deps` to find the effective `target_directory`; an explicit target adds its target-triple subdirectory.

This example **moves/clicks the mouse and sends keyboard input**:

```rust
use logi_vhid::{
    hid, AbsoluteMode, KeyboardLayout, LogitechVirtualHid, MouseButton, TextMode,
};

fn main() -> logi_vhid::Result<()> {
    let mut device = LogitechVirtualHid::open()?;

    device.mouse_move_relative(10, -5)?;
    device.click_at(400, 300, MouseButton::Left, AbsoluteMode::Driver)?;
    device.press_combo(&[hid::LEFT_CTRL, hid::C])?;
    device.type_text_layout("hello", KeyboardLayout::Us)?;
    device.type_text("中文", TextMode::Win32Unicode)?;

    Ok(())
}
```

`open()` selects both devices. `open_with_options(OpenOptions::keyboard_only())` or `OpenOptions::mouse_only()` selects one. Each selected child is checked for HID presence for up to two seconds; this does not prove input delivery.

Rust provides key down/up, combinations, holds, repeats, layout-aware text, mouse buttons, scrolling, dragging, `Sequence`, monitor/window queries and coordinate conversion. C and Python expose only a subset; use their own references rather than assuming every Rust method is available.

`sequence.run_controlled(&mut device, &checkpoint)` adds cooperative cancellation/deadlines through a caller-supplied checkpoint returning `Result<()>`. Use `Error::Cancelled` or `Error::TimedOut` to stop at a safe boundary. Its `ExecutionFailure` includes the cause and per-device `ReleaseReport` cleanup results; failure cleanup preserves inputs held before the sequence unless explicitly released.

`device.release_all_inputs()` attempts neutral reports for every enabled keyboard/mouse category, even if the other category fails, and returns their individual results (`None` means not selected). It is separate from the keyboard-only `release_all()`. Neither execution nor cleanup can interrupt a synchronous `DeviceIoControl`, undo delivered input, or confirm that an application received it.

### Command line

Run commands from the checkout as `cargo run -p logi-vhid --bin logi-vhid-demo --release --` followed by a command below, or pass the command directly to the built `logi-vhid-demo.exe`.

| Command / example | Behavior |
| --- | --- |
| `help` | List commands; no device is opened |
| `cursor` | Read cursor position in physical screen pixels; no device is opened |
| `info` | Open/create both virtual devices, then print control/HID paths and PID gate state |
| `move 10 -5` | Relative HID counts, not guaranteed pixel displacement |
| `move-to 400 300 --mode driver` | Position the cursor without clicking |
| `click 400 300 --button left --mode driver` | Position, then press/release a mouse button |
| `double-click 400 300 --button left` | Position, then send two clicks |
| `key 0x04` | Press/release HID usage A, not Win32 virtual key `0x41` |
| `combo 0xE0,0x06` | Press/release Left Ctrl + C; usages are comma-separated |
| `type "Hello" --mode us` | Layout-mapped HID text |
| `type "中文" --mode unicode` | Win32 Unicode input |
| `window "Notepad" 100 100 --mode driver` | Click a client-relative point in the first visible, case-insensitive title match |

Absolute modes are `driver` (default), `win32` and `set` (`setcursorpos` is also accepted). Text modes are `us` (default), `uk`, `de`, `fr` and `unicode`. Buttons are `left` (default), `right`, `middle`, `back` and `forward`. Key usages accept decimal or lowercase `0x` hexadecimal notation.

Put positional arguments before options; `type` also permits `--mode` before its text. Unknown/missing/repeated options and extra arguments are rejected before a device opens. Text starting with `--` cannot be passed as a positional argument; there is no end-of-options delimiter.

Every command except help aliases and `cursor` opens the G HUB bus, **including Unicode and Win32 positioning modes**. `window` only finds a window and converts coordinates: it does not activate it, check client bounds or account for overlapping windows. The click lands on whatever is actually at that screen point.

### Local stdio MCP

After a version containing MCP assets is published, download `logi-vhid-mcp-<Cargo version>-windows-x86_64-msvc.exe` or its native `.mcpb` bundle from [GitHub Releases](https://github.com/lona-cn/logi-vhid/releases), or build with `cargo build --locked -p logi-vhid-mcp --bin logi-vhid-mcp`. Version **0.1.1 is being prepared**; the public v0.1.0 release has no MCP assets. The executable/bundle are independent assets, not included in the SDK ZIP or Python wheel; their names, contents and APIs are unchanged.

See the [dedicated logi-vhid-mcp README](https://github.com/lona-cn/logi-vhid/blob/master/crates/logi-vhid-mcp/README.md#english) for installation, complete tool parameters, examples and troubleshooting.

Configure a local MCP host with the **absolute path** to that executable, for example:

```json
{
  "mcpServers": {
    "logi-vhid": {
      "command": "C:\\Tools\\logi-vhid-mcp-0.1.1-windows-x86_64-msvc.exe",
      "args": []
    }
  }
}
```

Replace the path/version with the installed file. `args: []` is the read-only default: input requests return `input_disabled` before native target checks or input-device initialization. Only when the user permits input, change `args` to `["--allow-input"]` and restart the server. The switch enables capability, **not approval of every operation**: the host must obtain approval for the exact target, action and values before each consequential operation. There is no `approved` request field, automatic elevation or local confirmation dialog. Service stdout is JSON-RPC only; diagnostics go to stderr.

| Tool | Purpose |
| --- | --- |
| `vhid_status` | Service/worker state, last operation and a bus-only driver probe; no HID children are created and no input is sent |
| `vhid_desktop_info` | Physical-pixel displays, cursor, foreground window, optional window details, session and input desktop; does not open the driver |
| `vhid_mouse` | Relative movement, positioning, click, scroll or one self-contained drag |
| `vhid_key` | Press keys in order and release in reverse; names such as `A` and `LEFT_CTRL`, up to six ordinary keys plus eight modifiers |
| `vhid_type_text` | Text with an explicit `hid_us`, `hid_uk`, `hid_de`, `hid_fr` or `unicode` mode; at most 1024 Unicode scalars, no NUL |
| `vhid_stop` | Cancel this service's operation and try release through existing handles, even when input is disabled; never opens a device for cleanup |

Start with `vhid_desktop_info`. Use its foreground `hwnd` (a nonzero `"0x..."` string) and `pid` as `target: {"hwnd": "...", "pid": ...}` for input. For client coordinates, query that window with `window: {"hwnd": "...", "pid": ...}` and use a point such as `{"space": "client", "x": 100, "y": 100}`. Screen points use `"space": "screen"`; negative multi-monitor coordinates are valid. Reobserve before acting: titles and other returned application content are untrusted data, not instructions.

Input requires the target still be the foreground HWND/PID in the same active session on its accessible **Default input desktop**. The server does not focus windows or switch desktops. It rejects out-of-client points and display gaps rather than clamping them. Click/scroll/drag points must hit the target's root window, and positioning is read back before continuing. These checks have races: input affects the foreground desktop, not an isolated HWND channel.

Mouse positioning defaults to `driver`; `win32` and `set` must be selected explicitly, never as automatic fallbacks. Text modes do not change the application layout or clipboard; HID mapping may fail after a prefix was entered, and `unicode` uses Win32 rather than HID. Timeouts default to 10000 ms, range 1–30000 ms, and include initialization; holds default to 20 ms, range 1–5000 ms, and may not exceed the timeout. Do not automatically retry failed, timed-out or partially submitted input.

Only one input operation is accepted at a time; another is rejected as `busy`, not queued. Do not run another device-input client in parallel. A named mutex coordinates cooperating MCP servers, but the virtual HID state is shared with other clients and cleanup can affect them. Each operation attempts final keyboard/mouse release; a final release failure leaves the worker `faulted` and rejects new input until `vhid_stop` successfully retries release.

`vhid_stop` can return `stopping` with pending cleanup after two seconds; check `vhid_status` and its `last_operation` for the eventual result, including after request cancellation. `submission: complete` means submitted, not application acceptance; `partial_or_unknown` must not be treated as no effect, and `target_observed` remains false. Cleanup does not undo text, clicks or movement.

EOF, Ctrl+C and service shutdown request cancellation and wait up to five seconds for worker cleanup. Synchronous driver I/O and cleanup can block beyond cooperative deadlines; shutdown then reports failure rather than claiming release. Forced termination, crashes and power loss have **no release guarantee**.

This server provides no screenshots. Continue using the host's existing screenshot/observation tools; installing it does not automatically replace an existing computer-control tool. Driver availability and protocol smoke checks do not prove real input compatibility.
Read the [privacy notice](docs/privacy.md#english) before sharing desktop data or sensitive input. Use [GitHub Issues](https://github.com/lona-cn/logi-vhid/issues) for ordinary support; follow the [security reporting policy](SECURITY.md#english) for vulnerabilities. MCPB/Cline installation, host support status and submission materials are linked from the dedicated MCP README.



### C/C++ SDK

For a published release, download `logi-vhid-<version>-windows-x86_64-msvc.zip` from [GitHub Releases](https://github.com/lona-cn/logi-vhid/releases). It contains the DLL, **DLL import library** `logi_vhid.dll.lib`, C header, SDK README, LICENSE and safe C/C++ examples. Do not link the separate Rust static library `logi_vhid.lib` in its place.

See the [SDK guide](https://github.com/lona-cn/logi-vhid/blob/master/sdk/README.md) for linking, CMake commands and C usage; see [`include/logi_vhid.h`](https://github.com/lona-cn/logi-vhid/blob/master/include/logi_vhid.h) for parameters, return values, pointer lifetimes and concurrency rules. The packaged examples test only null-handle/error-query behavior; they never open devices or send input.

### Python

In a Python virtual environment, install an available Windows x64 wheel from PyPI:

```powershell
python -m pip install logi-vhid
```

Alternatively install the `.whl` from the matching GitHub Release. From this source checkout, `python -m pip install .` builds and installs the DLL using the x64 MSVC Rust toolchain and build tools described above.

Import loads the package's `_native/logi_vhid.dll`; constructing `LogitechVirtualHid` opens the bus. The package does not search a checkout's `target` directories. Set `LOGI_VHID_DLL` to a specific DLL **before importing** only when intentionally overriding the bundled DLL; do not add the repository to `PYTHONPATH` for an installed package.

The following example **moves/clicks the mouse and types into the foreground application**:

```python
from logi_vhid import LogitechVirtualHid, LEFT, MODE_SET_CURSOR_POS

with LogitechVirtualHid() as device:
    device.set_cursor(400, 300, MODE_SET_CURSOR_POS)
    device.move(10, 0)
    device.click(LEFT)
    device.type_unicode("中文")
```

The Python API is:

| Member | Meaning |
| --- | --- |
| `LogitechVirtualHid(keyboard=True, mouse=True)` | Open the bus and create/verify selected devices; both flags may be `False` |
| `device_path` | Copied control-interface path, not a child HID path |
| `move(dx, dy)` | Relative HID movement |
| `click(button=LEFT)` / `double_click(button=LEFT)` | Click at the current cursor position |
| `key(usage)` | Press/release one HID Keyboard/Keypad usage, e.g. `0x04` for A |
| `type_ascii(text)` | US-layout printable ASCII plus tab, CR and LF |
| `type_unicode(text)` | Win32 Unicode events, including non-BMP text |
| `set_cursor(x, y, mode=MODE_DRIVER)` | Position in physical screen pixels |
| `pid_gate_active()` | Query whether registered PIDs disable relative mouse movement |
| `close()` | Wait for the current operation and close once; repeated calls do nothing |

Buttons are `LEFT`, `RIGHT`, `MIDDLE`, `BACK`, `FORWARD` (0–4). Modes are `MODE_DRIVER`, `MODE_WIN32`, `MODE_SET_CURSOR_POS` (0–2). Coordinates/deltas must fit signed 32-bit integers; usages must fit a byte. The wrapper does not range-check these integers before `ctypes` conversion, so out-of-range values can wrap.

DLL failures and use after close raise `RuntimeError`. Embedded NUL in text and out-of-range buttons/modes raise `ValueError` before the input call; invalid Unicode surrogates fail during UTF-8 encoding. A missing bundled DLL raises `FileNotFoundError`; Windows loader failures raise `OSError`. See `help(LogitechVirtualHid)` for method details.

Use `with` or call `close()` explicitly; there is no finalizer. One instance serializes its own calls, including close, but separate instances/processes do not coordinate shared device state. Even with both flags `False`, Unicode and Win32 cursor methods still require a successfully opened, live bus handle.

### Coordinates and input modes

| Absolute mode | Implementation | Success means |
| --- | --- | --- |
| `driver` | Read the cursor and correct with relative G HUB reports | The clamped target was observed; failure returns an error, never an automatic fallback |
| `win32` | Absolute Win32 `SendInput` | Windows accepted the event; final position is not read back |
| `set` | Win32 `SetCursorPos` | The API succeeded; final position is not read back |

In the library, CLI, C and Python APIs, absolute screen/window coordinates are **physical pixels**, not DPI-scaled logical coordinates or normalized `0..65535` values. Multi-monitor screen coordinates may be negative. Absolute targets are clamped to the virtual desktop's bounding rectangle; gaps between displays or cursor clipping can still make them unreachable. CLI `window` coordinates start at the client area's top-left, excluding borders and the title bar. The MCP server instead validates target bounds and display membership before positioning.

Coordinate helpers temporarily use a per-monitor DPI-aware thread context and attempt to restore it before returning. Relative `move` uses **HID counts**, affected by Windows pointer settings. It does not promise a one-count-to-one-pixel mapping.

HID text uses the selected US/UK/DE/FR layout without changing the target application's layout. It samples Caps Lock once, treats CRLF as one Enter, and handles tab as a Tab key. Unmappable characters, dead-key sequences and non-BMP characters return an error. Validation is incremental: a prefix may already have been typed. Unicode mode instead sends UTF-16 Unicode events through `SendInput`; it does not normalize CRLF or synthesize HID Enter/Tab, and the receiving application must support that input path.

### Limitations and cleanup

- No virtual joystick, media-key or keyboard-indicator support. The keyboard report holds at most six non-modifier usages plus eight modifier bits.
- The virtual keyboard and mouse are shared, not isolated per object. Avoid interleaving clients' reports or removing devices another client uses.
- Temporary press/combo/text/click/drag operations preserve keys/buttons already held by the same Rust object and attempt to release newly held input on failure. A `Sequence` stops at the first error and attempts the same cleanup. This is not a rollback: delivered text, clicks and movement remain.
- Cleanup can fail. For explicit Rust `key_down`/`mouse_button_down`, pair releases yourself; `release_all()` releases **keyboard keys only**, not mouse buttons. Closing/dropping the handle is not an input-release operation.
- Do not type manually or switch layouts during HID text input; held modifiers and other input sources can affect the result. Win32 input remains subject to Windows security restrictions, and applications may reject or reinterpret it.
- This project does not bypass driver signing, Windows security or application/game input restrictions.

### Troubleshooting

| Observation | Check / action |
| --- | --- |
| Bus open fails or no G HUB interface is found | Confirm that the G HUB virtual bus driver is installed/available; wheel/SDK installation does not install it. Keep the full native error and Windows/G HUB versions. |
| Relative movement succeeds but the cursor does not move | Inspect `info`, Rust `registered_pids()` / `ensure_mouse_available()`, or Python `pid_gate_active()`. A nonempty PID list makes the driver zero X/Y. These queries do not clear registrations; `info` opens/creates devices. |
| Driver positioning cannot reach the point | Check the target, display gaps, cursor clipping and PID gate. Select `win32` or `set` explicitly only if that input method is acceptable; there is no silent fallback. |
| HID text is wrong or fails partway | Match the active layout, avoid held modifiers, or choose Unicode mode for unmappable text. Already delivered input is not undone. |
| Python import fails | Check Windows/Python x64, the bundled DLL or intentional override, and the VC++ runtime. Preserve the loader error; do not fix it by adding a source checkout to `PYTHONPATH`. |

### Testing

These checks do not send desktop input; the ignored hardware test remains skipped:

```powershell
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo rustdoc --locked -p logi-vhid --lib -- -D warnings
python -m venv target/release-tools
.\target\release-tools\Scripts\python.exe -m pip install -r tools/requirements-release.txt
.\target\release-tools\Scripts\python.exe -B -m unittest discover -s python/tests -p "test_*.py"
.\target\release-tools\Scripts\python.exe -B -m unittest discover -s tools -p "test_*.py"
```

The standalone standard-library MCP smoke test needs neither G HUB nor permission for desktop input. It starts the real executable with no arguments, queries the desktop, checks input rejection, calls stop, and closes stdin:

```powershell
cargo build --locked --target x86_64-pc-windows-msvc -p logi-vhid-mcp --bin logi-vhid-mcp
$metadata = cargo metadata --locked --format-version 1 --no-deps | ConvertFrom-Json
$exe = Join-Path $metadata.target_directory "x86_64-pc-windows-msvc/debug/logi-vhid-mcp.exe"
python tools/mcp_smoke.py --exe "$exe"
```

The hardware test (`crates/logi-vhid/tests/hardware.rs`) **creates devices, moves the cursor and presses/releases F24**. It attempts keyboard release and cursor restoration, but failures may prevent full cleanup. Release F24 first and run only on a desktop where these effects are safe:

```powershell
cargo test -p logi-vhid --test hardware -- --ignored --nocapture --test-threads=1
```

Building SDK/wheel/MCP artifacts, verifying packages, publishing a draft and configuring PyPI are documented in the [release maintenance guide](https://github.com/lona-cn/logi-vhid/blob/master/docs/releasing.md). Automated package checks do not replace separately authorized manual hardware acceptance.

### License

MIT. See [LICENSE](LICENSE).

## 简体中文

面向 Windows x64 的 Rust 键鼠输入库，通过 Logitech G HUB 的虚拟 HID 总线发送输入，同时提供 CLI、C ABI、Python `ctypes` 包和本地 stdio MCP 服务。项目不附带驱动或 G HUB 安装程序。

[Rust 接入](#rust-接入) · [命令行](#命令行) · [MCP](#本地-stdio-mcp) · [C/C++ SDK](#cc-sdk-1) · [Python](#python-安装与使用) · [坐标与输入方式](#坐标与输入方式) · [排错](#排错) · [测试](#测试)

### 环境要求

- Windows 10 1703 或更新版本，x64；输入操作需要可交互的桌面。
- 打开设备时需要已安装且可访问的 Logitech G HUB 虚拟总线驱动。项目没有证明所有 G HUB 版本均兼容。
- 从源码构建需要 Rust >=1.96、x64 MSVC toolchain、Visual Studio C++ build tools 和 Windows SDK。下列 Cargo 命令假定当前使用 `x86_64-pc-windows-msvc` 工具链。
- Python 需要 Python >=3.12、Windows x64 和 Microsoft Visual C++ v14 x64 运行库。安装预编译 wheel 不需要 Rust。

原生 DLL 需要 `VCRUNTIME140.dll`。缺少时请安装微软支持的 [x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)。SDK 和 wheel 都不会安装系统组件。

**输入示例会实际操作当前桌面。** 运行前请选择允许这些操作的前台应用。打开设备会创建、检查共享的虚拟 HID 设备，并非只读查询；关闭句柄不会自动释放按住的键、鼠标按钮，也不会删除虚拟设备。

### Rust 接入

根 `Cargo.toml` 是虚拟 workspace：`crates/logi-vhid` 包含库和 demo，`crates/logi-vhid-mcp` 包含服务。两者共用根 `Cargo.lock`、版本和产物目录；C 头文件仍位于 `include/logi_vhid.h`。

在使用本库的项目中，可直接依赖本地源码，不必等待发布到 crates.io。请将路径改为实际的源码位置：

```powershell
cargo add logi-vhid --path ../logi-vhid/crates/logi-vhid
```

如果所需版本已发布到 crates.io，则使用 `cargo add logi-vhid`。在本仓库中，构建库和 CLI、生成完整 API 文档：

```powershell
cargo build -p logi-vhid --release
cargo doc -p logi-vhid --no-deps --open
```

产物包括 `logi-vhid-demo.exe`、`logi_vhid.dll`、DLL 导入库 `logi_vhid.dll.lib`，以及单独的 Rust 静态库 `logi_vhid.lib`。默认位于 `target/release/`；Cargo 配置、`CARGO_TARGET_DIR` 和显式指定的 `--target` 都会改变路径。可用 `cargo metadata --format-version 1 --no-deps` 查询实际 `target_directory`；显式指定 target 时，还会增加对应 target triple 子目录。

以下示例会**移动、点击鼠标并发送键盘输入**：

```rust
use logi_vhid::{
    hid, AbsoluteMode, KeyboardLayout, LogitechVirtualHid, MouseButton, TextMode,
};

fn main() -> logi_vhid::Result<()> {
    let mut device = LogitechVirtualHid::open()?;

    device.mouse_move_relative(10, -5)?;
    device.click_at(400, 300, MouseButton::Left, AbsoluteMode::Driver)?;
    device.press_combo(&[hid::LEFT_CTRL, hid::C])?;
    device.type_text_layout("hello", KeyboardLayout::Us)?;
    device.type_text("中文", TextMode::Win32Unicode)?;

    Ok(())
}
```

`open()` 同时选择键盘和鼠标。只需其中一个时，使用 `open_with_options(OpenOptions::keyboard_only())` 或 `OpenOptions::mouse_only()`。打开时会等待每个选中的 HID 子设备出现，最多检查两秒；设备存在不等于已验证真实输入成功。

Rust 支持按键按下/释放、组合键、长按、重复按键、按布局输入文本、鼠标按钮、滚轮、拖动、`Sequence`、显示器和窗口查询及坐标转换。C 和 Python 只提供其中一部分，请以各自接口说明为准。

`sequence.run_controlled(&mut device, &checkpoint)` 通过调用方提供的 `Result<()>` 检查点支持合作式取消和截止时间；可返回 `Error::Cancelled` 或 `Error::TimedOut`，在安全边界停止。失败时的 `ExecutionFailure` 包含原始原因和各类设备的 `ReleaseReport` 清理结果；序列清理保留执行前已按住且未被显式释放的输入。

`device.release_all_inputs()` 尝试向所有启用的键盘、鼠标类别发送中性报告，即使一类失败也继续另一类，并分别返回结果（`None` 表示未选中）。它与仅释放键盘的 `release_all()` 不同。执行和清理都不能中断同步 `DeviceIoControl`、撤销已发送的输入，也不能证明应用已收到输入。

### 命令行

在仓库中，使用 `cargo run -p logi-vhid --bin logi-vhid-demo --release --` 加下列命令；也可以把命令直接传给编译后的 `logi-vhid-demo.exe`。

| 命令 / 示例 | 行为 |
| --- | --- |
| `help` | 查看帮助，不打开设备 |
| `cursor` | 读取光标的物理屏幕坐标，不打开设备 |
| `info` | 打开、创建键盘和鼠标，再打印控制接口、HID 路径和 PID gate 状态 |
| `move 10 -5` | 发送相对 HID 计数，不保证移动相同数量的像素 |
| `move-to 400 300 --mode driver` | 定位光标，不点击 |
| `click 400 300 --button left --mode driver` | 先定位，再按下并释放鼠标按钮 |
| `double-click 400 300 --button left` | 先定位，再发送两次点击 |
| `key 0x04` | 按下并释放 HID usage A，不是 Win32 virtual key `0x41` |
| `combo 0xE0,0x06` | 按下并释放左 Ctrl + C，usage 之间用逗号分隔 |
| `type "Hello" --mode us` | 按键盘布局发送 HID 文本输入 |
| `type "中文" --mode unicode` | 通过 Win32 发送 Unicode 输入 |
| `window "Notepad" 100 100 --mode driver` | 在第一个标题匹配的可见窗口中点击客户区坐标，标题匹配不区分大小写 |

绝对定位模式为 `driver`（默认）、`win32` 和 `set`，也接受 `setcursorpos`。文本模式为 `us`（默认）、`uk`、`de`、`fr` 和 `unicode`。鼠标按钮为 `left`（默认）、`right`、`middle`、`back`、`forward`。按键 usage 接受十进制和小写 `0x` 前缀的十六进制。

位置参数放在选项前；`type` 也允许把 `--mode` 放在文本前。未知选项、缺少选项值、重复选项和多余参数都会在打开设备前报错。不能将以 `--` 开头的文本作为位置参数，也不支持用单独的 `--` 结束选项解析。

除帮助命令及其别名、`cursor` 外，所有命令都会打开 G HUB 总线，**包括 Unicode 和 Win32 定位模式**。`window` 只查找窗口并转换坐标，不会激活窗口、不检查客户区边界，也不处理遮挡；点击作用于那个屏幕位置实际显示的内容。

### 本地 stdio MCP

含 MCP 资产的版本发布后，从 [GitHub Releases](https://github.com/lona-cn/logi-vhid/releases) 下载 `logi-vhid-mcp-<Cargo 版本>-windows-x86_64-msvc.exe` 或其原生 `.mcpb` 包，也可用 `cargo build --locked -p logi-vhid-mcp --bin logi-vhid-mcp` 构建。当前准备版本为 **0.1.1**；公开 v0.1.0 没有 MCP 附件，需先正式发版。exe/包是独立附件，不放进 SDK ZIP 或 Python wheel；原有包名、内容与接口不变。

安装、完整工具参数、调用示例和排错见 [logi-vhid-mcp 独立 README](https://github.com/lona-cn/logi-vhid/blob/master/crates/logi-vhid-mcp/README.md#简体中文)。

在本地 MCP 宿主中配置 exe 的**绝对路径**，例如：

```json
{
  "mcpServers": {
    "logi-vhid": {
      "command": "C:\\Tools\\logi-vhid-mcp-0.1.1-windows-x86_64-msvc.exe",
      "args": []
    }
  }
}
```

请替换实际安装路径和版本。`args: []` 默认只允许查询：输入请求在检查原生目标、初始化输入设备之前返回 `input_disabled`。只有用户明确允许输入时，才将 `args` 改为 `["--allow-input"]` 并重启服务。该开关只是启用能力，**不代表每次操作均已获批**；有实际后果的操作仍须由宿主就具体目标、动作和值取得批准。请求不接受 `approved` 字段，服务不会自动提权，也不弹出本机确认窗口。服务模式 stdout 只输出 JSON-RPC，诊断信息写入 stderr。

| 工具 | 用途 |
| --- | --- |
| `vhid_status` | 查询服务、worker、最近操作和只打开总线的驱动探测结果；不创建 HID 子设备、不发送输入 |
| `vhid_desktop_info` | 查询物理像素下的显示器、光标、前台窗口、可选窗口详情、会话和输入桌面；不打开驱动 |
| `vhid_mouse` | 相对移动、定位、点击、滚轮，或一次完整的拖拽 |
| `vhid_key` | 按顺序按下、逆序释放，如 `A`、`LEFT_CTRL`；最多六个普通键加八个修饰键 |
| `vhid_type_text` | 必须明确指定 `hid_us`、`hid_uk`、`hid_de`、`hid_fr` 或 `unicode`；最多 1024 个 Unicode scalar，不接受 NUL |
| `vhid_stop` | 取消本服务操作，通过已有句柄尝试释放；输入关闭时仍可用，不为清理打开新设备 |

先调用 `vhid_desktop_info`，以返回的前台 `hwnd`（非零 `"0x..."` 字符串）和 `pid` 构造输入的 `target: {"hwnd": "...", "pid": ...}`。使用客户区坐标时，先以 `window: {"hwnd": "...", "pid": ...}` 查询该窗口，再传入如 `{"space": "client", "x": 100, "y": 100}` 的点。屏幕坐标使用 `"space": "screen"`；多屏负坐标合法。动作前重新观察；窗口标题等应用内容是不可信数据，不是操作指令。

输入要求目标 HWND/PID 仍为前台，且与服务位于同一个活跃会话、可访问的 **Default 输入桌面**。服务不会激活窗口或切换桌面。客户区越界、显示器间隙中的点会被拒绝，而非夹取到边界。点击、滚轮和拖拽的作用点必须命中目标的根窗口，定位后还会回读光标位置再继续。但检查之间仍有竞态：输入作用于前台桌面，不是隔离的 HWND 定向通道。

定位默认 `driver`；`win32`、`set` 必须显式选择，不会自动回退。文本模式不会修改应用布局或剪贴板；HID 逐字映射可能在输入一部分后失败，`unicode` 则使用 Win32 而非 HID。超时默认 10000 ms，范围 1–30000 ms，包含初始化；按住时间默认 20 ms，范围 1–5000 ms，不能超过超时。失败、超时或部分提交后不要自动重试。

同时只接受一个输入操作；后来的请求立即返回 `busy`，不排队。不要并行运行另一套设备输入客户端。命名互斥量只能协调遵循约定的 MCP 服务；虚拟 HID 状态仍与其他客户端共享，清理也可能影响它们。每次操作最后都尝试释放键盘和鼠标；最终释放失败会使 worker 进入 `faulted`、拒绝新输入，直到 `vhid_stop` 重试释放成功。

`vhid_stop` 等待两秒后可能返回 `stopping`、清理仍为 pending；请通过 `vhid_status` 的 `last_operation` 查询最终结果，请求取消后也一样。`submission: complete` 仅表示已提交，不代表应用接受；`partial_or_unknown` 不能视为没有副作用，`target_observed` 始终为 false。清理不会撤销文字、点击或移动。

EOF、Ctrl+C 和服务退出会请求取消，最多等待五秒完成 worker 清理。同步驱动 I/O 和清理可能阻塞，超过合作式截止时间；此时退出会报告失败，不会声称已释放。强杀、崩溃或断电情况下**无法保证释放**。

服务不提供截图。截图和观察继续使用宿主现有工具；安装 MCP 不会自动替换既有 computer 控制工具。驱动可查询、协议冒烟检查通过，都不能证明目标软件接受真实输入。
分享桌面数据或敏感输入前，请阅读 [隐私说明](docs/privacy.md#简体中文)。普通支持使用 [GitHub Issues](https://github.com/lona-cn/logi-vhid/issues)，漏洞报告遵循 [安全报告政策](SECURITY.md#简体中文)。MCPB/Cline 安装、宿主支持状态和上架素材见 MCP 独立 README。



### C/C++ SDK

版本发布后，可从 [GitHub Releases](https://github.com/lona-cn/logi-vhid/releases) 下载 `logi-vhid-<版本>-windows-x86_64-msvc.zip`。包内包含 DLL、**DLL 导入库** `logi_vhid.dll.lib`、C 头文件、SDK README、LICENSE 和安全的 C/C++ 示例。不要用单独的 Rust 静态库 `logi_vhid.lib` 代替导入库。

链接方法、CMake 命令和 C 用法见 [SDK 说明](https://github.com/lona-cn/logi-vhid/blob/master/sdk/README.md)；参数、返回值、指针有效期和并发要求见 [`include/logi_vhid.h`](https://github.com/lona-cn/logi-vhid/blob/master/include/logi_vhid.h)。包内示例只检查空句柄和错误查询，不打开设备、不发送输入。

### Python 安装与使用

在 Python 虚拟环境中，安装 PyPI 上已发布的 Windows x64 wheel：

```powershell
python -m pip install logi-vhid
```

也可安装对应 GitHub Release 附带的 `.whl`。从当前源码安装时，运行 `python -m pip install .`；这会用前述 x64 MSVC Rust 工具链和构建工具编译、安装 DLL。

import 时加载包内 `_native/logi_vhid.dll`，构造 `LogitechVirtualHid` 时才打开总线。包不会搜索源码的 `target` 目录。只有确实需要覆盖内置 DLL 时，才在 **import 前**将 `LOGI_VHID_DLL` 设为指定的 DLL 文件；使用已安装的包不要把源码目录加入 `PYTHONPATH`。

以下示例会**移动、点击鼠标，并向前台应用输入文字**：

```python
from logi_vhid import LogitechVirtualHid, LEFT, MODE_SET_CURSOR_POS

with LogitechVirtualHid() as device:
    device.set_cursor(400, 300, MODE_SET_CURSOR_POS)
    device.move(10, 0)
    device.click(LEFT)
    device.type_unicode("中文")
```

Python 接口：

| 成员 | 作用 |
| --- | --- |
| `LogitechVirtualHid(keyboard=True, mouse=True)` | 打开总线并创建、检查选中的设备；两个选项都可设为 `False` |
| `device_path` | 返回控制接口路径的字符串副本，不是 HID 子设备路径 |
| `move(dx, dy)` | 发送相对 HID 移动 |
| `click(button=LEFT)` / `double_click(button=LEFT)` | 在当前光标位置点击 |
| `key(usage)` | 按下并释放一个 HID Keyboard/Keypad usage，例如 A 为 `0x04` |
| `type_ascii(text)` | 输入 US 布局的可打印 ASCII，以及 Tab、CR、LF |
| `type_unicode(text)` | 发送 Win32 Unicode 事件，支持非 BMP 字符 |
| `set_cursor(x, y, mode=MODE_DRIVER)` | 按物理屏幕坐标定位 |
| `pid_gate_active()` | 查询是否有已注册 PID 禁用了相对鼠标移动 |
| `close()` | 等待当前操作完成并关闭一次；重复调用无操作 |

按钮常量为 `LEFT`、`RIGHT`、`MIDDLE`、`BACK`、`FORWARD`（0–4）；定位模式为 `MODE_DRIVER`、`MODE_WIN32`、`MODE_SET_CURSOR_POS`（0–2）。坐标和移动量必须在有符号 32 位整数范围内，usage 必须在一个字节内；这些整数没有在 `ctypes` 转换前检查范围，超出后可能绕回其他数值。

DLL 报错和关闭后使用会抛出 `RuntimeError`。文本中的 NUL、越界的按钮或模式会在输入调用前抛出 `ValueError`；无效 Unicode surrogate 在 UTF-8 编码时失败。缺少包内 DLL 时抛出 `FileNotFoundError`，Windows 加载器失败时抛出 `OSError`。详细方法说明可用 `help(LogitechVirtualHid)` 查看。

请使用 `with` 或显式调用 `close()`；类没有 finalizer。同一实例会串行执行包括关闭在内的调用，但不同实例、进程不会协调共享设备状态。即使两个设备选项都为 `False`，Unicode 和 Win32 定位方法仍需要已成功打开、尚未关闭的总线句柄。

### 坐标与输入方式

| 绝对定位模式 | 实现方式 | 成功的含义 |
| --- | --- | --- |
| `driver` | 读取光标位置，通过 G HUB 相对报告逐步校正 | 已观察到光标到达目标（越界坐标先限制到桌面边界）；失败报错，不自动切换方式 |
| `win32` | Win32 `SendInput` 绝对移动 | Windows 接受了事件，不回读最终位置 |
| `set` | Win32 `SetCursorPos` | API 调用成功，不回读最终位置 |

库、CLI、C 和 Python API 的绝对屏幕、窗口坐标均为**物理像素**，不是随 DPI 缩放的逻辑坐标，也不是 `0..65535` 归一化数值。多屏坐标可以为负。绝对目标会限制在虚拟桌面的包围矩形内；显示器之间的空隙、光标限制等仍可能导致目标无法到达。CLI 的 `window` 坐标从客户区左上角算起，不包括边框和标题栏。MCP 服务则在定位前检查目标边界和点是否位于真实显示器内。

坐标辅助函数临时切换当前线程的 per-monitor DPI-aware context，返回前尝试恢复。相对 `move` 使用 **HID 计数**，受 Windows 指针设置影响，不保证一个计数对应一个像素。

HID 文本输入使用选定的 US/UK/DE/FR 布局，但不切换目标应用的布局。库会读取一次 Caps Lock，将 CRLF 作为一次 Enter、Tab 作为 Tab 键发送。无法映射、需要 dead key 或超出 BMP 的字符会报错；字符逐个检查，因此出错前可能已输入一部分文字。Unicode 模式则通过 `SendInput` 发送 UTF-16 Unicode 事件，不合并 CRLF，也不把它们转换为 HID Enter/Tab；接收应用必须支持这种输入方式。

### 限制与释放输入

- 不支持虚拟摇杆、媒体控制键和键盘指示灯。键盘报告最多同时容纳六个非修饰键，以及八个修饰键位。
- 虚拟键盘和鼠标由多个对象共享，不是每个对象独立创建。不要交错发送不同客户端的报告，也不要删除其他客户端正在使用的设备。
- 临时按键、组合键、文本、点击和拖动会保留同一个 Rust 对象原本按住的键、按钮；失败时尝试释放本次新增的按下状态。`Sequence` 遇到第一个错误就停止，并尝试同样的清理。这不会撤销已送出的文字、点击或移动。
- 清理也可能失败。Rust 显式调用 `key_down`、`mouse_button_down` 后，需要自行配对释放；`release_all()` **只释放键盘按键**，不释放鼠标按钮。关闭句柄或丢弃对象不等于释放输入。
- HID 文本输入期间不要手动输入或切换布局；已按住的修饰键及其他输入来源都可能影响结果。Win32 输入仍受 Windows 安全限制，应用也可能拒绝或重新解释输入。
- 本项目不绕过驱动签名、Windows 安全设置或应用、游戏的输入限制。

### 排错

| 现象 | 检查与处理 |
| --- | --- |
| 打开总线失败，找不到 G HUB 接口 | 确认已安装且可访问 G HUB 虚拟总线驱动；wheel、SDK 不会安装驱动。保留完整底层错误及 Windows/G HUB 版本。 |
| 相对移动返回成功，但光标不动 | 查看 `info`、Rust `registered_pids()` / `ensure_mouse_available()` 或 Python `pid_gate_active()`。PID 列表非空时，驱动会将 X/Y 清零。这些查询不会清除注册；`info` 会打开、创建设备。 |
| driver 模式无法到达目标 | 检查目标、显示器空隙、光标限制及 PID gate。如果允许换用其他输入方式，请显式选择 `win32` 或 `set`；库不会悄悄切换。 |
| HID 文字错误或输入一部分后报错 | 匹配当前输入布局，避免按住修饰键；无法映射的文字可改用 Unicode 模式。已输入的内容不会撤销。 |
| Python import 失败 | 检查 Windows/Python x64、内置 DLL 或有意设置的覆盖路径，以及 VC++ 运行库。保留加载器错误，不要靠把源码加入 `PYTHONPATH` 处理安装问题。 |

### 测试

以下检查不会发送桌面输入，默认继续跳过硬件测试：

```powershell
cargo test --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo rustdoc --locked -p logi-vhid --lib -- -D warnings
python -m venv target/release-tools
.\target\release-tools\Scripts\python.exe -m pip install -r tools/requirements-release.txt
.\target\release-tools\Scripts\python.exe -B -m unittest discover -s python/tests -p "test_*.py"
.\target\release-tools\Scripts\python.exe -B -m unittest discover -s tools -p "test_*.py"
```

独立的 MCP 冒烟脚本只依赖 Python 标准库，不需要 G HUB 或桌面输入授权。它无参数启动真实 exe，查询桌面、确认输入被拒绝、调用 stop，再关闭 stdin：

```powershell
cargo build --locked --target x86_64-pc-windows-msvc -p logi-vhid-mcp --bin logi-vhid-mcp
$metadata = cargo metadata --locked --format-version 1 --no-deps | ConvertFrom-Json
$exe = Join-Path $metadata.target_directory "x86_64-pc-windows-msvc/debug/logi-vhid-mcp.exe"
python tools/mcp_smoke.py --exe "$exe"
```

硬件测试（`crates/logi-vhid/tests/hardware.rs`）会**创建设备、移动光标、按下并释放 F24**，然后尝试释放键盘按键、恢复光标；发生错误时不保证清理完整。请先松开 F24，且只在允许这些操作的桌面上运行：

```powershell
cargo test -p logi-vhid --test hardware -- --ignored --nocapture --test-threads=1
```

SDK/wheel/MCP 构建、发行包验收、草稿发布和 PyPI 配置见 [发布维护说明](https://github.com/lona-cn/logi-vhid/blob/master/docs/releasing.md)。自动发行包检查不能代替另行获得授权的人工硬件验收。

### 许可证

采用 MIT 许可证，完整内容见 [LICENSE](LICENSE)。
