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

### C API and Python

The C header is [include/logi_vhid.h](include/logi_vhid.h). After a release build, the DLL is at `target/release/logi_vhid.dll`, or in the `release/` subdirectory of `CARGO_TARGET_DIR` if set.

The Python module is [python/logi_vhid.py](python/logi_vhid.py). Add the project's `python/` directory to `PYTHONPATH` before importing it. Set `LOGI_VHID_DLL` to use a specific DLL; otherwise, the module searches `target/release/` and then `target/debug/`.

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

The following checks do not send keyboard or mouse input:

```powershell
cargo test --all-targets
cargo test --doc
python -B -m unittest discover -s python -p "test_*.py"
```

The hardware test is ignored by default. It moves the cursor and presses/releases F24, then attempts to release test keys and restore the cursor. Run it only when those desktop actions are safe:

```powershell
cargo test --test hardware -- --ignored --nocapture --test-threads=1
```

### Before Publishing

Windows CI checks Rust 1.96 and stable without sending desktop input. Documentation examples are compiled but not executed.

Before uploading a release:

- Set `package.repository` to the public source repository URL.
- Review and commit the release contents, then record the commit and version tag.
- Record the Windows and G HUB versions used for the hardware test, what input you observed, and whether the test left any keys or buttons pressed.
- Run the following from the reviewed checkout with no uncommitted changes. This checks the crate package without uploading it:

```powershell
cargo publish --dry-run --locked --registry crates-io
```

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

### C 接口和 Python

C 头文件见 [include/logi_vhid.h](include/logi_vhid.h)。编译 release 版本后，DLL 位于 `target/release/logi_vhid.dll`；若设置了 `CARGO_TARGET_DIR`，则位于该目录的 `release/` 子目录。

Python 模块见 [python/logi_vhid.py](python/logi_vhid.py)。导入前将项目的 `python/` 目录加入 `PYTHONPATH`。如需指定 DLL，请设置 `LOGI_VHID_DLL`；否则模块会依次查找 `target/release/` 和 `target/debug/`。

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

以下检查不会发送键盘或鼠标输入：

```powershell
cargo test --all-targets
cargo test --doc
python -B -m unittest discover -s python -p "test_*.py"
```

硬件测试默认跳过。它会移动光标并按下、释放 F24，最后尝试释放测试按键并恢复光标。确认当前桌面允许这些操作后再运行：

```powershell
cargo test --test hardware -- --ignored --nocapture --test-threads=1
```

### 发布前检查

Windows CI 会使用 Rust 1.96 和 stable 执行不发送桌面输入的检查。文档示例只编译，不执行。

上传版本前：

- 将公开源码仓库地址写入 `package.repository`。
- 审查并提交发布内容，记录对应的 commit 和版本标签。
- 记录硬件测试使用的 Windows、G HUB 版本、实际观察到的输入，以及测试结束后是否仍有按键或鼠标按钮未松开。
- 在已审查且没有未提交改动的项目目录运行以下命令，检查 crate 包；该命令不会上传：

```powershell
cargo publish --dry-run --locked --registry crates-io
```

### 许可证

采用 MIT 许可证，完整内容见 [LICENSE](LICENSE)。
