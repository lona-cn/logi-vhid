# Release maintenance / 发布维护

[English](#english) · [简体中文](#简体中文) · [Project README](https://github.com/lona-cn/logi-vhid/blob/master/README.md)

## English

This guide is for maintainers working from a source checkout. For application integration, see the project README or the [native SDK guide](https://github.com/lona-cn/logi-vhid/blob/master/sdk/README.md). None of the local commands below upload packages; the tag push and draft publication described later do trigger remote workflows.

### Version and prerequisites

- `Cargo.toml`'s `[workspace.package].version` is the version source. A release tag must exactly equal `v` followed by that version.
- The root manifest is a virtual workspace with `crates/logi-vhid` and `crates/logi-vhid-mcp`, sharing one lockfile and output directory. Select the package by name when reading Cargo metadata; the first package is not a version-selection contract. The C header remains at root `include/logi_vhid.h`.
- Supported versions are `MAJOR.MINOR.PATCH`, optionally followed by `-alpha.N`, `-beta.N`, or `-rc.N`. Numeric components have no leading zeros; build metadata is rejected. For example, `0.2.0-rc.1` becomes Python version `0.2.0rc1`.
- Native builds require Windows x64, Rust >=1.96 with the `x86_64-pc-windows-msvc` target, and the Visual Studio C/C++ x64 build tools and Windows SDK. Full verification also needs CMake >=3.20 and a Visual Studio generator supporting `-A x64`.
- Release tooling requires Python >=3.12 and `tools/requirements-release.txt`. Install it in a virtual environment, not the system Python.
- The automatic checks do not require G HUB and do not send keyboard/mouse input. Real input acceptance needs a separate suitable Windows/G HUB environment.

The current preparation targets **0.1.1**. Public v0.1.0 has no MCP assets; publish a new release before advertising downloadable MCP packages. This documentation is a procedure, **not evidence of a completed build, host installation or real input test**. No host installation, marketplace submission, Registry registration or real input acceptance has been performed as part of this publication preparation.

### Build and verify locally

Run in PowerShell from the repository root:

```powershell
python -m venv target/release-tools
.\target\release-tools\Scripts\python.exe -m pip install -r tools/requirements-release.txt
$version = ((cargo metadata --format-version 1 --no-deps --locked | ConvertFrom-Json).packages | Where-Object { $_.name -eq "logi-vhid" }).version
$tag = "v$version"
.\target\release-tools\Scripts\python.exe tools/release.py build --tag $tag --dist dist
.\target\release-tools\Scripts\python.exe tools/release.py verify --tag $tag --dist dist
```

`build` requires `dist` to be absent or empty. If a previous build is present, choose a different `--dist` directory or move those files aside; do not delete assets you still need. `verify` must use the same tag and directory.

One `cargo build --locked --release --target x86_64-pc-windows-msvc -p logi-vhid --lib` supplies the byte-identical DLL for the SDK and wheel. A separate `cargo build --locked --release --target x86_64-pc-windows-msvc -p logi-vhid-mcp --bin logi-vhid-mcp` builds the server. The script obtains the effective target directory through Cargo metadata, including Cargo configuration and `CARGO_TARGET_DIR`, then uses the target-triple/release subdirectory for both builds and renames the MCP executable in staging. The same executable bytes are used inside the MCPB. It passes the existing DLL to the wheel backend using `LOGI_VHID_NATIVE_DIR`; do not confuse this build-time directory with the Python runtime override `LOGI_VHID_DLL`, which names a DLL file.

The output contains:

| File | Purpose |
| --- | --- |
| `logi-vhid-<Cargo version>-windows-x86_64-msvc.zip` | DLL, MSVC DLL import library, C header, SDK README, LICENSE, and safe C/C++ examples |
| `logi_vhid-<Python version>-py3-none-win_amd64.whl` | Python package with the byte-identical DLL |
| `logi-vhid-mcp-<Cargo version>-windows-x86_64-msvc.exe` | Independent local stdio MCP server, not bundled inside the SDK ZIP or wheel |
| `logi-vhid-mcp-<Cargo version>-windows-x86_64-msvc.mcpb` | Native Windows binary MCPB with the same MCP executable |
| `server.json` | Publishable MCP Registry metadata for the exact versioned MCPB URL and SHA-256 |
| `SHA256SUMS` | SHA-256 hashes of exactly five basenames: SDK ZIP, wheel, MCP exe, MCPB and `server.json` |
| `RELEASE_NOTES.md` | Draft release body, not a separate published asset |

`verify` first checks names, versions, archive paths, metadata, hashes, archive CRCs and DLL equality. The SDK/wheel contents and consumer names are unchanged. It extracts the SDK into an owned temporary directory, compiles/runs its C and C++ consumers, and installs the wheel using `--no-index --no-deps` into a fresh temporary environment outside the checkout. It removes `PYTHONPATH`, `PYTHONHOME`, `LOGI_VHID_DLL` and `LOGI_VHID_NATIVE_DIR` from subprocess environments. The C/Python smoke checks use only null-handle/error-query calls, not `open()` or actual input. Temporary verification files are removed afterward.

MCPB source metadata is `crates/logi-vhid-mcp/manifest.json`; `icon.png` and the MCP-specific README come from that same directory. The build stamps the shared version and creates exactly `manifest.json`, `server/logi-vhid-mcp.exe`, `icon.png`, `LICENSE`, and `README.md` in the bundle. It is `server.type: binary`, platform `win32` only. `user_config.allow_input` is an optional Boolean, default `false`; `mcp_config.args` is `["--allow-input=${user_config.allow_input}"]`. The executable accepts exactly lowercase `true`/`false` for this setting, without a launcher or environment-variable permission switch. Keep the default disabled; enabling capability never replaces host approval of each operation.

`crates/logi-vhid-mcp/server.template.json` contains static Registry metadata only; it is **not publishable as-is**. Build generates `dist/server.json` with shared version, MCPB package version, the precise `https://github.com/lona-cn/logi-vhid/releases/download/v<version>/logi-vhid-mcp-<version>-windows-x86_64-msvc.mcpb` identifier and `fileSha256` from the final bundle. Planned name: `io.github.lona-cn/logi-vhid-mcp`; namespace availability/ownership and registration are not established by generating a file. Do not hand-edit the generated manifest or bundle after hashing; rebuild instead.

It also runs the exact staged MCP executable using the standalone Python standard-library module `tools/mcp_smoke.py`, with the same cleaned environment and no executable arguments. To repeat just this check:

```powershell
.\target\release-tools\Scripts\python.exe tools/mcp_smoke.py --exe "dist/logi-vhid-mcp-$version-windows-x86_64-msvc.exe"
```

The smoke sequence is initialize → initialized → tools/list → desktop info → a valid key request rejected as `input_disabled` with `submission: none` → stop without devices → stdin EOF. It verifies JSON-RPC stdout and normal exit. In headless CI, desktop info may report an explicit `desktop_unavailable`/`native_error`, not fabricated coordinates. The script offers no `--allow-input` option, needs no G HUB or MCP client installation, and sends no desktop input. Protocol acceptance is separate from real hardware/input acceptance.

The native DLL requires the Microsoft Visual C++ v14 x64 runtime; neither the SDK nor wheel installs it, and the MCP executable does not install system components. C/C++ consumers may need additional runtime DLLs for their own build settings. Verification on a configured build machine is not proof of prerequisites on a clean Windows installation.

### Inspect assets without running native code

Use the interpreter where the release dependencies were installed. On Windows,
reuse the environment created above:

```powershell
.\target\release-tools\Scripts\python.exe tools/release.py verify-assets --tag v0.1.1 --dist dist
```

On Linux, create a local environment with Python >=3.12 and install the same
dependencies before inspecting downloaded assets:

```sh
python3 -m venv target/release-tools
target/release-tools/bin/python -m pip install -r tools/requirements-release.txt
target/release-tools/bin/python tools/release.py verify-assets --tag v0.1.1 --dist dist
```

Replace `v0.1.1` with `v` plus the exact `[workspace.package].version` in the checkout's root `Cargo.toml`; verification is not independent of the matching source version. This performs only path, structure and hash checks: no extraction to disk, native loading, compilation, executable launch, or subprocesses, including on Linux. It requires exactly the correctly named, non-symlink five assets and five checksum entries; missing/additional artifacts, missing entries or hash mismatches are rejected. It inspects MCPB manifest/platform/configuration and exact member layout, requires the embedded executable to match the standalone exe, and checks generated Registry version, exact release URL and `fileSha256` against the bundle. It does not require a separate `RELEASE_NOTES.md`. Checksums verify consistency with the manifest, not the trustworthiness of an unknown publisher.

### Publish through GitHub and PyPI

1. Review and commit the release contents, run the local checks, and confirm that the version and tag match.
2. Create and push that exact tag. For example, for package version `0.1.1`:

   ```console
   git tag v0.1.1
   git push origin v0.1.1
   ```

3. The tag workflow reuses CI, then builds and verifies the final packages once with Rust 1.96.0. `skip_release_build` skips CI's separate release builds/CLI and MCP smoke runs; the release job performs final artifact builds and verification instead. CI still runs locked workspace tests/doc tests, skipping ignored hardware tests, and a core crates.io dry-run. The workflow creates a **draft** GitHub Release with SDK ZIP, wheel, standalone MCP exe, MCPB, `server.json`, five-entry `SHA256SUMS` and draft notes as the release body. It does not overwrite an existing release. Supported prerelease tags produce prerelease drafts.
4. Before publishing the draft, obtain explicit approval for the actual desktop operations and perform separate manual hardware acceptance. Use a new unsaved test window, not an existing user document; record Windows/G HUB versions, target HWND/PID, screenshots or other observations, input results and key/button cleanup. With an explicitly authorized `--allow-input` MCP session, check layout/Unicode text, client-coordinate click, relative movement, key combination, scroll, drag, cancellation/stop and stale-target rejection. The default CI suite skips `crates/logi-vhid/tests/hardware.rs`; neither hosted CI nor SDK/wheel/MCP smoke checks establish G HUB compatibility or real input delivery. Follow the [hardware-test warning](https://github.com/lona-cn/logi-vhid/blob/master/README.md#testing) before running it. Handle close/drop alone does not release inputs; cooperative cancellation cannot interrupt synchronous driver I/O, and forced termination provides no cleanup guarantee.
5. Publishing the GitHub Release triggers a Linux job that downloads and verifies the published assets without rebuilding. A separate `pypi` job receives only that verified wheel, does not check out source or build packages, and publishes using OIDC. `id-token: write` is limited to that job; no long-lived PyPI token is used.

Crates.io publishing remains a separate maintainer action. CI runs `cargo publish -p logi-vhid --dry-run --locked --registry crates-io`, not an upload.

### Manual MCP Registry and channel publication

MCPB distribution, Registry registration, Cline marketplace submission and Claude directory review are separate actions. No workflow here logs into or publishes to the MCP Registry or a host marketplace. Preparing an MCPB **is not** a Claude official directory submission.

1. Inspect the five final release assets using `verify-assets`; use the matching checkout and tag. Complete the separate, approved host/input acceptance relevant to any compatibility claims. Check [privacy](privacy.md), [security reporting](../SECURITY.md) and [listing materials](../crates/logi-vhid-mcp/LISTING.md). Private Vulnerability Reporting is currently disabled; do not claim a private intake exists.
2. Publish the matching GitHub Release first. Confirm the generated MCPB URL is publicly downloadable and that its exact bytes match both `fileSha256` in `server.json` and `SHA256SUMS`. A draft/inaccessible URL is not an installable public package. Do not reuse v0.1.0 or invent a hash/URL.
3. An authorized maintainer must separately obtain the official `mcp-publisher` CLI and confirm authority for `io.github.lona-cn/logi-vhid-mcp`. Review the [official CLI reference](https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/cli/commands.md) and [MCPB package rules](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx) before authentication/publication. The CLI is not a release-tool dependency and is not installed by this repository.
4. In PowerShell **inside the `dist` directory that contains the generated `server.json`**, validate, authenticate with the intended GitHub account, then publish only after explicit authorization:

   ```powershell
   Set-Location dist
   mcp-publisher validate server.json
   mcp-publisher login github
   mcp-publisher publish
   ```

   Both `validate` and `publish` default to `./server.json`; `publish server.json` is also accepted. Do not run default `publish` from the repository root or publish the source template. `--registry` is a **login-only** flag; publishing uses the registry stored in the login token. These commands authenticate and publish externally; they are manual instructions, not commands run during this preparation.
5. Confirm the Registry entry/version and download metadata before claiming listing. Registry publication is metadata distribution, not certification or proof of host compatibility. For Cline, follow its current submission form only after testing installation from the README; do not tick unperformed installation/stability attestations. Native MCPB installation in Claude Desktop or another host needs its own testing and review, not a fabricated certification badge.


### One-time PyPI configuration

Before the first public release, configure a [pending Trusted Publisher](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/) in the publishing PyPI account:

| Field | Value |
| --- | --- |
| PyPI project | `logi-vhid` |
| GitHub owner | `lona-cn` |
| Repository | `logi-vhid` |
| Workflow filename | `release.yml` |
| Environment | `pypi` |

Create the `pypi` environment in the GitHub repository settings; requiring a reviewer is recommended. The scripts do not configure the PyPI account or environment protection rules. A pending publisher does not reserve the project name: confirm that the account can publish it. Uploads cannot succeed until the matching publisher is configured.

## 简体中文

本说明面向从源码发布版本的维护者。应用接入请看项目 README 或 [C/C++ SDK 说明](https://github.com/lona-cn/logi-vhid/blob/master/sdk/README.md)。下列本地构建命令不会上传文件；后文的标签推送和发布草稿操作会触发远程工作流。

### 版本与环境要求

- 版本统一读取根 `Cargo.toml` 的 `[workspace.package].version`，发布标签必须是 `v` 加该版本，完全一致。
- 根 manifest 是虚拟 workspace，包含 `crates/logi-vhid` 和 `crates/logi-vhid-mcp`，共用一个锁文件和产物目录。读取 Cargo metadata 时按 package 名选择，不依赖 packages 数组第一项。C 头文件仍在根目录的 `include/logi_vhid.h`。
- 支持 `MAJOR.MINOR.PATCH`，以及带 `-alpha.N`、`-beta.N`、`-rc.N` 的预发布版本。数字不能有前导零，不接受 build metadata。例如 `0.2.0-rc.1` 对应 Python 版本 `0.2.0rc1`。
- 原生构建需要 Windows x64、Rust >=1.96、`x86_64-pc-windows-msvc` target、Visual Studio C/C++ x64 build tools 和 Windows SDK。完整验收还需要 CMake >=3.20，以及支持 `-A x64` 的 Visual Studio generator。
- 发布工具需要 Python >=3.12 和 `tools/requirements-release.txt`。请在虚拟环境中安装，不要修改系统 Python。
- 自动检查不需要 G HUB，也不会发送键鼠输入。真实输入验收需要另行准备合适的 Windows/G HUB 环境。

本次准备版本为 **0.1.1**。公开 v0.1.0 没有 MCP 资产；须先发布新版本，再宣传可下载的新 MCP 包。本文是操作流程，**不是构建完成、宿主安装或真实输入测试证据**；本次上架准备没有执行宿主安装、市场提交、Registry 登记或真实输入验收。

### 本地构建与验收

在仓库根目录的 PowerShell 中运行：

```powershell
python -m venv target/release-tools
.\target\release-tools\Scripts\python.exe -m pip install -r tools/requirements-release.txt
$version = ((cargo metadata --format-version 1 --no-deps --locked | ConvertFrom-Json).packages | Where-Object { $_.name -eq "logi-vhid" }).version
$tag = "v$version"
.\target\release-tools\Scripts\python.exe tools/release.py build --tag $tag --dist dist
.\target\release-tools\Scripts\python.exe tools/release.py verify --tag $tag --dist dist
```

`build` 要求 `dist` 不存在或为空。如果已有构建结果，请换一个 `--dist` 目录或先移走旧文件，不要删除仍需保留的产物。`verify` 必须使用同一个标签和目录。

脚本只运行一次 `cargo build --locked --release --target x86_64-pc-windows-msvc -p logi-vhid --lib`，供 SDK 和 wheel 复用同一份 DLL；另用 `cargo build --locked --release --target x86_64-pc-windows-msvc -p logi-vhid-mcp --bin logi-vhid-mcp` 构建服务。实际输出目录通过 Cargo metadata 获取，支持 Cargo 配置和 `CARGO_TARGET_DIR`；两个构建均从 target triple 下的 release 子目录取产物，再将 MCP exe 复制、改为版本化名称，MCPB 内嵌同一份 exe 字节。现成 DLL 通过 `LOGI_VHID_NATIVE_DIR` 交给 wheel 构建后端；这是构建时的目录，不是 Python 运行时使用的 DLL 文件路径 `LOGI_VHID_DLL`。

输出文件：

| 文件 | 用途 |
| --- | --- |
| `logi-vhid-<Cargo 版本>-windows-x86_64-msvc.zip` | DLL、MSVC DLL 导入库、C 头文件、SDK README、LICENSE 和安全的 C/C++ 示例 |
| `logi_vhid-<Python 版本>-py3-none-win_amd64.whl` | Python 包，内含与 SDK 完全相同的 DLL |
| `logi-vhid-mcp-<Cargo 版本>-windows-x86_64-msvc.exe` | 独立的本地 stdio MCP 服务，不放进 SDK ZIP 或 wheel |
| `logi-vhid-mcp-<Cargo 版本>-windows-x86_64-msvc.mcpb` | 原生 Windows binary MCPB，内含同一份 MCP exe |
| `server.json` | 可发布的 MCP Registry 元数据，指向精确版本的 MCPB URL 与 SHA-256 |
| `SHA256SUMS` | 恰好列出 SDK ZIP、wheel、MCP exe、MCPB、`server.json` 五个文件名及其 SHA-256 |
| `RELEASE_NOTES.md` | 草稿 Release 正文，不单独作为发布附件 |

`verify` 先检查文件名、版本、压缩包路径、元数据、哈希、CRC 和 DLL 一致性。SDK/wheel 内部内容和调用方名称不变；它会在自建临时目录中解包 SDK、编译并运行 C/C++ 示例，在源码目录以外的新临时环境中用 `--no-index --no-deps` 安装 wheel。子进程环境移除 `PYTHONPATH`、`PYTHONHOME`、`LOGI_VHID_DLL`、`LOGI_VHID_NATIVE_DIR`。C/Python 冒烟检查只调用空句柄和错误查询，不调用 `open()`、不发送实际输入。验证结束后清理临时文件。

MCPB 元数据源为 `crates/logi-vhid-mcp/manifest.json`，`icon.png` 和 MCP 专用 README 也来自该目录。构建时填入共用版本，包内固定为 `manifest.json`、`server/logi-vhid-mcp.exe`、`icon.png`、`LICENSE`、`README.md`。它使用 `server.type: binary`，平台仅 `win32`。`user_config.allow_input` 为可选布尔设置，默认 `false`，`mcp_config.args` 为 `["--allow-input=${user_config.allow_input}"]`。exe 对该设置只接受小写 `true`/`false`，不使用启动器或环境变量授权开关。默认保持关闭，启用能力也不能代替宿主逐次批准。

`crates/logi-vhid-mcp/server.template.json` 仅保存 Registry 静态元数据，**不能直接发布**；构建生成 `dist/server.json`，填入共用版本、MCPB 包版本、精确 `https://github.com/lona-cn/logi-vhid/releases/download/v<版本>/logi-vhid-mcp-<版本>-windows-x86_64-msvc.mcpb` identifier 及最终包的 `fileSha256`。拟用名称为 `io.github.lona-cn/logi-vhid-mcp`；生成文件不代表已确认 namespace 可用、归属或登记。哈希后不要手改产物，应重新构建。

它还会用独立的 Python 标准库模块 `tools/mcp_smoke.py`，在相同的净化环境中无参数启动发行目录里那一份 MCP exe。单独复查可运行：

```powershell
.\target\release-tools\Scripts\python.exe tools/mcp_smoke.py --exe "dist/logi-vhid-mcp-$version-windows-x86_64-msvc.exe"
```

固定流程为 initialize → initialized → tools/list → 桌面查询 → 合法按键请求被拒绝为 `input_disabled` 且 `submission: none` → 无设备 stop → stdin EOF，同时检查 stdout 为 JSON-RPC 且正常退出。无交互桌面的 CI 可返回明确的 `desktop_unavailable`/`native_error`，不能伪造坐标。脚本不提供 `--allow-input` 选项，不需要 G HUB、无需安装 MCP 客户端，也不发送桌面输入。协议验收和真实硬件输入验收是两回事。

原生 DLL 需要 Microsoft Visual C++ v14 x64 运行库，SDK 和 wheel 都不会安装它，MCP exe 也不会安装系统组件。C/C++ 应用自身的编译设置还可能引入其他运行库。已配置的构建机通过验收，不等于在全新 Windows 系统上已验证全部前置条件。

### 不执行原生代码的文件检查

请使用已安装发布依赖的解释器。Windows 下继续使用上文创建的虚拟环境：

```powershell
.\target\release-tools\Scripts\python.exe tools/release.py verify-assets --tag v0.1.1 --dist dist
```

Linux 下先使用 Python >=3.12 创建本地虚拟环境、安装相同依赖，再检查下载的发行包：

```sh
python3 -m venv target/release-tools
target/release-tools/bin/python -m pip install -r tools/requirements-release.txt
target/release-tools/bin/python tools/release.py verify-assets --tag v0.1.1 --dist dist
```

将 `v0.1.1` 换成当前源码根 `Cargo.toml` 中 `[workspace.package].version` 的 `v` 加精确版本；该检查仍需要匹配版本的源码。它只检查路径、结构和哈希，不解包到磁盘、不加载 DLL、不编译、不运行 exe 或任何子进程，在 Linux 上也是如此。必须恰好有名称正确、非符号链接的五项资产及五条校验和；缺少或多出资产、缺少条目或哈希错误都会拒绝。它还检查 MCPB manifest/平台/配置、精确成员布局、内嵌 exe 与独立 exe 相同，以及 Registry 版本、精确 Release URL 和 `fileSha256` 与 MCPB 一致。它不要求单独的 `RELEASE_NOTES.md`。校验和只能证明文件与清单一致，不能证明未知发布者可信。

### 通过 GitHub 与 PyPI 发布

1. 审查并提交发布内容，完成本地检查，确认包版本与标签完全一致。
2. 创建并推送对应标签。例如包版本为 `0.1.1` 时：

   ```console
   git tag v0.1.1
   git push origin v0.1.1
   ```

3. 标签工作流先复用 CI，再用 Rust 1.96.0 构建并验收最终发行包。`skip_release_build` 跳过 CI 独立的 release 构建、CLI 与 MCP 冒烟检查，改由 release 任务构建、验收最终资产；CI 仍执行 locked workspace 单测和文档测试、跳过 ignored 硬件测试，并对 core 做 crates.io dry-run。随后创建附带 SDK ZIP、wheel、独立 MCP exe、MCPB、`server.json`、五条 `SHA256SUMS` 及作为正文的发布说明的 **GitHub 草稿 Release**，不会覆盖已有 Release。受支持的预发布标签会创建 prerelease 草稿。
4. 发布草稿前，先就具体桌面操作取得明确授权，再单独完成人工硬件验收。使用新建、未保存的测试窗口，不操作用户已有文档；记录 Windows/G HUB 版本、目标 HWND/PID、截图等观察结果、真实输入和释放清理情况。明确获准启用 `--allow-input` 后，检查布局/Unicode 文本、客户区点击、相对移动、组合键、滚轮、拖拽、取消/stop 和旧目标拒绝。默认 CI 跳过 `crates/logi-vhid/tests/hardware.rs`；托管 CI 和 SDK/wheel/MCP 冒烟检查都不能证明 G HUB 兼容性或真实输入成功。运行硬件测试前先阅读 [测试警告](https://github.com/lona-cn/logi-vhid/blob/master/README.md#测试)。关闭句柄或丢弃对象不等于释放输入；合作式取消不能中断同步驱动 I/O，强杀也不能保证清理。
5. GitHub Release 发布后，Linux 任务下载并校验已发布文件，不重新构建；独立的 `pypi` 任务只接收已验证的同一份 wheel，不检出源码或构建包，通过 OIDC 上传。只有该任务具有 `id-token: write`，不使用长期 PyPI Token。

crates.io 仍由维护者单独发布。CI 只执行 `cargo publish -p logi-vhid --dry-run --locked --registry crates-io`，不会上传。

### 人工发布 MCP Registry 与各渠道

MCPB 分发、Registry 登记、Cline 市场提交与 Claude 目录审核是不同动作。本仓库没有自动登录/发布 Registry 或宿主市场的工作流；制作 MCPB **不等于** 向 Claude 官方目录投稿。

1. 用匹配源码及标签的 `verify-assets` 检查五项最终资产。就要声明的兼容性另行完成明确授权的宿主/输入验收，并检查 [隐私](privacy.md)、[安全报告](../SECURITY.md) 和 [上架素材](../crates/logi-vhid-mcp/LISTING.md)。仓库目前未启用 Private Vulnerability Reporting，不要声称已有私密接收渠道。
2. 先正式发布匹配的 GitHub Release，确认生成的 MCPB URL 可公开下载，且字节与 `server.json` 的 `fileSha256`、`SHA256SUMS` 相同。草稿或不可访问 URL 不是可公开安装的包，不得借用 v0.1.0 或编造哈希/URL。
3. 获准维护者需自行取得官方 `mcp-publisher` CLI，确认有权发布 `io.github.lona-cn/logi-vhid-mcp`。认证/发布前复核 [官方 CLI 文档](https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/cli/commands.md) 与 [MCPB 包规则](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx)。CLI 不属于发布工具依赖，本仓库不安装它。
4. PowerShell 中进入**包含生成 `server.json` 的 `dist` 目录**，验证、用目标 GitHub 账号登录，明确获准后才发布：

   ```powershell
   Set-Location dist
   mcp-publisher validate server.json
   mcp-publisher login github
   mcp-publisher publish
   ```

   `validate` 与 `publish` 均默认读取 `./server.json`，也可显式使用 `publish server.json`。不要在仓库根目录运行默认 `publish`，也不要发布源模板。`--registry` **仅用于 login**；publish 使用登录 Token 中的 Registry 地址。这些是需要人工执行的外部认证/发布说明，本次准备没有执行。
5. 核对 Registry 条目/版本及下载元数据后才能宣布收录。Registry 发布不是认证或宿主兼容性证据。Cline 需先按 README 实测安装，再走当前提交表单，不勾选未完成的安装/稳定性声明。Claude Desktop 或其他宿主的原生 MCPB 安装也需单独实测与审核，不得捏造认证标志。


### 首次发布前的 PyPI 配置

在有发布权限的 PyPI 账号中配置 [pending Trusted Publisher](https://docs.pypi.org/trusted-publishers/creating-a-project-through-oidc/)：

| 字段 | 值 |
| --- | --- |
| PyPI 项目 | `logi-vhid` |
| GitHub owner | `lona-cn` |
| Repository | `logi-vhid` |
| Workflow filename | `release.yml` |
| Environment | `pypi` |

同时在 GitHub 仓库设置中创建 `pypi` environment，建议启用 required reviewers。脚本不会代替你修改 PyPI 账号或环境保护规则。pending publisher 不会预留项目名，需要确认账号有权发布该项目；配置匹配前，上传不会成功。
