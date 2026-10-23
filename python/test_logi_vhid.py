"""Lifecycle tests with a controlled DLL boundary; no device is opened."""

import ctypes
import importlib.util
import os
from pathlib import Path
import threading
import unittest
from unittest import mock


class NativeFunction:
    def __init__(self, library, name):
        self.library = library
        self.name = name

    def __call__(self, *args):
        return self.library.invoke(self.name, args)


class NativeLibrary:
    def __init__(self):
        self.handle = object()
        self.live = False
        self.calls = []
        self.callbacks = {}
        self.functions = {}
        self.open_result = self.handle

    def __getattr__(self, name):
        if name not in self.functions:
            self.functions[name] = NativeFunction(self, name)
        return self.functions[name]

    def invoke(self, name, args):
        self.calls.append((name, args))
        if name == "logi_vhid_open_options":
            self.live = self.open_result is not None
            return self.open_result
        if name == "logi_vhid_last_error":
            return b"controlled open failure"
        if not args or args[0] is not self.handle:
            raise AssertionError("native call received an unknown handle")
        if not self.live:
            raise AssertionError("native call used a closed handle")
        callback = self.callbacks.get(name)
        result = callback() if callback else None
        if name == "logi_vhid_close":
            self.live = False
            return None
        if name == "logi_vhid_device_path":
            return b"controlled-device-path"
        if name == "logi_vhid_pid_gate":
            return 1
        return 0 if result is None else result


class ObservedLock:
    """Expose contention without deciding ordering from a sleep or timeout."""

    def __init__(self, contention):
        self.lock = threading.Lock()
        self.contention = contention

    def __enter__(self):
        if not self.lock.acquire(blocking=False):
            self.contention.set()
            self.lock.acquire()
        return self

    def __exit__(self, *exc):
        self.lock.release()


OPERATIONS = (
    ("logi_vhid_move", lambda device: device.move(1, -2)),
    ("logi_vhid_click", lambda device: device.click()),
    ("logi_vhid_double_click", lambda device: device.double_click()),
    ("logi_vhid_key", lambda device: device.key(4)),
    ("logi_vhid_type_ascii", lambda device: device.type_ascii("text")),
    ("logi_vhid_type_unicode", lambda device: device.type_unicode("文字")),
    ("logi_vhid_set_cursor", lambda device: device.set_cursor(12, 34)),
    ("logi_vhid_device_path", lambda device: device.device_path),
    ("logi_vhid_pid_gate", lambda device: device.pid_gate_active()),
)


