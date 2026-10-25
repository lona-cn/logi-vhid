# logi-vhid-mcp

[English](#english) · [简体中文](#简体中文)

## English

A local **stdio MCP server for Windows x64 keyboard and mouse input**. It calls the [logi-vhid Rust library](https://github.com/lona-cn/logi-vhid) directly and exposes six tools for input, driver status and physical-pixel coordinates.

**Input is disabled by default.** Screenshots come from the host's existing observation tools. This executable does not automatically replace a computer-control tool, provide a remote HTTP service, install a driver or elevate privileges.

### Requirements

- Windows 10 version 1703 or later, x64.
- For input: an interactive, active user session on the accessible **Default input desktop**, with the intended target already in the foreground.
- For nonempty input requests: the Logitech G HUB virtual bus driver must be installed and accessible. On the first accepted request, the server opens the bus and creates or verifies both virtual HID devices, then reuses its handles. **Unicode and Win32 positioning modes also require this initialization.** The driver and G HUB installer are not included; compatibility with every G HUB version or target application is not established.
- The prebuilt executable needs neither Python nor a Rust toolchain. Source builds additionally require Rust >=1.96, the x64 MSVC toolchain, Visual Studio C++ build tools and the Windows SDK.

`vhid_desktop_info` does not open the driver. `vhid_status` opens only the control bus to query it, without creating HID children or sending input; a missing driver is reported as `driver.state: "not_found"`. Safe protocol checks can therefore run without G HUB.

### Download and configure

After a version containing MCP assets is published, download the following from [GitHub Releases](https://github.com/lona-cn/logi-vhid/releases). **0.1.1 is being prepared; public v0.1.0 has no MCP assets.** A release must be published before these new packages can be installed by download.

- `logi-vhid-mcp-<version>-windows-x86_64-msvc.exe`
- `SHA256SUMS`

The MCP executable is a separate release asset, not part of the C/C++ SDK ZIP or Python wheel. No SDK DLL or Python package installation is needed to use it. Replace the example version and path below with the downloaded file.

To inspect its hash from the download directory:

```powershell
Get-FileHash .\logi-vhid-mcp-0.1.1-windows-x86_64-msvc.exe -Algorithm SHA256
```

Compare it with the entry for the same filename in the release's `SHA256SUMS`. Matching hashes establish consistency with that manifest, not trust in an unknown publisher. If Windows reports a missing Microsoft Visual C++ runtime DLL, use Microsoft's official [x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist); this server does not install system components.

For a host using the `mcpServers` configuration format:

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

Use an **absolute executable path**. Other hosts may use a different configuration format, but the transport is still stdio. The host starts the process and communicates over stdin/stdout; launching the server in a terminal does not open a GUI. Service stdout contains JSON-RPC only, and diagnostics go to stderr.

With `args: []`, valid input requests return `input_disabled` before native target checks or input-device initialization. To permit input, the user must explicitly choose `args: ["--allow-input"]` and restart the server. **This startup flag is not approval of each operation.** The host remains responsible for obtaining approval for the exact target, action and values. There is no `approved` request field or built-in confirmation dialog.

Startup accepts no arguments, `--allow-input`, `--allow-input=true`, `--allow-input=false`, `--help`, or `--version`, with at most one argument. The explicit Boolean forms support bundle settings; only lowercase `true` and `false` are accepted. No argument or `--allow-input=false` keeps input disabled. Help/version print text and exit without creating a worker or opening the driver. Unknown, repeated or combined arguments exit with code 2.

### Native MCPB installation

For a Windows x64 host that supports **native binary MCPB bundles**, use `logi-vhid-mcp-0.1.1-windows-x86_64-msvc.mcpb` after publication. Compare its `Get-FileHash -Algorithm SHA256` result with the same filename in `SHA256SUMS`, then use the host's local bundle import/install control. Review the publisher, permissions and [privacy notice](https://github.com/lona-cn/logi-vhid/blob/master/docs/privacy.md#english) before accepting. A `.mcpb` file is a packaging format, **not** approval or an automatic submission to Claude's official directory.

The bundle contains `manifest.json`, `server/logi-vhid-mcp.exe`, `icon.png`, `LICENSE` and this README. It uses the same executable as the standalone asset; no Node.js/Python launcher or separate SDK DLL is needed. The manifest supports only `win32`.

In the host's bundle settings, keep **`allow_input` disabled (`false`)** for queries. Only when explicitly permitting input, enable the Boolean setting and restart/reconnect the server if required by the host. The manifest passes `--allow-input=false` or `--allow-input=true` directly to the executable; there is no launcher, environment-variable permission switch or automatic approval. Per-operation approval remains required.

### Cline: shortest manual setup

Follow [Cline's official MCP configuration guide](https://docs.cline.bot/mcp/configuring-mcp-servers): **MCP Servers → Configure → Configure MCP Servers**, then merge the `mcpServers` entry above into the existing settings (do not replace other servers). Use the downloaded exe's absolute path and `args: []`; keep automatic approval off and do not add input tools to an `autoApprove` list. Reconnect and confirm that the six tools appear, then call `vhid_desktop_info` with `{}`. Missing G HUB does not prevent that query. Cline CLI can use the same stdio entry in its own MCP settings.

This is a local stdio configuration, not a remote URL, npm package or Cline-specific plugin. Input is opt-in via `["--allow-input"]` and still needs host approval for every concrete operation. Cline marketplace inclusion and actual host/input compatibility are **not verified**; use the host's existing observation tools, not this server, for screenshots.

### Support status and policies

The following are installation targets, **not a tested compatibility matrix**. No host version or test date is claimed.

| Target | Prepared installation route | Status |
| --- | --- | --- |
| Claude Desktop / another native MCPB-capable Windows host | Local `.mcpb` import; depends on native binary and array-setting support | Not host-tested; not officially listed/certified |
| Cline IDE extension / CLI on Windows | Manual local stdio exe configuration | Not host-tested; marketplace submission not performed |
| Other local stdio MCP hosts on Windows x64 | Absolute exe path and empty args | Not host-tested; approval/observation controls must be checked |
| Official MCP Registry | Generated `server.json` pointing to the exact release MCPB | Metadata prepared only; namespace/publication not registered |
| Real Windows/G HUB/target-application input | Separate explicitly authorized acceptance | Not performed for this publication preparation; no demo recording supplied |

Ordinary support: [GitHub Issues](https://github.com/lona-cn/logi-vhid/issues). Read the [privacy notice](https://github.com/lona-cn/logi-vhid/blob/master/docs/privacy.md#english) and [security policy](https://github.com/lona-cn/logi-vhid/blob/master/SECURITY.md#english). GitHub Private Vulnerability Reporting is currently disabled and no dedicated private intake is advertised: use a public issue **only to coordinate contact without vulnerability details, PoCs, secrets or user data**. Do not treat that issue as confidential. Copy-ready descriptions and channel checklists: [LISTING.md](https://github.com/lona-cn/logi-vhid/blob/master/crates/logi-vhid-mcp/LISTING.md).


### Tools and limits

| Tool | Arguments | Purpose |
| --- | --- | --- |
| `vhid_status` | `{}` | `allow_input`, worker/device state, bus-only driver probe and `last_operation` |
| `vhid_desktop_info` | `{}` or `window: {hwnd, pid}` | Virtual desktop, monitors, cursor, foreground/selected window, session, input desktop and observation timestamp |
| `vhid_mouse` | `target`, `action`, optional `timeout_ms` | One relative move, absolute move, click, scroll or drag |
| `vhid_key` | `target`, `keys`, optional `hold_ms`, `timeout_ms` | Press keys in order, hold, then release in reverse order |
| `vhid_type_text` | `target`, `text`, explicit `mode`, optional `timeout_ms` | HID-layout text or Win32 Unicode text |
| `vhid_stop` | `{}` | Cancel this service's operation and attempt release through existing handles; never opens a device for cleanup |

Input requests use `target: {"hwnd": "0x12345", "pid": 4321}`. HWND must be a nonzero, `0x`-prefixed hexadecimal string that fits the native pointer; PID must be a nonzero integer. Unknown fields are rejected. The tool input schemas returned by `tools/list` give the exact JSON shapes and enum values.

| Parameter | Meaning and bounds |
| --- | --- |
| `timeout_ms` | Defaults to 10000; range 1–30000; starts at admission and includes device initialization and execution |
| `hold_ms` | Keys/clicks/drags default to 20; range 1–5000; cannot exceed `timeout_ms` |
| `keys` | Nonempty, no duplicates, at most six ordinary keys plus eight modifiers; uppercase HID names such as `A`, `NUM_1`, `ENTER`, `LEFT_CTRL`, not virtual-key numbers or aliases |
| Text `mode` | Required: `hid_us`, `hid_uk`, `hid_de`, `hid_fr`, or `unicode` |
| `text` | At most 1024 Unicode scalars, no NUL; an empty string completes without initializing input devices when input is enabled |
| Point | `{"space": "screen", "x": 400, "y": 300}` or `{"space": "client", "x": 100, "y": 100}`; physical pixels, not DPI-scaled logical units |
| Position `mode` | `driver` by default; explicit `win32` or `set` selects Win32 SendInput or SetCursorPos; no automatic fallback |
| `button` | Defaults to `left`; also `right`, `middle`, `back`, `forward` |

The mouse `action` has a `type` field:

| `type` | Fields | Behavior |
| --- | --- | --- |
| `move_relative` | `dx`, `dy` | Signed 16-bit HID counts, not guaranteed pixels; cannot both be zero |
| `move_to` | `at`, optional `mode` | Move to a point and verify the observed position |
| `click` | `at`, optional `button`, `clicks`, `hold_ms`, `mode` | One or two clicks; `clicks` defaults to 1, with a 40 ms interval between two complete clicks |
| `scroll` | `delta`, optional `at`, `mode` | Nonzero signed 8-bit HID wheel increment, not pixels or lines; without `at`, use the current cursor and ignore positioning `mode` |
| `drag` | `from`, `to`, optional `button`, `hold_ms`, `mode` | Position, press, hold, move, verify the endpoint and release within one operation |

HID text does not change the target application's keyboard layout; it must already match. Mapping is incremental, so an unsupported character can fail **after a prefix was entered**. Unicode uses Win32 SendInput, not HID, clipboard insertion or input-method switching. CR/LF handling differs: HID turns line breaks into Enter keys, while Unicode sends literal UTF-16 text.

### Observe first, then act

1. Call `vhid_desktop_info` with `{}` and observe the desktop using the host's screenshot/accessibility tools.
2. Choose the intended foreground window's `hwnd` and `pid`. Query `vhid_desktop_info` with its `window` if client coordinates are needed.
3. Obtain approval for the concrete operation. Send one input request using the freshly observed target.
4. Inspect the result and observe the application again. Do not automatically retry input.

The following are **`tools/call` parameter examples**, not shell commands. The HWND/PID and coordinates are illustrative: replace them with a fresh observation. Enabled input affects the real desktop; use only an approved, disposable test window for experiments.

Query the desktop:

```json
{"name": "vhid_desktop_info", "arguments": {}}
```

Click a client-relative point:

```json
{
  "name": "vhid_mouse",
  "arguments": {
    "target": {"hwnd": "0x12345", "pid": 4321},
    "action": {"type": "click", "at": {"space": "client", "x": 100, "y": 100}, "button": "left", "clicks": 1}
  }
}
```

Press Shift+A:

```json
{
  "name": "vhid_key",
  "arguments": {
    "target": {"hwnd": "0x12345", "pid": 4321},
    "keys": ["LEFT_SHIFT", "A"],
    "hold_ms": 20
  }
}
```

Type Unicode text:

```json
{
  "name": "vhid_type_text",
  "arguments": {
    "target": {"hwnd": "0x12345", "pid": 4321},
    "text": "中文测试",
    "mode": "unicode"
  }
}
```

To request cancellation, call `vhid_stop` with `{}`. There are no cross-request key-down/button-down tools or arbitrary input scripts.

### Results, cancellation and safety

Known-tool results use `structuredContent` with `ok`, `data` and `error`; tool failures also set MCP `isError: true`. Successful envelopes have `error: null`. Argument decoding errors may have `data: null`. Input outcomes include:

- `state`: `completed`, `rejected`, `failed`, `cancelled`, `timed_out` or `stopping`.
- `submission`: `none`, `complete` or `partial_or_unknown`. **Complete submission is not application acceptance.** Final cleanup can fail after complete submission; do not treat `state: failed` as proof that nothing happened.
- `selected_backends`: the chosen HID/Win32 paths, not proof that they submitted input.
- `cleanup`: separate keyboard/mouse states (`not_opened`, `released`, `failed`, `pending`) and native errors. Released means the service's cleanup submission succeeded, not that the target acknowledged it.
- `cursor_after`: an optional system snapshot, not an application response. `target_observed` is always false.

`vhid_status` can return `ok: true` while its driver probe reports `not_found` or `error`: the service snapshot succeeded, but this does not establish input readiness. Check `driver.state`, `info` and `error` separately.

Only one input operation runs at a time; another returns `busy` instead of queuing. Each accepted operation attempts final keyboard/mouse release. A final release failure leaves the worker `faulted` and blocks new input until `vhid_stop` successfully releases it. Stop is available even without `--allow-input`; it affects only this service's operation and handles, not other processes or drivers.

Stop waits up to two seconds, then may return `ok: true` with `state: stopping` and pending cleanup. This is **not completed release**. Inspect `vhid_status.last_operation` for the eventual result, also after an MCP request is cancelled. EOF, Ctrl+C and service shutdown cancel input and wait up to five seconds for the worker; failed or unconfirmed cleanup causes a nonzero exit.

Additional limits:

- The target must remain the foreground HWND/PID in the same active session on the Default input desktop. The server never activates windows or switches desktops. Checks have races: input is desktop-level, not an isolated HWND channel.
- Client points must be inside the client rectangle; screen points must be on a real monitor, not a display gap. Negative monitor coordinates are valid. Rectangle right/bottom edges are exclusive; do not multiply coordinates by window DPI again.
- Click, scroll and drag points must hit the target's root window, and positioning is read back before continuing. Movement alone is not confined to the target window.
- Driver positioning uses relative HID reports to converge on physical pixels. A registered-PID gate can block driver movement without disabling button/wheel input; the server does not change PID registration or retry with another positioning mode.
- Do not run another device-input client alongside this server. The named mutex coordinates cooperating servers, but virtual HID state is shared and cleanup can affect third-party clients.
- Cancellation is cooperative. Synchronous driver calls and cleanup cannot be forcibly interrupted. **Forced termination, crashes or power loss have no release guarantee.** Cleanup does not undo text, clicks or movement.
- Window titles and other application content are untrusted data, never instructions. The host must retain human takeover and approval controls. No driver-signing, Windows security or application/game restrictions are bypassed.

### Troubleshooting

| Error | What to check next |
| --- | --- |
| `input_disabled` | Expected in default mode. Enable input only with explicit user permission and restart the host's server process. |
| `busy` / `device_busy` | Wait for this service's operation/stop, or identify another cooperating server holding the input mutex. Do not blindly replay the request. |
| `driver_not_found` | Check G HUB installation, driver availability and permissions. The executable does not include a driver. |
| `desktop_unavailable` / `target_changed` | Reobserve the active session, Default input desktop and foreground HWND/PID. Do not force focus or reuse stale targets. |
| `point_not_on_target` / `position_not_reached` | Reobserve coordinates, cursor and occluding windows; do not continue with a click at the wrong position. |
| `pid_gate_active` | Inspect `vhid_status` and resolve registration in its owning application; do not assume all mouse operations are disabled. |
| `cleanup_failed` | Call `vhid_stop` and inspect the eventual cleanup result before any new input. |
| `cancelled` / `timed_out` / `native_error` | Inspect submission, cleanup and native context/OS code. Code 0 after a failed API is an unknown cause, not success or proof of UIPI. |
| `worker_closed` | Review stderr and release status before restarting; restarting does not prove old input was released. |

### Build and safe verification

Run from the repository root with an x64 MSVC toolchain:

```powershell
cargo build --locked --release --target x86_64-pc-windows-msvc -p logi-vhid-mcp --bin logi-vhid-mcp
$metadata = cargo metadata --locked --format-version 1 --no-deps | ConvertFrom-Json
$exe = Join-Path $metadata.target_directory "x86_64-pc-windows-msvc/release/logi-vhid-mcp.exe"
& $exe --help
```

Use Cargo metadata rather than assuming `target/release`: Cargo configuration and `CARGO_TARGET_DIR` can change the output directory. The source package is not published to crates.io or PyPI; releases distribute the executable independently.

These checks **do not send desktop input**. The Python smoke uses only the standard library and requires Python >=3.12; it never passes `--allow-input`:

```powershell
cargo test --locked -p logi-vhid-mcp --all-targets
python tools/mcp_smoke.py --exe "$exe"
```

The smoke checks initialize, discovery, desktop information, rejection of a valid input request, stop without devices, and normal EOF exit. A headless desktop may return a structured error instead of coordinates. These checks do not establish real input compatibility; hardware/target-application acceptance requires separate, concrete authorization.

For the shared version, SDK/wheel/exe/MCPB/Registry metadata build, five-asset checksums and release acceptance, see the [release maintenance guide](https://github.com/lona-cn/logi-vhid/blob/master/docs/releasing.md). License: [MIT](https://github.com/lona-cn/logi-vhid/blob/master/LICENSE).

## 简体中文

面向 **Windows x64 的本地 stdio MCP 键鼠输入服务**，直接调用 [logi-vhid Rust 库](https://github.com/lona-cn/logi-vhid)，提供六个工具，用于输入、驱动状态和物理像素坐标查询。

**默认禁止输入。** 截图继续由宿主现有的观察工具负责。这个程序不会自动替换既有 computer 控制工具，不提供远程 HTTP 服务，不安装驱动，也不自动提权。

### 环境要求

- Windows 10 1703 或更新版本，x64。
- 输入需要可交互的活跃用户会话、可访问的 **Default 输入桌面**，且目标窗口已经处于前台。
- 非空输入请求需要已安装且可访问的 Logitech G HUB 虚拟总线驱动。第一次接受输入时，服务打开总线、创建或检查键盘和鼠标两个虚拟设备，之后复用句柄。**选择 Unicode 或 Win32 定位模式也需要这一步初始化。** 项目不附带驱动或 G HUB 安装程序，也没有证明所有 G HUB 版本、目标软件均兼容。
- 使用预编译 exe 不需要 Python 或 Rust。从源码构建还需要 Rust >=1.96、x64 MSVC toolchain、Visual Studio C++ build tools 和 Windows SDK。

`vhid_desktop_info` 不打开驱动。`vhid_status` 只打开控制总线查询，不创建 HID 子设备、不发送输入；没有驱动时返回 `driver.state: "not_found"`。因此，安全协议检查不要求安装 G HUB。

### 下载与配置

含 MCP 资产的版本发布后，从 [GitHub Releases](https://github.com/lona-cn/logi-vhid/releases) 下载。**当前准备版本为 0.1.1；公开 v0.1.0 没有 MCP 附件。** 新包须先正式发版才能从 Release 安装。

- `logi-vhid-mcp-<版本>-windows-x86_64-msvc.exe`
- `SHA256SUMS`

MCP exe 是独立附件，不在 C/C++ SDK ZIP 或 Python wheel 内；使用它不需要复制 SDK DLL 或安装 Python 包。下列示例的版本和路径请替换为实际文件。

在下载目录查看哈希：

```powershell
Get-FileHash .\logi-vhid-mcp-0.1.1-windows-x86_64-msvc.exe -Algorithm SHA256
```

与同一 Release 的 `SHA256SUMS` 中对应文件名的记录比较。哈希一致只能证明文件与清单一致，不能证明未知发布者可信。如果 Windows 提示缺少 Microsoft Visual C++ 运行库 DLL，请使用微软官方的 [x64 Redistributable](https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist)；服务本身不会安装系统组件。

采用 `mcpServers` 格式的宿主可这样配置：

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

`command` 必须是 exe 的**绝对路径**。其他宿主的配置格式可能不同，但传输方式同样为 stdio。宿主负责启动进程，通过 stdin/stdout 通信；在终端直接启动不会打开 GUI。服务模式 stdout 只输出 JSON-RPC，诊断信息写入 stderr。

`args: []` 下，合法输入请求会在检查原生目标、初始化输入设备之前返回 `input_disabled`。只有用户明确允许输入时，才将 `args` 改为 `["--allow-input"]` 并重启服务。**启动开关不代表每次操作均已获批**；宿主仍须就具体目标、动作和值取得批准。请求没有 `approved` 字段，也没有内置确认窗口。

启动支持无参数，或单独传入 `--allow-input`、`--allow-input=true`、`--allow-input=false`、`--help`、`--version`，最多一个参数。布尔形式用于包设置，只接受小写 `true` 和 `false`；无参数或 `--allow-input=false` 均禁止输入。帮助和版本会输出文本并退出，不创建 worker、不打开驱动。未知、重复或组合参数退出码为 2。

### 原生 MCPB 安装

对于支持**原生 binary MCPB 包**的 Windows x64 宿主，版本发布后可使用 `logi-vhid-mcp-0.1.1-windows-x86_64-msvc.mcpb`。用 `Get-FileHash -Algorithm SHA256` 与 `SHA256SUMS` 同名记录比较后，通过宿主的本地包导入/安装入口安装。接受前检查发布者、权限及 [隐私说明](https://github.com/lona-cn/logi-vhid/blob/master/docs/privacy.md#简体中文)。`.mcpb` 是打包格式，**不代表获批，也不是向 Claude 官方目录自动投稿**。

包内固定为 `manifest.json`、`server/logi-vhid-mcp.exe`、`icon.png`、`LICENSE` 和本 README，与独立 exe 使用同一个程序，不需要 Node.js/Python 启动器或单独的 SDK DLL。manifest 平台仅为 `win32`。

宿主包设置中的 **`allow_input` 默认保持关闭（`false`）**，用于查询。只有明确允许输入时才开启这个布尔设置，并按宿主要求重启/重连。manifest 会直接向 exe 传入 `--allow-input=false` 或 `--allow-input=true`，不使用启动器、环境变量授权开关或自动批准。逐次操作批准仍不可省略。

### Cline 最短手动接入

按 [Cline 官方 MCP 配置说明](https://docs.cline.bot/mcp/configuring-mcp-servers)，进入 **MCP Servers → Configure → Configure MCP Servers**，将上面的 `mcpServers` 条目合并到现有配置，不覆盖其他服务。使用下载 exe 的绝对路径和 `args: []`，保持自动批准关闭，不将输入工具加入 `autoApprove` 列表。重连后确认六个工具出现，先用 `{}` 调用 `vhid_desktop_info`；该查询不需要 G HUB。Cline CLI 可在其自身 MCP 设置中使用同一个 stdio 条目。

这是本地 stdio 配置，不是远程 URL、npm 包或 Cline 专用插件。只有显式选择 `["--allow-input"]` 才启用输入，且宿主仍需逐次批准具体操作。**尚未验证 Cline 市场收录或实际宿主/输入兼容性**；截图继续使用宿主既有观察工具。

### 支持状态与政策

以下是准备接入的目标，**不是已通过测试的兼容矩阵**，不声称任何宿主测试版本或日期。

| 目标 | 已准备的安装方式 | 状态 |
| --- | --- | --- |
| Claude Desktop / 其他支持原生 MCPB 的 Windows 宿主 | 本地导入 `.mcpb`；依赖 binary 及数组设置支持 | 未实测宿主，未官方收录/认证 |
| Windows 上的 Cline IDE 扩展 / CLI | 手动配置本地 stdio exe | 未实测宿主，未提交市场 |
| 其他 Windows x64 本地 stdio MCP 宿主 | exe 绝对路径和空参数 | 未实测，需核对批准/观察控制 |
| 官方 MCP Registry | 生成指向精确 Release MCPB 的 `server.json` | 仅准备元数据，未登记 namespace/发布 |
| 真实 Windows/G HUB/目标应用输入 | 另行明确授权后验收 | 本次上架准备未执行，无演示录屏 |

普通支持：[GitHub Issues](https://github.com/lona-cn/logi-vhid/issues)。请阅读 [隐私说明](https://github.com/lona-cn/logi-vhid/blob/master/docs/privacy.md#简体中文) 和 [安全政策](https://github.com/lona-cn/logi-vhid/blob/master/SECURITY.md#简体中文)。仓库目前未启用 GitHub Private Vulnerability Reporting，也没有已公布的专用私密接收通道：公开 Issue **只能用于不含漏洞细节、PoC、密钥或用户数据的联系协调**，不是保密报告。可复制的介绍与渠道清单见 [LISTING.md](https://github.com/lona-cn/logi-vhid/blob/master/crates/logi-vhid-mcp/LISTING.md)。


### 工具与参数

| 工具 | 参数 | 用途 |
| --- | --- | --- |
| `vhid_status` | `{}` | 查询 `allow_input`、worker/设备状态、控制总线驱动探测和 `last_operation` |
| `vhid_desktop_info` | `{}` 或 `window: {hwnd, pid}` | 查询虚拟桌面、显示器、光标、前台/指定窗口、会话、输入桌面和观察时间 |
| `vhid_mouse` | `target`、`action`，可选 `timeout_ms` | 一次相对移动、绝对定位、点击、滚轮或拖拽 |
| `vhid_key` | `target`、`keys`，可选 `hold_ms`、`timeout_ms` | 按顺序按下，按住后逆序释放 |
| `vhid_type_text` | `target`、`text`、必须指定的 `mode`，可选 `timeout_ms` | HID 布局文本或 Win32 Unicode 文本 |
| `vhid_stop` | `{}` | 取消本服务操作，通过已有句柄尝试释放；不为清理打开新设备 |

输入使用 `target: {"hwnd": "0x12345", "pid": 4321}`。HWND 必须是非零、带 `0x` 前缀且可表示为本机指针的十六进制字符串；PID 必须是非零整数。未知字段会被拒绝。`tools/list` 返回的输入 schema 列出了准确的 JSON 结构和枚举值。

| 参数 | 含义与范围 |
| --- | --- |
| `timeout_ms` | 默认 10000，范围 1–30000；从请求被接受时计时，包含设备初始化和执行 |
| `hold_ms` | 按键、点击和拖拽默认 20，范围 1–5000；不能超过 `timeout_ms` |
| `keys` | 非空、不重复，最多六个普通键加八个修饰键；使用 `A`、`NUM_1`、`ENTER`、`LEFT_CTRL` 等大写 HID 名称，不接受虚拟键数字或别名 |
| 文本 `mode` | 必须指定 `hid_us`、`hid_uk`、`hid_de`、`hid_fr` 或 `unicode` |
| `text` | 最多 1024 个 Unicode scalar，拒绝 NUL；已允许输入时，空串直接完成，不初始化输入设备 |
| 坐标点 | `{"space": "screen", "x": 400, "y": 300}` 或 `{"space": "client", "x": 100, "y": 100}`；单位为物理像素，不是 DPI 缩放后的逻辑单位 |
| 定位 `mode` | 默认 `driver`；显式 `win32`、`set` 分别使用 Win32 SendInput、SetCursorPos，不会自动回退 |
| `button` | 默认 `left`，也接受 `right`、`middle`、`back`、`forward` |

鼠标 `action` 使用 `type` 区分操作：

| `type` | 字段 | 行为 |
| --- | --- | --- |
| `move_relative` | `dx`、`dy` | 有符号 16 位 HID counts，不保证等于像素；不能同时为 0 |
| `move_to` | `at`，可选 `mode` | 定位到指定点，并检查实际光标位置 |
| `click` | `at`，可选 `button`、`clicks`、`hold_ms`、`mode` | `clicks` 只接受 1 或 2，默认 1；双击在两个完整点击之间等待 40 ms |
| `scroll` | `delta`，可选 `at`、`mode` | 非零有符号 8 位 HID 滚轮增量，不是像素或行数；未指定 `at` 时使用当前光标，定位 `mode` 不起作用 |
| `drag` | `from`、`to`，可选 `button`、`hold_ms`、`mode` | 在一个操作内完成定位、按下、等待、移动、确认终点和释放 |

HID 文本不会改变目标应用的键盘布局，应用布局应与所选模式一致。字符逐个映射，不支持的字符可能在**已经输入一部分文字之后**才报错。Unicode 使用 Win32 SendInput，不伪装成 HID，不修改剪贴板或切换输入法。两种路径的换行处理不同：HID 将换行转换为 Enter，Unicode 发送原始 UTF-16 文本。

### 先观察，再操作

1. 用 `{}` 调用 `vhid_desktop_info`，并通过宿主的截图/辅助功能工具观察桌面。
2. 选择目标前台窗口的 `hwnd`、`pid`。需要客户区坐标时，再以该 `window` 查询窗口详情。
3. 就具体操作取得批准，使用刚观察到的目标发送一次输入请求。
4. 检查返回结果，并再次观察应用。不要自动重试输入。

以下是 **`tools/call` 的参数示例**，不是 shell 命令。窗口标识和坐标仅用于展示格式，必须替换为新观察到的值。开启输入后会实际操作桌面；试验仅使用明确获准的新建、未保存测试窗口。

查询桌面：

```json
{"name": "vhid_desktop_info", "arguments": {}}
```

点击客户区坐标：

```json
{
  "name": "vhid_mouse",
  "arguments": {
    "target": {"hwnd": "0x12345", "pid": 4321},
    "action": {"type": "click", "at": {"space": "client", "x": 100, "y": 100}, "button": "left", "clicks": 1}
  }
}
```

按 Shift+A：

```json
{
  "name": "vhid_key",
  "arguments": {
    "target": {"hwnd": "0x12345", "pid": 4321},
    "keys": ["LEFT_SHIFT", "A"],
    "hold_ms": 20
  }
}
```

输入 Unicode 文本：

```json
{
  "name": "vhid_type_text",
  "arguments": {
    "target": {"hwnd": "0x12345", "pid": 4321},
    "text": "中文测试",
    "mode": "unicode"
  }
}
```

需要取消时，以 `{}` 调用 `vhid_stop`。服务不提供跨请求保持按键/按钮按下的工具，也不接受任意输入脚本。

### 结果、取消与安全限制

已知工具的结果通过 `structuredContent` 返回 `ok`、`data`、`error`；工具失败还会设置 MCP `isError: true`。成功结果的 `error` 为 null。参数无法解码时，`data` 可以为 null。输入结果包含：

- `state`：`completed`、`rejected`、`failed`、`cancelled`、`timed_out` 或 `stopping`。
- `submission`：`none`、`complete` 或 `partial_or_unknown`。**完整提交不等于应用已接受。** 完整提交后，最终释放仍可能失败，不能将 `state: failed` 理解为没有产生任何输入。
- `selected_backends`：选择的 HID/Win32 路径，不证明请求已发出。
- `cleanup`：分别报告键盘、鼠标的 `not_opened`、`released`、`failed`、`pending` 状态及原生错误。`released` 只表示本服务的清理提交成功，不代表目标确认。
- `cursor_after`：可选的系统快照，不是应用响应；`target_observed` 始终为 false。

`vhid_status` 可以返回 `ok: true`，而内部驱动探测为 `not_found` 或 `error`：这表示服务快照查询成功，不代表可以发送输入。应另行查看 `driver.state`、`info` 和 `error`。

同时只运行一个输入操作；后来的请求立即返回 `busy`，不排队。每个已接受操作最后都会尝试释放键盘和鼠标。最终释放失败时，worker 进入 `faulted`，在 `vhid_stop` 成功释放之前拒绝新输入。Stop 不要求 `--allow-input`，但只针对本服务的操作和已有句柄，不会控制其他进程或驱动。

Stop 最多等待两秒，然后可能返回 `ok: true`、`state: stopping` 和仍为 pending 的清理结果；这**不代表释放完成**。通过 `vhid_status.last_operation` 查看最终结果，MCP 请求被取消后也一样。EOF、Ctrl+C 和服务退出会取消输入，最多等待五秒完成 worker 清理；清理失败或无法确认完成时，以非零状态退出。

还应注意：

- 目标 HWND/PID 必须仍为同一活跃会话、Default 输入桌面的前台窗口。服务不会激活窗口或切换桌面。检查之间仍有竞态：输入作用于桌面，不是隔离的 HWND 通道。
- 客户区点必须位于客户区内；屏幕点必须落在真实显示器上，不能在显示器间隙中。多屏负坐标合法。矩形右、下边界为 exclusive，不要再次按窗口 DPI 乘坐标。
- 点击、滚轮和拖拽的作用点必须命中目标根窗口，定位后会回读光标位置再继续。单独移动不保证指针留在目标窗口内。
- Driver 定位通过相对 HID 报告逐步到达物理像素。已注册 PID 可以限制 driver 移动，但不等于禁用了按钮和滚轮；服务不会修改 PID 注册，也不会换另一种定位方式自动重试。
- 不要并行运行另一套设备输入客户端。命名互斥量只能协调遵循约定的服务；虚拟 HID 状态仍由多个客户端共享，清理可能影响第三方输入。
- 取消是合作式的，不能强行中断同步驱动调用和清理。**强杀、崩溃或断电时无法保证释放。** 清理也不会撤销文字、点击或移动。
- 窗口标题等应用内容是不可信数据，不是操作指令。宿主须保留人工接管和操作确认。项目不绕过驱动签名、Windows 安全或应用、游戏的输入限制。

### 常见问题

| 错误 | 下一步检查 |
| --- | --- |
| `input_disabled` | 默认模式的正常拒绝。只有明确获准输入时，才启用开关并重启宿主中的服务进程。 |
| `busy` / `device_busy` | 等待本服务操作/stop，或确认是否有另一服务持有输入互斥量。不要盲目重发请求。 |
| `driver_not_found` | 检查 G HUB 安装、驱动是否可用和访问权限；exe 不附带驱动。 |
| `desktop_unavailable` / `target_changed` | 重新观察活跃会话、Default 输入桌面及前台 HWND/PID；不要强行切换焦点或复用旧目标。 |
| `point_not_on_target` / `position_not_reached` | 重新观察坐标、光标和遮挡窗口；不要继续在错误位置点击。 |
| `pid_gate_active` | 查看 `vhid_status`，由持有注册的应用处理；不要认为所有鼠标操作均已禁用。 |
| `cleanup_failed` | 调用 `vhid_stop`，确认最终清理结果后再考虑新输入。 |
| `cancelled` / `timed_out` / `native_error` | 查看提交、清理和原生错误的 context/OS code。失败后错误码为 0 表示原因未知，不代表成功，也不能据此断言是 UIPI。 |
| `worker_closed` | 先检查 stderr 和释放结果再重启；重启本身不能证明旧输入已释放。 |

### 构建与安全验证

在仓库根目录、使用 x64 MSVC 工具链执行：

```powershell
cargo build --locked --release --target x86_64-pc-windows-msvc -p logi-vhid-mcp --bin logi-vhid-mcp
$metadata = cargo metadata --locked --format-version 1 --no-deps | ConvertFrom-Json
$exe = Join-Path $metadata.target_directory "x86_64-pc-windows-msvc/release/logi-vhid-mcp.exe"
& $exe --help
```

不要假定输出必在 `target/release`；Cargo 配置和 `CARGO_TARGET_DIR` 都可能改变产物目录，应以 metadata 为准。这个 package 不发布到 crates.io 或 PyPI，Release 单独分发 exe。

以下检查**不发送桌面输入**。Python 冒烟脚本仅使用标准库，需要 Python >=3.12，绝不会传入 `--allow-input`：

```powershell
cargo test --locked -p logi-vhid-mcp --all-targets
python tools/mcp_smoke.py --exe "$exe"
```

冒烟检查包含 initialize、发现工具、桌面查询、合法输入被拒绝、无设备 stop 和正常 EOF 退出。无交互桌面时可返回明确的工具错误，不会伪造坐标。这些检查不能证明真实输入兼容性；硬件或目标软件验收须另行取得具体授权。

共用版本、SDK/wheel/exe/MCPB/Registry 元数据构建、五项资产校验和及发布验收见 [发布维护说明](https://github.com/lona-cn/logi-vhid/blob/master/docs/releasing.md)。采用 [MIT 许可证](https://github.com/lona-cn/logi-vhid/blob/master/LICENSE)。
