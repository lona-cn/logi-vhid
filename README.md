# logi-vhid

[English](#english) · [简体中文](#简体中文)

## English

A Rust library for mouse and keyboard input on Windows. It creates virtual keyboard and mouse devices through Logitech G HUB, and also provides a command-line tool, a C API, and a Python module.

### Features

- Relative mouse movement, clicks, double-clicks, dragging, and scrolling.
- Cursor positioning: move to specified coordinates using the virtual mouse or Windows input APIs.
- Keyboard keys, key combinations, long presses, and repeated key presses.
- Type text using the US, UK, DE, or FR keyboard layout, or type Unicode text such as Chinese.
- Execute a sequence of mouse and keyboard operations, and query window, display, and coordinate information.

### Requirements

- Windows 10 version 1703 or later, x64.
- Logitech G HUB installed and running.
- Rust 1.96 or later to build from source; Python to use the Python module.

### Rust Dependency

After a version is published on crates.io, add it to a Rust project with:

```powershell
cargo add logi-vhid
```

### Build

Run this command from the project directory:

```powershell
cargo build --release
```

By default, the executable `logi-vhid-demo.exe` and library `logi_vhid.dll` are built in `target/release/`. If you set `CARGO_TARGET_DIR`, look in its `release/` subdirectory instead.

### Rust Example

This example moves and clicks the mouse, sends Ctrl+C, and types text into the active application. Run it only when those actions are safe.

```rust
use logi_vhid::{
    hid, AbsoluteMode, KeyboardLayout, LogitechVirtualHid, MouseButton, TextMode,
};

fn main() -> logi_vhid::Result<()> {
    let mut device = LogitechVirtualHid::open()?;

    device.mouse_move_relative(10, -5)?;
    device.set_cursor_pos(400, 300, AbsoluteMode::Driver)?;
    device.click_at(400, 300, MouseButton::Left, AbsoluteMode::Driver)?;
    device.press_combo(&[hid::LEFT_CTRL, hid::C])?;
    device.type_text_layout("hello", KeyboardLayout::Us)?;
    device.type_text("中文", TextMode::Win32Unicode)?;

    Ok(())
}
```

Unicode text is entered through Windows `SendInput`, not the G HUB virtual keyboard. See below for the differences between absolute positioning modes.

### Command Line

Run `cargo run --release -- help` to see all commands. Common operations:

```powershell
cargo run --release -- info
cargo run --release -- cursor
cargo run --release -- move 10 -5
cargo run --release -- move-to 400 300 --mode driver
cargo run --release -- move-to 400 300 --mode win32
cargo run --release -- click 400 300 --button left
cargo run --release -- double-click 400 300 --button left
cargo run --release -- type "Hello" --mode us
cargo run --release -- type "中文" --mode unicode
cargo run --release -- window "Notepad" 100 100
```

Commands such as `move-to` and `click` accept `driver`, `win32`, or `set` to choose the cursor positioning method. `type` supports `us`, `uk`, `de`, `fr`, and `unicode`. `window` searches visible window titles for the given text; its coordinates start at the top-left of the window's client area, excluding the title bar and borders.

Unknown options, missing option values, repeated options, and extra arguments are rejected before a device is opened.

### C/C++ SDK

After a release is published, download the Windows x86_64 MSVC SDK ZIP from [GitHub Releases](https://github.com/lona-cn/logi-vhid/releases). It contains the DLL, `logi_vhid.dll.lib` import library, C header, LICENSE, and C/C++ examples. The import library is not the separate Rust `staticlib` file named `logi_vhid.lib`.

See [sdk/README.md](https://github.com/lona-cn/logi-vhid/blob/master/sdk/README.md) for linking, runtime requirements, and CMake commands. The examples only exercise error/query calls: they do not open a device or send desktop input. The G HUB driver is not included.

### Python

Python 3.12 or later, Windows x64. After a wheel is published on PyPI:

```powershell
python -m pip install logi-vhid
```

You can also install the `.whl` attached to the matching GitHub Release. Wheels include the DLL and need no Rust toolchain. To build and install directly from this source checkout, use `python -m pip install .` with the MSVC Rust toolchain installed.

The DLL requires the Microsoft Visual C++ v14 x64 runtime (`VCRUNTIME140.dll`). If it is missing, install the supported [x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist); the wheel does not install this system component.

The package loads its own `_native/logi_vhid.dll`; it does not search a source checkout's `target` directories. Set `LOGI_VHID_DLL` only when explicitly selecting a different DLL. Do not add the repository to `PYTHONPATH` for an installed package.

The following example sends real input to the desktop:

```python
from logi_vhid import LogitechVirtualHid, LEFT, MODE_SET_CURSOR_POS

with LogitechVirtualHid() as device:
    device.set_cursor(400, 300, MODE_SET_CURSOR_POS)
    device.move(10, 0)
    device.click(LEFT)
    device.type_unicode("中文")
```

Only one operation at a time can run on a Python instance. `close()` waits for the current operation to finish; calling it again has no effect. In C, do not use or close the same handle concurrently. See the header for which pointers must be freed and how long they remain valid.

The library owns the error string returned to C callers. Do not free or modify it; it remains valid until the next library error on that thread or until the thread exits. Python raises `ValueError` for text containing NUL characters or invalid button or positioning mode values before calling the DLL.

### Cursor Positioning

The virtual mouse supports relative movement but does not report absolute coordinates. To position the cursor at screen coordinates, choose one of these methods:

- `driver`: Moves to the target incrementally using the virtual mouse. Failures return an error; no other method is selected automatically.
- `win32`: Sends absolute mouse movement input through Windows `SendInput`.
- `set`: Calls Windows `SetCursorPos` directly to move the cursor, without using `SendInput`.

`cursor_pos` and the screen/window coordinate functions use physical pixels, regardless of display scaling. They restore the calling thread's DPI setting before returning. `virtual_desktop_rect()` can fail and returns `Result<Rect>`; use `?` to pass the error to your caller or handle it explicitly.

### Limitations and Notes

- Virtual joysticks, media keys, and keyboard indicator lights are not supported. Multiple virtual devices of the same type cannot be created at the same time.
- For HID text, the selected layout must match the target application's input layout. The library reads Caps Lock before typing and treats CRLF as one newline. Characters outside Unicode's Basic Multilingual Plane, such as many emoji, and characters requiring a dead key (an accent key pressed before a letter) return an error; use Unicode mode for those. Do not type manually or switch layouts while text is being entered.
- A sequence stops on the first error and attempts to release keys and mouse buttons it left pressed, without releasing those already held before the sequence. Earlier cursor movement and typed text are not undone.
- If Windows accepts only part of a Unicode input request, the library attempts to release the key it pressed and returns the original error. Text already entered is not undone.
- Logitech G HUB updates may affect compatibility.
- Commands interact with the Windows desktop. Some applications or games may detect or restrict virtual input; follow the target application's rules. This project does not bypass those restrictions or Windows security settings.

### Testing

The following checks do not send keyboard or mouse input. Run the Python commands in a virtual environment:

```powershell
cargo test --all-targets
cargo test --doc
python -m pip install -r tools/requirements-release.txt
python -B -m unittest discover -s python/tests -p "test_*.py"
python -B -m unittest discover -s tools -p "test_*.py"
```

The hardware test is ignored by default. It moves the cursor and presses/releases F24, then attempts to release test keys and restore the cursor. Run it only when those desktop actions are safe:

```powershell
cargo test --test hardware -- --ignored --nocapture --test-threads=1
```

### Build and Verify Release Packages

`Cargo.toml` is the version source. A tag must exactly match `v` followed by that version. Stable versions and `-alpha.N`, `-beta.N`, or `-rc.N` versions have corresponding Python versions; build metadata and unsupported forms are rejected.

From the Windows x64 source checkout:

```powershell
python -m venv target/release-tools
target/release-tools/Scripts/python.exe -m pip install -r tools/requirements-release.txt
target/release-tools/Scripts/python.exe tools/release.py build --tag v0.1.0 --dist dist
target/release-tools/Scripts/python.exe tools/release.py verify --tag v0.1.0 --dist dist
```

These commands do not create a tag or upload anything. Building requires Rust and MSVC; verification also needs CMake and its C/C++ compiler. The output contains the SDK ZIP, a `py3-none-win_amd64` wheel, `SHA256SUMS`, and draft release notes. One Cargo build supplies the same DLL to both packages. `verify` checks the hashes and metadata, compiles and runs the extracted SDK examples, and installs the wheel with `--no-index --no-deps` into a fresh temporary environment outside the checkout.

`verify-assets` accepts the same arguments and only checks files, versions, hashes, and DLL consistency. It can run on Linux and never builds or loads the DLL. All automatic smoke checks use null-handle/error paths, not real input.

### Publish a Version

After reviewing and committing the release contents:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

The tag workflow reuses CI, then builds the final native packages once with Rust 1.96.0. It creates a **draft** GitHub Release with the SDK, wheel, checksums, and verification notes. It will not overwrite an existing release.

Before publishing that draft, record the Windows/G HUB versions, observed real input, and whether all keys/buttons were released. Hosted CI does not perform this hardware verification.

Publishing the GitHub Release triggers a separate Linux job: it downloads and verifies the published files without rebuilding, then passes only that same wheel to the PyPI publishing job. The latter does not check out source or build packages. Its OIDC permission is limited to that job; no long-lived PyPI token is used. Crates.io publishing remains a separate maintainer action; CI only runs `cargo publish --dry-run`.

### One-Time PyPI Setup

Before the first public release, configure a [pending Trusted Publisher](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/) in your PyPI account:

| Field | Value |
| --- | --- |
| PyPI project | `logi-vhid` |
| GitHub owner | `lona-cn` |
| Repository | `logi-vhid` |
| Workflow filename | `release.yml` |
| Environment | `pypi` |

Create the `pypi` environment in the GitHub repository settings; requiring a reviewer is recommended. PyPI account configuration and environment protection rules are not created by these scripts. A pending publisher does not reserve the project name: confirm that your account can publish the project. No PyPI upload can succeed until the matching publisher is configured.

### License

MIT. See [LICENSE](LICENSE) for the full text.

## 简体中文

面向 Windows 的 Rust 键鼠输入库，通过 Logitech G HUB 创建虚拟键盘和鼠标。项目同时提供命令行工具、C 接口和 Python 模块。

### 功能

- 鼠标相对移动、点击、双击、拖动和滚轮操作。
- 光标定位：通过虚拟鼠标或 Windows 输入接口移动到指定坐标。
- 键盘按键、组合键、长按和重复按键。
- 按 US、UK、DE、FR 键盘布局输入文本，也可输入中文等 Unicode 文本。
- 按顺序执行一组键鼠操作，并查询窗口、显示器和坐标信息。

### 环境要求

- Windows 10 1703 或更新版本，x64。
- 已安装并运行 Logitech G HUB。
- 从源码编译需要 Rust 1.96 或更新版本；使用 Python 模块需要安装 Python。

### Rust 依赖

版本发布到 crates.io 后，可用以下命令将其添加到 Rust 项目：

```powershell
cargo add logi-vhid
```

### 编译

在项目目录运行：

```powershell
cargo build --release
```

默认情况下，可执行文件 `logi-vhid-demo.exe` 和库文件 `logi_vhid.dll` 位于 `target/release/`。若设置了 `CARGO_TARGET_DIR`，请到该目录的 `release/` 子目录查找。

### Rust 示例

这个示例会移动并点击鼠标、发送 Ctrl+C，再向当前应用输入文字。请确认这些操作不会影响正在进行的工作后再运行。

```rust
use logi_vhid::{
    hid, AbsoluteMode, KeyboardLayout, LogitechVirtualHid, MouseButton, TextMode,
};

fn main() -> logi_vhid::Result<()> {
    let mut device = LogitechVirtualHid::open()?;

    device.mouse_move_relative(10, -5)?;
    device.set_cursor_pos(400, 300, AbsoluteMode::Driver)?;
    device.click_at(400, 300, MouseButton::Left, AbsoluteMode::Driver)?;
    device.press_combo(&[hid::LEFT_CTRL, hid::C])?;
    device.type_text_layout("hello", KeyboardLayout::Us)?;
    device.type_text("中文", TextMode::Win32Unicode)?;

    Ok(())
}
```

Unicode 文本通过 Windows `SendInput` 输入，不经过 G HUB 虚拟键盘。绝对定位模式的区别见下文。

### 命令行

运行 `cargo run --release -- help` 查看所有命令。常用操作：

```powershell
cargo run --release -- info
cargo run --release -- cursor
cargo run --release -- move 10 -5
cargo run --release -- move-to 400 300 --mode driver
cargo run --release -- move-to 400 300 --mode win32
cargo run --release -- click 400 300 --button left
cargo run --release -- double-click 400 300 --button left
cargo run --release -- type "Hello" --mode us
cargo run --release -- type "中文" --mode unicode
cargo run --release -- window "Notepad" 100 100
```

`move-to`、`click` 等命令可用 `driver`、`win32` 或 `set` 选择光标定位方式。`type` 支持 `us`、`uk`、`de`、`fr` 和 `unicode`。`window` 在可见窗口的标题中查找指定文字；坐标从窗口内容区域（客户区）的左上角算起，不包括标题栏和边框。

未知选项、缺少选项值、重复选项和多余参数都会在打开设备前报错。

### C/C++ SDK

版本发布后，可从 [GitHub Releases](https://github.com/lona-cn/logi-vhid/releases) 下载 Windows x86_64 MSVC SDK ZIP。包内包含 DLL、`logi_vhid.dll.lib` 导入库、C 头文件、LICENSE 和 C/C++ 示例。导入库不是 Rust 单独生成的静态库 `logi_vhid.lib`，不要混用。

链接方法、运行库要求和 CMake 命令见 [sdk/README.md](https://github.com/lona-cn/logi-vhid/blob/master/sdk/README.md)。示例只调用错误查询等接口，不打开设备、不发送桌面输入。包内不包含 G HUB 驱动。

### Python

需要 Python 3.12 或更新版本、Windows x64。wheel 发布到 PyPI 后，可直接安装：

```powershell
python -m pip install logi-vhid
```

也可以安装对应 GitHub Release 附带的 `.whl`。wheel 内置 DLL，安装时不需要 Rust 工具链。从当前源码安装时，先准备 MSVC Rust 工具链，再运行 `python -m pip install .`。

DLL 需要 Microsoft Visual C++ v14 x64 运行库（`VCRUNTIME140.dll`）。如果缺少该运行库，请安装受支持的 [x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)；wheel 不会安装系统运行库。

包默认加载自身的 `_native/logi_vhid.dll`，不再搜索源码目录的 `target`。只有需要明确指定其他 DLL 时才设置 `LOGI_VHID_DLL`；使用已安装的包无需设置 `PYTHONPATH`。

以下示例会向当前桌面发送真实输入：

```python
from logi_vhid import LogitechVirtualHid, LEFT, MODE_SET_CURSOR_POS

with LogitechVirtualHid() as device:
    device.set_cursor(400, 300, MODE_SET_CURSOR_POS)
    device.move(10, 0)
    device.click(LEFT)
    device.type_unicode("中文")
```

同一个 Python 实例一次只能执行一个操作。`close()` 会等待当前操作结束；重复调用不会再次关闭设备。使用 C 接口时，不要同时操作或关闭同一个句柄。哪些指针需要释放、可以使用多久，见头文件说明。

C 接口返回的错误字符串由库管理，不要修改或释放。它在当前线程下一次库调用报错或线程退出前有效。Python 遇到含 NUL 字符的文本、无效的按钮值或定位模式值时，会在调用 DLL 前抛出 `ValueError`。

### 光标定位方式

虚拟鼠标只能报告相对移动，不能直接指定绝对坐标。需要移动到屏幕上的某个位置时，可选择以下方式：

- `driver`：通过虚拟鼠标逐步移动到目标位置。操作失败会返回错误，不会自动改用其他方式。
- `win32`：通过 Windows 的 `SendInput` 发送绝对移动输入。
- `set`：直接调用 Windows 的 `SetCursorPos` 改变光标位置，不使用 `SendInput`。

`cursor_pos` 和屏幕、窗口坐标函数均使用物理像素，不受显示缩放比例影响；返回前会恢复调用线程的 DPI 设置。`virtual_desktop_rect()` 可能失败，返回值为 `Result<Rect>`。可以用 `?` 将错误传给调用方，也可以自行处理。

### 限制与注意事项

- 不支持虚拟摇杆、媒体控制键、键盘指示灯，也不支持同时创建多个同类型虚拟设备。
- 使用 HID 输入文本时，所选键盘布局必须与目标应用的输入布局一致。库会在输入前读取 Caps Lock 状态，并将 CRLF 作为一次换行。超出 Unicode 基本多文种平面（BMP）的字符（如许多 emoji），以及需要先按重音键、再按字母键的字符，会返回错误；请改用 Unicode 模式。输入过程中不要手动按键或切换键盘布局。
- 一组操作遇到第一个错误后停止，并尝试释放执行过程中按下但尚未松开的键和鼠标按钮，不会释放执行前就已按住的键或按钮。已经完成的光标移动和文字输入不会撤销。
- 如果 Windows 只接受了部分 Unicode 输入事件，库会尝试松开已按下的键，并返回原始错误。已经输入的文字不会撤销。
- Logitech G HUB 更新可能影响兼容性。
- 命令会实际操作 Windows 桌面。部分应用或游戏可能识别或限制虚拟输入；请遵守目标应用的规则。本项目不绕过这些限制或 Windows 安全设置。

### 测试

以下检查不会发送键盘或鼠标输入。Python 命令请在虚拟环境中运行：

```powershell
cargo test --all-targets
cargo test --doc
python -m pip install -r tools/requirements-release.txt
python -B -m unittest discover -s python/tests -p "test_*.py"
python -B -m unittest discover -s tools -p "test_*.py"
```

硬件测试默认跳过。它会移动光标并按下、释放 F24，最后尝试释放测试按键并恢复光标。确认当前桌面允许这些操作后再运行：

```powershell
cargo test --test hardware -- --ignored --nocapture --test-threads=1
```

### 本地构建与验收发行包

版本统一读取 `Cargo.toml`，标签必须是 `v` 加该版本。支持正式版本及 `-alpha.N`、`-beta.N`、`-rc.N` 的 Python 版本转换；不接受 build metadata 或不支持的版本形式。

在 Windows x64 源码目录运行：

```powershell
python -m venv target/release-tools
target/release-tools/Scripts/python.exe -m pip install -r tools/requirements-release.txt
target/release-tools/Scripts/python.exe tools/release.py build --tag v0.1.0 --dist dist
target/release-tools/Scripts/python.exe tools/release.py verify --tag v0.1.0 --dist dist
```

这些命令不会创建标签或上传文件。构建需要 Rust 和 MSVC，验收还需要 CMake 及 C/C++ 编译器。产物包括 SDK ZIP、`py3-none-win_amd64` wheel、`SHA256SUMS` 和草稿发布说明；SDK 与 wheel 复用一次 Cargo 构建得到的同一个 DLL。`verify` 会检查校验和及元数据，编译、运行解包后的 SDK 示例，并在源码目录以外的新临时环境中，用 `--no-index --no-deps` 安装 wheel。

`verify-assets` 使用相同参数，只校验文件、版本、哈希和 DLL 一致性；可在 Linux 运行，不构建或加载 DLL。所有自动冒烟只走空句柄、错误查询等路径，不发送真实输入。

### 发布版本

审查并提交发布内容后，再创建并推送对应标签：

```powershell
git tag v0.1.0
git push origin v0.1.0
```

标签工作流先复用 CI 检查，再用 Rust 1.96.0 构建一次最终发行包，创建附带 SDK、wheel、校验和及验收说明的 **GitHub 草稿 Release**；不会覆盖已有 Release。

将草稿发布前，需要记录实测的 Windows/G HUB 版本、真实输入结果，以及所有按键、按钮是否已释放。托管 CI 不会执行这项硬件验收。

GitHub Release 发布后，独立的 Linux 任务会下载并校验已发布文件，不重新构建；随后只把同一份 wheel 交给 PyPI 上传任务。上传任务不检出源码或构建包，OIDC 权限仅在该任务启用，不保存长期 PyPI Token。crates.io 仍由维护者单独发布，CI 只做 `cargo publish --dry-run`。

### 首次发布前的 PyPI 配置

首次公开发布前，在自己的 PyPI 账号中配置 [pending Trusted Publisher](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/)：

| 字段 | 值 |
| --- | --- |
| PyPI 项目 | `logi-vhid` |
| GitHub owner | `lona-cn` |
| Repository | `logi-vhid` |
| Workflow filename | `release.yml` |
| Environment | `pypi` |

同时在 GitHub 仓库设置中创建 `pypi` environment，建议启用 required reviewers。脚本不会代替你修改 PyPI 账号配置或环境保护规则。pending publisher 不会预留项目名，需要确认账号有权发布该项目；配置匹配之前，PyPI 上传不会成功。

### 许可证

采用 MIT 许可证，完整内容见 [LICENSE](LICENSE)。
