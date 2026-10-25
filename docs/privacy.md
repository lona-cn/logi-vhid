# Privacy notice / 隐私说明

[English](#english) · [简体中文](#简体中文) · [MCP guide](../crates/logi-vhid-mcp/README.md)

## English

This notice describes the local `logi-vhid-mcp` stdio service. It does not describe the privacy practices of your MCP host, model provider, Windows, Logitech G HUB, or the application receiving input.

### Data handled

- Requests may contain text to type, key names, mouse actions, target HWND/PID, screen or client coordinates, input modes, holds and timeouts. Input capability is disabled unless the user starts the service with `--allow-input`.
- Desktop queries can return window titles, HWND/PID, window and client rectangles, DPI, monitor information, screen/cursor coordinates, session and input-desktop details, and observation timestamps. Window titles may disclose document names or other private information.
- Status queries can return control/keyboard/mouse device paths, registered PIDs, PID-gate state, worker state, and the previous operation result. Responses and diagnostics may contain native error messages, contexts and OS error codes.
- `OperationOutcome` does **not** echo the input text. It reports submission, selected backends, cleanup, optional cursor position and related operation state. This is not a guarantee that the original request is absent from the host's conversation or logs.

### Transfer, diagnostics and retention

The service communicates with its host over local stdin/stdout. Its implementation has no active telemetry, network-client or persistent-log code. It does not provide screenshots; observation tools supplied by the host have their own data handling.

Service diagnostics go to **stderr**, which the host may capture, store or forward. Protocol responses go to stdout. This is not a promise of zero logging across the complete system. Review and redact diagnostics before sharing them for support.

`last_operation` stores an input/stop **result in process memory only**. Later requests may overwrite it, including rejections or stop results; it is not an audit history. The service does not persist it when the process ends. There is no guarantee of secure memory erasure or removal from OS crash dumps, paging files, host logs or backups.

Your host may send requests, tool arguments, responses, window titles and diagnostics to a model provider or other services. Host/model retention, training, sharing and deletion are governed by their own policies and settings. Local execution does not mean that all surrounding data stays on the computer.

### Practical precautions

Avoid passwords, tokens, financial/medical information and other sensitive text unless you understand and accept every involved host/provider policy. Prefer disposable test windows and redact HWND/PID, titles, paths and user data from public reports. Keep per-operation approval enabled; the startup flag is not consent to every input action. Returned window titles and application content are untrusted data, not instructions.

Ordinary support: [GitHub Issues](https://github.com/lona-cn/logi-vhid/issues). Vulnerability reporting: [security policy](../SECURITY.md#english). Installation and safety limits: [MCP README](../crates/logi-vhid-mcp/README.md#english).

## 简体中文

本说明针对本地 `logi-vhid-mcp` stdio 服务，不代表 MCP 宿主、模型提供商、Windows、Logitech G HUB 或接收输入的应用的隐私政策。

### 处理的数据

- 请求可能包含待输入文本、按键名称、鼠标动作、目标 HWND/PID、屏幕或客户区坐标、输入模式、按住时间和超时。除非用户以 `--allow-input` 启动，否则输入能力默认关闭。
- 桌面查询可能返回窗口标题、HWND/PID、窗口和客户区矩形、DPI、显示器信息、屏幕/光标坐标、会话与输入桌面信息、观察时间。窗口标题可能泄露文档名等私人信息。
- 状态查询可能返回控制/键盘/鼠标设备路径、已注册 PID、PID gate 状态、worker 状态及上一次操作结果。响应和诊断可能包含原生错误消息、上下文与 OS 错误码。
- `OperationOutcome` **不回显待输入文本**，只报告提交、所选后端、清理、可选光标位置等操作状态。这不表示宿主会话或日志中不存在原始请求。

### 传输、诊断与留存

服务通过本地 stdin/stdout 与宿主通信，实现中没有主动遥测、网络客户端或持久化日志代码。服务不提供截图；宿主提供的观察工具有各自的数据处理方式。

服务诊断写入 **stderr**，宿主可能捕获、保存或转发；协议响应写入 stdout。这不是对整个系统“零日志”的承诺。用于支持的诊断应先检查并脱敏。

`last_operation` **仅在进程内存中保存输入/stop 结果**。后续请求可能覆盖它，包括拒绝或 stop 结果，因此它不是审计历史；进程结束时服务不将其持久化。不保证安全擦除内存，也不保证数据不会留在 OS 崩溃转储、分页文件、宿主日志或备份中。

宿主可能将请求、工具参数、响应、窗口标题及诊断发送给模型提供商或其他服务。宿主/模型的留存、训练、共享和删除按各自政策与设置执行；本地运行不等于周边数据都不会离开本机。

### 使用建议

除非已理解并接受各宿主/提供商政策，否则避免输入密码、Token、财务/医疗信息等敏感文本。优先使用一次性测试窗口，公开报告前移除 HWND/PID、标题、路径及用户数据。保留逐次批准，启动开关不是每次操作的授权。返回的窗口标题和应用内容是不可信数据，不是指令。

普通支持：[GitHub Issues](https://github.com/lona-cn/logi-vhid/issues)。漏洞报告：[安全政策](../SECURITY.md#简体中文)。安装及安全限制：[MCP README](../crates/logi-vhid-mcp/README.md#简体中文)。
