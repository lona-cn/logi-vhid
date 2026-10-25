# Listing materials / 上架素材

[English](#english) · [简体中文](#简体中文) · [Installation guide](README.md)

## English

### Copy-ready text

**Name:** logi-vhid-mcp

**Short description:** Local Windows x64 stdio MCP for keyboard/mouse input through Logitech G HUB, plus desktop and driver queries. Input is disabled by default; each input operation requires host approval.

**Positioning:** A Rust-native, local input companion for MCP hosts that already provide screenshots/observation and per-operation approval. Six tools expose status, physical-pixel desktop information, mouse, keys, text and stop. An opt-in startup flag enables input capability; it does not grant blanket consent. Target/desktop checks, cooperative cancellation and cleanup report outcomes, not application acceptance or guaranteed recovery. Windows x64 and an accessible G HUB virtual bus are required for nonempty input, including Unicode/Win32 modes. No driver, remote HTTP service, OAuth service, screenshot capability or host-specific plugin is included.

**Suggested categories/tags:** Windows, local stdio, keyboard, mouse, desktop automation, virtual HID, Rust. These are descriptive tags, not certifications. Do not market this as stealth, anti-cheat evasion, a security bypass, official Logitech/Anthropic/Cline endorsement, or a replacement for human approval.

### Copy-ready safe starting example

After a release with MCP assets is published, configure a local stdio host:

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

Then call `vhid_desktop_info` with `{}` and inspect `vhid_status`. These do not send input. For a native MCPB import, keep the Boolean `allow_input` setting disabled. Only after explicitly opting in, enable that setting (or use standalone `args: ["--allow-input"]`), obtain approval for a freshly observed target/action/values, and follow the [usage examples](README.md#observe-first-then-act). Do not auto-approve or automatically retry input. Screenshots remain the host's responsibility.

### Links and assets

| Field | Value |
| --- | --- |
| Public repository | https://github.com/lona-cn/logi-vhid |
| MCP source subdirectory | `crates/logi-vhid-mcp` |
| MCP landing page | https://github.com/lona-cn/logi-vhid/tree/master/crates/logi-vhid-mcp |
| Installation README | https://github.com/lona-cn/logi-vhid/blob/master/crates/logi-vhid-mcp/README.md |
| Icon relative to this file / MCPB manifest | `icon.png` |
| Icon source URL (400×400 PNG) | https://raw.githubusercontent.com/lona-cn/logi-vhid/master/crates/logi-vhid-mcp/icon.png |
| Releases | https://github.com/lona-cn/logi-vhid/releases |
| Privacy | https://github.com/lona-cn/logi-vhid/blob/master/docs/privacy.md |
| Ordinary support | https://github.com/lona-cn/logi-vhid/issues |
| Security reporting policy | https://github.com/lona-cn/logi-vhid/blob/master/SECURITY.md |
| License | https://github.com/lona-cn/logi-vhid/blob/master/LICENSE |
| Planned Registry name | `io.github.lona-cn/logi-vhid-mcp` (not registered) |

These source/icon/policy URLs require the corresponding changes to be pushed before submission. There is no demo video, fabricated recording URL or claimed host/hardware acceptance result: **no real input or host installation was performed for this publication preparation**. Version 0.1.1 is being prepared; public v0.1.0 has no MCP assets. No marketplace/Registry submission has been performed.

### Before submitting: first channels

**Shared release gate**

- Publish a release containing the SDK ZIP, wheel, standalone exe, native MCPB and generated `server.json`, with `SHA256SUMS` covering exactly those five assets. Use the [release guide](../../docs/releasing.md); do not advertise MCP downloads from v0.1.0.
- Review the final manifest, same-exe bundle layout, icon, README, privacy/support/security links and public availability. Keep input disabled by default. GitHub Private Vulnerability Reporting is currently disabled; no dedicated private intake is advertised. Public issues are for contact coordination only when a vulnerability is involved, without PoCs, secrets or user data.
- Record actual host version, installation outcome and separately authorized input/cleanup acceptance before making compatibility claims. No checkboxes may stand in for unperformed tests.

**Native MCPB distribution / host import**

- Use `logi-vhid-mcp-<version>-windows-x86_64-msvc.mcpb` and verify its exact `SHA256SUMS` entry. The bundle is Windows-only `binary`; verify the intended host supports native bundles and Boolean settings.
- Confirm `allow_input: false` starts input-disabled queries. Enable it only after explicit opt-in. Test discovery/read-only mode first, without enabling input or auto-approval.
- Follow the host's current local-import policies. **MCPB is a package format, not Claude's official directory submission form.** Claude directory review would be a separate process; do not label this server certified, listed or approved merely because a bundle exists. Claude/other MCPB host installation remains untested.

**Official MCP Registry**

- Use generated `dist/server.json`, not `server.template.json`. Confirm namespace authority/availability, version, exact public release MCPB URL and `fileSha256`. Review [official package rules](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx).
- An authorized maintainer separately authenticates and publishes, using the [manual Registry instructions](../../docs/releasing.md#manual-mcp-registry-and-channel-publication). Default `mcp-publisher publish` must run in the directory containing generated `server.json`. No automatic Registry publication is configured.
- Confirm the actual Registry response/entry before claiming registration; listing is not certification or installation evidence.

**Cline marketplace**

- First install manually from the [MCP README](README.md#cline-shortest-manual-setup) on a real Cline host, with `args: []` and no auto-approval. Record tool discovery and read-only query results. Keep actual input acceptance separate and explicitly authorized.
- Use the [official submission form](https://github.com/cline/mcp-marketplace/issues/new?template=mcp-server-submission.yml), with the public repository URL, a visible MCP-subdirectory landing link and the 400×400 PNG. Explain Windows/G HUB requirements, default-disabled input and the manual exe installation route in Additional Information.
- The current form requires affirming installation from README and public-use stability. **Do not submit or tick those attestations until they are actually satisfied.** Cline installation, stability acceptance and marketplace submission are not completed by this materials file; do not invent a version, date or demo link.

## 简体中文

### 可复制介绍

**名称：** logi-vhid-mcp

**短介绍：** 面向 Windows x64 的本地 stdio MCP，通过 Logitech G HUB 提供键鼠输入与桌面/驱动查询。默认禁止输入，每次输入操作仍需宿主批准。

**定位：** 为已有截图/观察和逐次批准机制的 MCP 宿主补充本地输入能力，使用 Rust 原生实现。六个工具涵盖状态、物理像素桌面信息、鼠标、按键、文本和 stop。显式启动开关只开启能力，不构成无限授权；目标/桌面检查、合作式取消和清理报告操作状态，不代表应用接收确认或保证恢复。非空输入（含 Unicode/Win32 模式）需要 Windows x64 与可访问的 G HUB 虚拟总线。不附带驱动、远程 HTTP/OAuth 服务、截图或各家专用插件。

**建议分类/标签：** Windows、本地 stdio、键盘、鼠标、桌面自动化、虚拟 HID、Rust。它们只是描述，不是认证。不得宣传隐身、反作弊规避、安全绕过、Logitech/Anthropic/Cline 官方背书或替代人工批准。

### 可复制的安全起步示例

含 MCP 资产的版本发布后，使用上面的 `mcpServers` JSON，exe 指向实际绝对路径，`args` 保持 `[]`。先用 `{}` 调用 `vhid_desktop_info`，再查看 `vhid_status`，不发送输入。原生 MCPB 导入后保持布尔设置 `allow_input` 关闭。明确选用输入时才开启该设置（独立 exe 为 `args: ["--allow-input"]`），重新观察目标，并就具体目标/动作/值逐次取得批准，再按 [使用示例](README.md#先观察再操作) 执行。不要自动批准或自动重试；截图继续由宿主提供。

### 链接与素材

公共仓库为 https://github.com/lona-cn/logi-vhid ，默认分支 `master`，不是本机工作目录名。MCP 子目录为 `crates/logi-vhid-mcp`；上方链接表可直接用于表单。图标相对路径为 **`icon.png`**，源为 400×400 PNG。源码、图标及政策链接需对应改动先推送后才对外可见。

隐私见 [docs/privacy.md](../../docs/privacy.md#简体中文)，普通支持见 [GitHub Issues](https://github.com/lona-cn/logi-vhid/issues)，漏洞报告见 [SECURITY.md](../../SECURITY.md#简体中文)。仓库目前未启用 GitHub 私密漏洞报告，没有已公布的专用私密接收渠道；涉及漏洞时公开 Issue 只能不含细节地联系协调，不公开 PoC、密钥或用户数据。

**本次上架准备没有执行真实输入或宿主安装，没有演示录屏、伪造视频链接或硬件验收结果。** 当前准备版本为 0.1.1，公开 v0.1.0 没有 MCP 资产；尚未提交市场或登记 Registry。

### 首批渠道提交前清单

**共同发布条件**

- 按 [发布说明](../../docs/releasing.md#简体中文) 发布 SDK ZIP、wheel、独立 exe、原生 MCPB、生成的 `server.json` 五项资产及恰好覆盖它们的 `SHA256SUMS`，不借用 v0.1.0 宣传 MCP 下载。
- 核对最终 manifest、同 exe 包布局、图标、README、隐私/支持/安全链接公开可访问；默认禁止输入。根据实际完成的宿主安装、单独授权的输入/清理验收记录声明兼容性，不凭复选框替代测试。

**原生 MCPB 分发 / 宿主导入**

- 使用 `logi-vhid-mcp-<版本>-windows-x86_64-msvc.mcpb` 并核对哈希；确认目标宿主支持 Windows 原生 `binary` 包及布尔设置。
- 先以 `allow_input: false` 测试发现与查询；明确选用输入时才开启，不默认启用输入或自动批准。
- 遵循宿主现行本地导入政策。**MCPB 是格式，不是 Claude 官方目录投稿形式**；目录审核另行处理，不因有包就声称认证/收录。Claude 或其他 MCPB 宿主安装仍未实测。

**官方 MCP Registry**

- 只用生成的 `dist/server.json`，不用源模板；确认 namespace 权限/可用性、版本、公开 Release 精确 MCPB URL 与 `fileSha256`，复核 [官方包规则](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx)。
- 按 [人工发布流程](../../docs/releasing.md#人工发布-mcp-registry-与各渠道) 由获准维护者单独登录/发布，默认 `mcp-publisher publish` 必须在含生成 `server.json` 的目录执行。本仓库没有自动 Registry 发布任务。
- 查看真实返回和条目后才能宣布登记，收录不是认证或安装证据。

**Cline 市场**

- 先在真实 Cline 宿主按 [最短手动接入](README.md#cline-最短手动接入) 从 README 安装，保持 `args: []`、不自动批准，记录发现六工具与只读查询结果；输入验收另行明确授权。
- 使用 [官方提交表单](https://github.com/cline/mcp-marketplace/issues/new?template=mcp-server-submission.yml)，提供公共仓库 URL、可见的 MCP 子目录入口和 400×400 PNG，在补充说明中写明 Windows/G HUB、默认禁止输入和手动 exe 安装方式。
- 当前表单要求确认已凭 README 安装并适合公开使用，**未完成前不要提交或勾选这些声明**。本素材不代表已完成 Cline 安装/稳定性验收/市场提交，不编造版本、日期或演示链接。
