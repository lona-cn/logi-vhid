use rmcp::{model::CallToolRequestParams, transport::TokioChildProcess, ClientLifecycleMode};
use serde_json::{json, Value};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};

const EXE: &str = env!("CARGO_BIN_EXE_logi-vhid-mcp");
const LIMIT: Duration = Duration::from_secs(5);
struct Process {
    child: Child,
    input: Option<ChildStdin>,
    output: tokio::io::Lines<BufReader<ChildStdout>>,
}
impl Process {
    fn new() -> Self {
        let mut child = Command::new(EXE)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap()).lines();
        Self {
            child,
            input,
            output,
        }
    }
    async fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        timeout(LIMIT, self.input.as_mut().unwrap().write_all(&bytes))
            .await
            .unwrap()
            .unwrap();
    }
    async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await;
        loop {
            let line = timeout(LIMIT, self.output.next_line())
                .await
                .unwrap()
                .unwrap()
                .expect("server closed stdout");
            let message: Value =
                serde_json::from_str(&line).expect("stdout must contain only JSON-RPC");
            assert_eq!(message["jsonrpc"], "2.0");
            if message["id"] == id {
                assert!(message.get("error").is_none(), "{message}");
                return message["result"].clone();
            }
        }
    }
    async fn finish(mut self) {
        self.input.take();
        let status = timeout(LIMIT, self.child.wait())
            .await
            .expect("EOF must shut down worker")
            .unwrap();
        assert!(status.success(), "server exit: {status}");
        while let Some(line) = self.output.next_line().await.unwrap() {
            if !line.is_empty() {
                let _: Value = serde_json::from_str(&line).unwrap();
            }
        }
    }
}
fn check_desktop(result: &Value) {
    let response = &result["structuredContent"];
    if response["ok"] == true {
        assert_eq!(response["data"]["coordinate_space"], "physical_pixels");
        assert!(response["data"]["cursor"]["x"].is_i64());
        assert!(response["data"]["virtual_desktop"]["right"].is_i64());
    } else {
        assert_eq!(result["isError"], true);
        assert!(matches!(
            response["error"]["code"].as_str(),
            Some("desktop_unavailable" | "native_error")
        ));
        assert!(response["data"].is_null());
    }
}
#[tokio::test]
async fn legacy_stdio_is_read_only_by_default_and_eof_exits() {
    let mut process = Process::new();
    let init=process.request(1,"initialize",json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"logi-vhid-test","version":"1"}})).await;
    assert_eq!(init["serverInfo"]["name"], "logi-vhid-mcp");
    assert_eq!(init["capabilities"], json!({"tools":{}}));
    process
        .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .await;
    let list = process.request(2, "tools/list", json!({})).await;
    let names: Vec<_> = list["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "vhid_status",
            "vhid_desktop_info",
            "vhid_mouse",
            "vhid_key",
            "vhid_type_text",
            "vhid_stop"
        ]
    );
    for (index, tool) in list["tools"].as_array().unwrap().iter().enumerate() {
        assert_eq!(tool["annotations"]["readOnlyHint"], index < 2);
        assert!(tool["outputSchema"].is_object());
        if (2..5).contains(&index) {
            assert_eq!(tool["annotations"]["destructiveHint"], true);
            assert_eq!(tool["annotations"]["idempotentHint"], false);
            assert_eq!(tool["annotations"]["openWorldHint"], true);
        }
    }
    let desktop = process
        .request(
            3,
            "tools/call",
            json!({"name":"vhid_desktop_info","arguments":{}}),
        )
        .await;
    check_desktop(&desktop);
    let key = process
        .request(
            4,
            "tools/call",
            json!({"name":"vhid_key","arguments":{"target":{"hwnd":"0x1","pid":1},"keys":["A"]}}),
        )
        .await;
    assert_eq!(key["isError"], true);
    assert_eq!(key["structuredContent"]["error"]["code"], "input_disabled");
    assert_eq!(key["structuredContent"]["data"]["submission"], "none");
    assert_eq!(key["structuredContent"]["data"]["state"], "rejected");
    let status = process
        .request(
            40,
            "tools/call",
            json!({"name":"vhid_status","arguments":{}}),
        )
        .await;
    let snapshot = &status["structuredContent"];
    assert_eq!(snapshot["ok"], true);
    assert_eq!(snapshot["data"]["allow_input"], false);
    assert_eq!(snapshot["data"]["device_initialized"], false);
    assert_eq!(snapshot["data"]["worker_state"], "idle");
    assert_eq!(
        snapshot["data"]["last_operation"]["error"]["code"],
        "input_disabled"
    );
    assert!(matches!(
        snapshot["data"]["driver"]["state"].as_str(),
        Some("available" | "not_found" | "error")
    ));
    let invalid=process.request(5,"tools/call",json!({"name":"vhid_key","arguments":{"target":{"hwnd":"0x1","pid":1},"keys":["A","A"]}})).await;
    assert_eq!(
        invalid["structuredContent"]["error"]["code"],
        "invalid_arguments"
    );
    let stop = process
        .request(6, "tools/call", json!({"name":"vhid_stop","arguments":{}}))
        .await;
    assert_eq!(stop["structuredContent"]["ok"], true);
    assert_eq!(
        stop["structuredContent"]["data"]["cleanup"]["keyboard"],
        "not_opened"
    );
    assert_eq!(
        stop["structuredContent"]["data"]["cleanup"]["mouse"],
        "not_opened"
    );
    process.finish().await;
}
#[tokio::test]
async fn sdk_discover_lifecycle_calls_real_read_only_tool() {
    let transport = TokioChildProcess::new(Command::new(EXE)).unwrap();
    let lifecycle = ClientLifecycleMode::Discover {
        preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
    };
    let mut client = timeout(
        LIMIT,
        rmcp::serve_client_with_lifecycle((), transport, lifecycle),
    )
    .await
    .unwrap()
    .unwrap();
    let tools = timeout(LIMIT, client.list_all_tools())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tools.len(), 6);
    let result = timeout(
        LIMIT,
        client.call_tool(
            CallToolRequestParams::new("vhid_desktop_info").with_arguments(serde_json::Map::new()),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    check_desktop(&serde_json::to_value(result).unwrap());
    client
        .close_with_timeout(LIMIT)
        .await
        .unwrap()
        .expect("client closes within limit");
}
#[tokio::test]
async fn startup_arguments_do_not_start_service() {
    for args in [
        vec!["--unknown"],
        vec!["--allow-input", "--allow-input"],
        vec!["--help", "--allow-input"],
    ] {
        let result = timeout(LIMIT, Command::new(EXE).args(args).output())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }
    for arg in ["--help", "--version"] {
        let result = timeout(LIMIT, Command::new(EXE).arg(arg).output())
            .await
            .unwrap()
            .unwrap();
        assert!(result.status.success());
        assert!(!result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
}

#[tokio::test]
async fn eof_before_handshake_does_not_leave_worker_running() {
    Process::new().finish().await;
}
