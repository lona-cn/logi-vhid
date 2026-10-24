"""Exercise a local MCP executable in its default, input-disabled mode.

Standard library only. The protocol sequence and child arguments are fixed; this
script never enables input, opens HID devices, or accepts arbitrary tool calls.
"""
from __future__ import annotations

import argparse
from collections import deque
import json
from pathlib import Path
from queue import Empty, Queue
import subprocess
import sys
from threading import Thread
import time

LIMIT = 5.0
TOOLS = ["vhid_status", "vhid_desktop_info", "vhid_mouse", "vhid_key", "vhid_type_text", "vhid_stop"]


class SmokeError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SmokeError(message)


def integer(value) -> bool:
    return type(value) is int


def rectangle(value) -> bool:
    return (isinstance(value, dict)
            and all(integer(value.get(key)) for key in ("left", "top", "right", "bottom"))
            and value["right"] > value["left"] and value["bottom"] > value["top"])


class ProtocolProcess:
    def __init__(self, exe: Path, *, env: dict[str, str] | None, cwd: Path | None):
        self.process = subprocess.Popen(
            [str(exe)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, encoding="utf-8", errors="strict", bufsize=1, env=env, cwd=cwd,
        )
        self.events = Queue()
        self.stderr = deque(maxlen=20)
        self.grace_timed_out = False
        self.threads = [Thread(target=self.read_stream, args=(name, stream), daemon=True)
                        for name, stream in (("stdout", self.process.stdout), ("stderr", self.process.stderr))]
        for thread in self.threads:
            thread.start()

    def read_stream(self, name, stream):
        try:
            for line in stream:
                self.events.put((name, line))
        except (OSError, UnicodeError) as error:
            self.events.put((name, error))
        finally:
            self.events.put((name, None))

    def consume(self, name, value):
        if isinstance(value, Exception):
            raise SmokeError(f"cannot decode/read {name}: {value}")
        if name == "stderr":
            if value is not None and value.strip():
                self.stderr.append(value.rstrip())
            return None
        if value is None:
            return None
        if not value.strip():
            return None
        try:
            message = json.loads(value)
        except (ValueError, TypeError) as error:
            raise SmokeError("stdout contains non-JSON protocol data") from error
        require(isinstance(message, dict) and message.get("jsonrpc") == "2.0",
                "stdout contains data other than JSON-RPC 2.0")
        return message

    def send(self, message):
        try:
            self.process.stdin.write(json.dumps(message, ensure_ascii=False) + "\n")
            self.process.stdin.flush()
        except (OSError, ValueError) as error:
            raise SmokeError("server closed stdin before the protocol sequence completed") from error

    def request(self, identifier, method, params):
        self.send({"jsonrpc": "2.0", "id": identifier, "method": method, "params": params})
        deadline = time.monotonic() + LIMIT
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise SmokeError(f"{method} response timed out")
            try:
                name, value = self.events.get(timeout=remaining)
            except Empty as error:
                raise SmokeError(f"{method} response timed out") from error
            if name == "stdout" and value is None:
                raise SmokeError(f"server exited before replying to {method}")
            message = self.consume(name, value)
            if message is None or "id" not in message:
                continue
            require(message["id"] == identifier, "unexpected JSON-RPC response identifier")
            require("error" not in message, f"{method} returned a JSON-RPC error instead of a tool result")
            require(isinstance(message.get("result"), dict), f"{method} result is not an object")
            return message["result"]

    def call(self, identifier, name, arguments):
        result = self.request(identifier, "tools/call", {"name": name, "arguments": arguments})
        response = result.get("structuredContent")
        require(isinstance(response, dict) and type(response.get("ok")) is bool
                and "data" in response and "error" in response,
                f"{name} did not return a structured tool envelope")
        return result, response

    def finish(self):
        deadline = time.monotonic() + LIMIT
        self.process.stdin.close()
        try:
            status = self.process.wait(timeout=LIMIT)
        except subprocess.TimeoutExpired as error:
            self.grace_timed_out = True
            raise SmokeError("server did not exit within five seconds of stdin EOF") from error
        for thread in self.threads:
            thread.join(timeout=max(0.0, deadline - time.monotonic()))
            require(not thread.is_alive(), "protocol output did not close after server exit")
        while True:
            try:
                name, value = self.events.get_nowait()
            except Empty:
                break
            self.consume(name, value)
        require(status == 0, f"server exited with status {status}")

    def close(self):
        # Only this instance's child is eligible for termination, and only after
        # EOF plus a bounded graceful wait failed. Never touch other clients.
        try:
            if not self.process.stdin.closed:
                self.process.stdin.close()
        except (OSError, ValueError):
            pass
        if self.process.poll() is None:
            if not self.grace_timed_out:
                try:
                    self.process.wait(timeout=LIMIT)
                except subprocess.TimeoutExpired:
                    self.grace_timed_out = True
            if self.grace_timed_out and self.process.poll() is None:
                self.process.terminate()
                try:
                    self.process.wait(timeout=1.0)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=1.0)
        for thread in self.threads:
            thread.join(timeout=1.0)
        self.process.stdout.close()
        self.process.stderr.close()