class LifecycleTests(unittest.TestCase):
    def setUp(self):
        self.native = NativeLibrary()
        # Patch only library discovery/loading. All methods under test are real.
        with mock.patch.dict(os.environ, {"LOGI_VHID_DLL": "controlled-test.dll"}):
            with mock.patch.object(ctypes, "WinDLL", return_value=self.native, create=True):
                spec = importlib.util.spec_from_file_location(
                    "logi_vhid_lifecycle_test", Path(__file__).with_name("logi_vhid.py")
                )
                self.module = importlib.util.module_from_spec(spec)
                spec.loader.exec_module(self.module)

    def start_worker(self, action):
        errors = []
        done = threading.Event()

        def run():
            try:
                action()
            except BaseException as error:
                errors.append(error)
            finally:
                done.set()

        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        return thread, done, errors

    def join_worker(self, worker):
        thread, done, errors = worker
        thread.join(5)
        self.assertTrue(done.is_set(), "worker did not finish")
        if errors:
            raise errors[0]

    def wait_for(self, event):
        # The deadline only detects a stuck test; the event establishes ordering.
        self.assertTrue(event.wait(5), "worker did not reach its synchronization point")

    def test_concurrent_close_releases_handle_once(self):
        device = self.module.LogitechVirtualHid()
        first_entered = threading.Event()
        second_reached = threading.Event()
        release_first = threading.Event()
        device._lock = ObservedLock(second_reached)
        closes = []

        def close_native():
            closes.append(self.native.handle)
            if len(closes) == 1:
                first_entered.set()
                if not release_first.wait(5):
                    raise AssertionError("first close was not released")
            else:
                second_reached.set()

        self.native.callbacks["logi_vhid_close"] = close_native
        first = self.start_worker(device.close)
        second = None
        try:
            self.wait_for(first_entered)
            second = self.start_worker(device.close)
            self.wait_for(second_reached)
            self.assertEqual(len(closes), 1)
            self.assertFalse(second[1].is_set())
        finally:
            release_first.set()
            try:
                self.join_worker(first)
            finally:
                if second is not None:
                    self.join_worker(second)
        device.close()
        self.assertEqual(len(closes), 1)

    def test_close_waits_for_each_native_operation(self):
        for native_name, operation in OPERATIONS:
            with self.subTest(operation=native_name):
                device = self.module.LogitechVirtualHid()
                entered = threading.Event()
                close_reached = threading.Event()
                release_operation = threading.Event()
                device._lock = ObservedLock(close_reached)
                closes = []

                def blocked_operation():
                    entered.set()
                    if not release_operation.wait(5):
                        raise AssertionError("native operation was not released")

                def close_native():
                    closes.append(self.native.handle)
                    close_reached.set()

                self.native.callbacks = {
                    native_name: blocked_operation,
                    "logi_vhid_close": close_native,
                }
                active = self.start_worker(lambda: operation(device))
                closing = None
                try:
                    self.wait_for(entered)
                    closing = self.start_worker(device.close)
                    self.wait_for(close_reached)
                    self.assertEqual(closes, [])
                    self.assertFalse(closing[1].is_set())
                finally:
                    release_operation.set()
                    try:
                        self.join_worker(active)
                    finally:
                        if closing is not None:
                            self.join_worker(closing)
                self.assertEqual(closes, [self.native.handle])

    def test_closed_methods_never_enter_native_code(self):
        device = self.module.LogitechVirtualHid()
        device.close()
        calls_after_close = len(self.native.calls)
        for native_name, operation in OPERATIONS:
            with self.subTest(operation=native_name):
                with self.assertRaises(RuntimeError):
                    operation(device)
                self.assertEqual(len(self.native.calls), calls_after_close)

    def test_text_with_embedded_nul_is_rejected_before_native_input(self):
        device = self.module.LogitechVirtualHid()
        self.addCleanup(device.close)
        calls_before_input = list(self.native.calls)
        for method, text in (
            ("type_ascii", "before\0after"),
            ("type_unicode", "文字\0after"),
        ):
            with self.subTest(method=method):
                with self.assertRaises(ValueError):
                    getattr(device, method)(text)
                self.assertEqual(self.native.calls, calls_before_input)

    def test_invalid_buttons_are_rejected_before_native_input(self):
        device = self.module.LogitechVirtualHid()
        self.addCleanup(device.close)
        calls_before_input = list(self.native.calls)
        for method in ("click", "double_click"):
            for button in (-256, -254, -1, 5, 255, 256, 258):
                with self.subTest(method=method, button=button):
                    with self.assertRaises(ValueError):
                        getattr(device, method)(button)
                    self.assertEqual(self.native.calls, calls_before_input)

    def test_invalid_cursor_modes_are_rejected_before_native_input(self):
        device = self.module.LogitechVirtualHid()
        self.addCleanup(device.close)
        calls_before_input = list(self.native.calls)
        for mode in (-256, -254, -1, 3, 255, 256, 258):
            with self.subTest(mode=mode):
                with self.assertRaises(ValueError):
                    device.set_cursor(12, 34, mode)
                self.assertEqual(self.native.calls, calls_before_input)

    def test_closed_handle_error_precedes_invalid_input(self):
        device = self.module.LogitechVirtualHid()
        device.close()
        calls_after_close = list(self.native.calls)
        for method, args in (
            ("type_ascii", ("before\0after",)),
            ("type_unicode", ("文字\0after",)),
            ("click", (256,)),
            ("double_click", (-256,)),
            ("set_cursor", (12, 34, 258)),
        ):
            with self.subTest(method=method):
                with self.assertRaises(RuntimeError):
                    getattr(device, method)(*args)
                self.assertEqual(self.native.calls, calls_after_close)


    def test_closed_handle_cannot_reenter_context(self):
        device = self.module.LogitechVirtualHid()
        device.close()
        calls_after_close = len(self.native.calls)
        with self.assertRaises(RuntimeError):
            with device:
                self.fail("closed context was entered")
        self.assertEqual(len(self.native.calls), calls_after_close)


if __name__ == "__main__":
    unittest.main()
