"""Use logi-vhid from Python through ctypes.

Build the library first:

    cargo build --release

Set LOGI_VHID_DLL to load a specific DLL. Otherwise, the module looks for
target/release/logi_vhid.dll, then target/debug/logi_vhid.dll in the project.
"""

from __future__ import annotations

import ctypes
import os
from ctypes import c_char_p, c_int, c_ubyte, c_void_p
from pathlib import Path
from threading import Lock

LEFT = 0
RIGHT = 1
MIDDLE = 2
BACK = 3
FORWARD = 4

MODE_DRIVER = 0
MODE_WIN32 = 1
MODE_SET_CURSOR_POS = 2


def _find_library() -> str:
    override = os.environ.get("LOGI_VHID_DLL")
    if override:
        return override
    root = Path(__file__).resolve().parent.parent
    for relative in ("target/release/logi_vhid.dll", "target/debug/logi_vhid.dll"):
        candidate = root / relative
        if candidate.exists():
            return str(candidate)
    raise FileNotFoundError(
        "logi_vhid.dll not found; run `cargo build --release` or set LOGI_VHID_DLL"
    )


_lib = ctypes.WinDLL(_find_library())
_lib.logi_vhid_open.restype = c_void_p
_lib.logi_vhid_open_options.argtypes = [c_ubyte, c_ubyte]
_lib.logi_vhid_open_options.restype = c_void_p
_lib.logi_vhid_close.argtypes = [c_void_p]
_lib.logi_vhid_close.restype = None
_lib.logi_vhid_last_error.restype = c_char_p
_lib.logi_vhid_device_path.argtypes = [c_void_p]
_lib.logi_vhid_device_path.restype = c_char_p
_lib.logi_vhid_move.argtypes = [c_void_p, c_int, c_int]
_lib.logi_vhid_move.restype = c_int
_lib.logi_vhid_click.argtypes = [c_void_p, c_ubyte]
_lib.logi_vhid_click.restype = c_int
_lib.logi_vhid_double_click.argtypes = [c_void_p, c_ubyte]
_lib.logi_vhid_double_click.restype = c_int
_lib.logi_vhid_key.argtypes = [c_void_p, c_ubyte]
_lib.logi_vhid_key.restype = c_int
_lib.logi_vhid_type_ascii.argtypes = [c_void_p, c_char_p]
_lib.logi_vhid_type_ascii.restype = c_int
_lib.logi_vhid_type_unicode.argtypes = [c_void_p, c_char_p]
_lib.logi_vhid_type_unicode.restype = c_int
_lib.logi_vhid_set_cursor.argtypes = [c_void_p, c_int, c_int, c_ubyte]
_lib.logi_vhid_set_cursor.restype = c_int
_lib.logi_vhid_pid_gate.argtypes = [c_void_p]
_lib.logi_vhid_pid_gate.restype = c_int


def _last_error() -> str:
    pointer = _lib.logi_vhid_last_error()
    return pointer.decode("utf-8", "replace") if pointer else "unknown error"


class LogitechVirtualHid:
    """Virtual keyboard and mouse. Each instance runs one operation at a time."""

    def __init__(self, keyboard: bool = True, mouse: bool = True) -> None:
        self._lock = Lock()
        self._handle = None
        with self._lock:
            self._handle = _lib.logi_vhid_open_options(int(keyboard), int(mouse))
            if not self._handle:
                raise RuntimeError(_last_error())

    def close(self) -> None:
        """Wait for the current operation, then close. Repeated calls do nothing."""
        with self._lock:
            handle = self._handle
            self._handle = None
            if handle:
                _lib.logi_vhid_close(handle)

    def __enter__(self) -> "LogitechVirtualHid":
        with self._lock:
            self._require_handle()
            return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()

    def _check(self, result: int) -> None:
        if result != 0:
            raise RuntimeError(_last_error())

    def _require_handle(self):
        if not self._handle:
            raise RuntimeError("handle is closed")
        return self._handle

    @property
    def device_path(self) -> str:
        with self._lock:
            pointer = _lib.logi_vhid_device_path(self._require_handle())
            return pointer.decode("utf-8", "replace") if pointer else ""

    def move(self, dx: int, dy: int) -> None:
        with self._lock:
            self._check(_lib.logi_vhid_move(self._require_handle(), dx, dy))

    def click(self, button: int = LEFT) -> None:
        with self._lock:
            handle = self._require_handle()
            if not 0 <= button <= 4:
                raise ValueError("button must be between 0 and 4")
            self._check(_lib.logi_vhid_click(handle, button))

    def double_click(self, button: int = LEFT) -> None:
        with self._lock:
            handle = self._require_handle()
            if not 0 <= button <= 4:
                raise ValueError("button must be between 0 and 4")
            self._check(_lib.logi_vhid_double_click(handle, button))

    def key(self, usage: int) -> None:
        with self._lock:
            self._check(_lib.logi_vhid_key(self._require_handle(), usage))

    def type_ascii(self, text: str) -> None:
        with self._lock:
            handle = self._require_handle()
            if "\0" in text:
                raise ValueError("text must not contain NUL")
            self._check(_lib.logi_vhid_type_ascii(handle, text.encode("utf-8")))

    def type_unicode(self, text: str) -> None:
        with self._lock:
            handle = self._require_handle()
            if "\0" in text:
                raise ValueError("text must not contain NUL")
            self._check(_lib.logi_vhid_type_unicode(handle, text.encode("utf-8")))

    def set_cursor(self, x: int, y: int, mode: int = MODE_DRIVER) -> None:
        with self._lock:
            handle = self._require_handle()
            if not 0 <= mode <= 2:
                raise ValueError("mode must be between 0 and 2")
            self._check(_lib.logi_vhid_set_cursor(handle, x, y, mode))

    def pid_gate_active(self) -> bool:
        with self._lock:
            result = _lib.logi_vhid_pid_gate(self._require_handle())
            if result < 0:
                raise RuntimeError(_last_error())
            return result == 1


__all__ = [
    "LogitechVirtualHid",
    "LEFT",
    "RIGHT",
    "MIDDLE",
    "BACK",
    "FORWARD",
    "MODE_DRIVER",
    "MODE_WIN32",
    "MODE_SET_CURSOR_POS",
]
