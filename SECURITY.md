# Security policy / 安全政策

[English](#english) · [简体中文](#简体中文) · [Privacy](docs/privacy.md)

## English

### Support and scope

Use [GitHub Issues](https://github.com/lona-cn/logi-vhid/issues) for ordinary installation, documentation and compatibility support. Include the release version, Windows/host information and a redacted error summary; do not post private window titles, device paths, input text or credentials. No fixed response time, supported-version window or compatibility guarantee is promised. Prefer the latest published version and describe the exact affected version when reporting a problem. Version 0.1.1 is being prepared; this policy is not a claim that it has been released or tested in a host.

### Reporting a vulnerability

**GitHub Private Vulnerability Reporting is currently disabled for this repository. There is no dedicated private intake channel advertised by this project.** Do not assume that a private reporting button or private email address exists.

If you need to report a potential vulnerability, open an [Issue](https://github.com/lona-cn/logi-vhid/issues/new) containing only a request to coordinate a private security report. Do **not** include exploit details, proof-of-concept code, keys, tokens, user data or sensitive diagnostics in that public issue. Wait for the maintainer to establish an appropriate private channel before sharing details. An issue is public contact coordination, not a confidential reporting mechanism.

A repository administrator may later enable [GitHub Private Vulnerability Reporting](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository). That is a GitHub account/repository setting, not something a code change enables. This documentation update does not change those settings, create a private channel or guarantee a response.

### Security boundaries

`logi-vhid-mcp` defaults to input disabled. `--allow-input` enables capability, not per-operation consent; the host must approve the exact target, action and values and retain human takeover. Input is desktop-level, not isolated to an HWND. Foreground/desktop checks have races. The service does not bypass Windows, driver-signing or application restrictions, install drivers, or automatically elevate.

Do not automatically retry partial or failed input. Cleanup does not undo actions, and forced termination/crashes/power loss have no release guarantee. Shared virtual HID state can affect other clients. See the [MCP safety guide](crates/logi-vhid-mcp/README.md#results-cancellation-and-safety) and [privacy notice](docs/privacy.md#english) before use.

## 简体中文

### 支持与范围

安装、文档与兼容性等普通问题使用 [GitHub Issues](https://github.com/lona-cn/logi-vhid/issues)。请提供发行版本、Windows/宿主信息和脱敏后的错误摘要，不要发布私人窗口标题、设备路径、输入文本或凭据。不承诺固定响应时间、受支持版本期限或兼容性保证；建议使用最新已发布版本，报告时写明准确的受影响版本。0.1.1 正在准备中，本政策不表示它已经发布或通过宿主测试。

### 漏洞报告

**仓库目前未启用 GitHub Private Vulnerability Reporting，项目没有已公布的专用私密接收通道。** 不要假定私报按钮或私密报告邮箱存在。

如需报告疑似漏洞，请先在 [Issue](https://github.com/lona-cn/logi-vhid/issues/new) 中仅请求协调私密安全报告。不要在公开 Issue 中贴漏洞细节、PoC、密钥、Token、用户数据或敏感诊断。等待维护者建立合适的私密渠道后再分享细节；公开 Issue 只是联系协调，不是保密报告机制。

仓库管理员之后可启用 [GitHub Private Vulnerability Reporting](https://docs.github.com/en/code-security/security-advisories/working-with-repository-security-advisories/configuring-private-vulnerability-reporting-for-a-repository)。这属于 GitHub 账号/仓库设置，不能由代码修改完成；本次文档更新没有更改这些设置、建立私密通道或保证响应。

### 安全边界

`logi-vhid-mcp` 默认禁止输入；`--allow-input` 只开启能力，不是逐次同意。宿主须批准具体目标、动作和值，并保留人工接管。输入作用于桌面，不是 HWND 隔离通道；前台/桌面检查仍有竞态。服务不会绕过 Windows、驱动签名或应用限制，不安装驱动、不自动提权。

失败或部分提交后不要自动重试。清理不能撤销操作，强杀、崩溃和断电不保证释放。共享虚拟 HID 状态可能影响其他客户端。使用前请阅读 [MCP 安全说明](crates/logi-vhid-mcp/README.md#结果取消与安全限制) 和 [隐私说明](docs/privacy.md#简体中文)。