def check_desktop(result, response):
    if response["ok"]:
        require(not result.get("isError", False) and response["error"] is None,
                "desktop success was marked as an error")
        data = response["data"]
        require(isinstance(data, dict) and data.get("coordinate_space") == "physical_pixels",
                "desktop coordinates are not identified as physical pixels")
        require(rectangle(data.get("virtual_desktop")), "desktop bounding rectangle is missing or invalid")
        monitors = data.get("monitors")
        require(isinstance(monitors, list) and bool(monitors)
                and all(isinstance(monitor, dict) and rectangle(monitor.get("rect")) for monitor in monitors),
                "successful desktop query has no actual monitor rectangles")
        cursor = data.get("cursor")
        require(isinstance(cursor, dict) and integer(cursor.get("x")) and integer(cursor.get("y")),
                "desktop query has no cursor coordinates")
    else:
        error = response["error"]
        require(result.get("isError") is True and response["data"] is None
                and isinstance(error, dict) and error.get("code") in ("desktop_unavailable", "native_error")
                and isinstance(error.get("message"), str) and bool(error["message"]),
                "unavailable desktop did not return an explicit structured tool error")


def run_smoke(exe: Path, *, env: dict[str, str] | None = None, cwd: Path | None = None) -> None:
    exe = Path(exe).resolve(strict=True)
    client = ProtocolProcess(exe, env=env, cwd=cwd)
    try:
        initialize = client.request(1, "initialize", {
            "protocolVersion": "2025-11-25", "capabilities": {},
            "clientInfo": {"name": "logi-vhid-mcp-smoke", "version": "1.0.0"},
        })
        require(initialize.get("serverInfo", {}).get("name") == "logi-vhid-mcp",
                "unexpected MCP server identity")
        require(initialize.get("protocolVersion") == "2025-11-25", "legacy protocol version was not negotiated")
        require(initialize.get("capabilities") == {"tools": {}}, "server advertised unexpected capabilities")
        client.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        listed = client.request(2, "tools/list", {})
        tools = listed.get("tools")
        require(isinstance(tools, list) and [tool.get("name") for tool in tools] == TOOLS,
                "tool discovery did not expose the six expected tools in order")
        desktop_result, desktop = client.call(3, "vhid_desktop_info", {})
        check_desktop(desktop_result, desktop)
        key_result, key = client.call(4, "vhid_key", {"target": {"hwnd": "0x1", "pid": 1}, "keys": ["A"]})
        require(key_result.get("isError") is True and key["ok"] is False
                and isinstance(key["error"], dict) and key["error"].get("code") == "input_disabled",
                "default mode did not reject valid input with input_disabled")
        outcome = key["data"]
        require(isinstance(outcome, dict) and outcome.get("submission") == "none"
                and outcome.get("state") == "rejected", "disabled input was not rejected before submission")
        stop_result, stop = client.call(5, "vhid_stop", {})
        require(stop["ok"] is True and stop["error"] is None and not stop_result.get("isError", False),
                "stop failed in default mode")
        outcome = stop["data"]
        cleanup = outcome.get("cleanup") if isinstance(outcome, dict) else None
        require(isinstance(cleanup, dict) and cleanup.get("keyboard") == "not_opened"
                and cleanup.get("mouse") == "not_opened" and cleanup.get("errors") == []
                and outcome.get("state") == "completed" and outcome.get("submission") == "none",
                "default-mode stop opened devices or did not complete")
        client.finish()
    except (SmokeError, OSError) as error:
        details = "\n".join(client.stderr)
        if details:
            raise SmokeError(f"{error}\nserver stderr:\n{details}") from error
        raise
    finally:
        client.close()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, type=Path)
    arguments = parser.parse_args(argv)
    try:
        run_smoke(arguments.exe)
    except (SmokeError, OSError) as error:
        print(f"mcp-smoke: {error}", file=sys.stderr)
        return 1
    print("MCP safe smoke passed: discovery, desktop query, input_disabled, stop, EOF")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
