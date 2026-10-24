"""Synchronous ctypes bindings for Logitech G HUB's virtual HID bus.

Requires Windows x64, Python 3.12+, and an available G HUB virtual bus driver
(``logi_joy_bus_enum``). Cursor positioning requires Windows 10 1703 or later.
The Windows x86_64 wheel bundles ``_native/logi_vhid.dll``. A source install
(``python -m pip install .``) normally builds it with the MSVC Rust toolchain
(Rust 1.96+); an absolute ``LOGI_VHID_NATIVE_DIR`` can instead supply a prebuilt
DLL directory at build time.

The DLL is loaded during import, before any device is opened. Set
``LOGI_VHID_DLL`` before importing to pass a specific DLL path to
``ctypes.WinDLL`` instead of using the bundled DLL; no ``target/`` directory is
searched. A missing bundled DLL raises FileNotFoundError; a DLL that Windows
cannot load raises OSError. The override must provide the expected C API.

Constructing LogitechVirtualHid opens the bus and creates/verifies the selected
virtual devices. Importing alone sends no input. Input methods affect the
current desktop/focused application, not a chosen window. Win32 Unicode and
cursor modes bypass HID reports but still require a successfully opened
instance. Use a context manager or close() explicitly; there is no finalizer.

Button constants LEFT, RIGHT, MIDDLE, BACK and FORWARD are 0 through 4.
MODE_DRIVER, MODE_WIN32 and MODE_SET_CURSOR_POS select the three cursor paths.
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
    candidate = Path(__file__).resolve().parent / "_native" / "logi_vhid.dll"
    if candidate.is_file():
        return str(candidate)
    raise FileNotFoundError(
        f"Bundled DLL not found at {candidate}; reinstall the Windows x86_64 "
        "logi-vhid wheel, build from source with `python -m pip install .` "
        "and Rust installed, or set LOGI_VHID_DLL to an existing DLL."
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
    """An open G HUB bus handle with synchronous keyboard/mouse operations.

    Args:
        keyboard: Create and verify the virtual keyboard (default True).
        mouse: Create and verify the virtual mouse (default True).

    Opening may wait up to two seconds for each selected HID device to appear.
    Passing both flags as False still opens the bus but creates neither device.
    HID keyboard/mouse methods require their corresponding flag; type_unicode
    and the two Win32 set_cursor modes do not.

    Each instance serializes all its methods, queries and close() with one
    lock. An operation blocks its caller until the native call finishes,
    including built-in delays. Different instances/processes are not serialized
    and use the shared virtual devices, not isolated keyboards or mice.

    Native failures raise RuntimeError with the DLL's error text. After close,
    all operations except close() (including entering a context) raise
    RuntimeError("handle is closed"). This takes precedence over local argument
    validation. Input failures may occur after some input was delivered;
    temporary key/button releases are attempted, not guaranteed on failure.

    Use bool flags, str text and integer numeric arguments. Numeric arguments
    cross ctypes as signed 32-bit values (coordinates/deltas) or unsigned bytes
    (usage/button/mode). Except for button/mode range checks, the wrapper does
    not validate integer ranges; out-of-range integers can wrap. Other Python
    type/encoding conversion errors propagate rather than becoming RuntimeError.

    Always use ``with LogitechVirtualHid(...) as device:`` or call close()
    explicitly. Closing releases the handle, not the shared virtual devices,
    and does not send an all-keys/buttons-up report.
    """

    def __init__(self, keyboard: bool = True, mouse: bool = True) -> None:
        self._lock = Lock()
        self._handle = None
        with self._lock:
            self._handle = _lib.logi_vhid_open_options(int(keyboard), int(mouse))
            if not self._handle:
                raise RuntimeError(_last_error())

    def close(self) -> None:
        """Wait for the current operation, then release this instance's handle.

        Repeated or concurrent calls close the handle at most once. The
        instance cannot be reopened. This does not remove virtual devices or
        guarantee release of inputs left held after a failed operation.
        """
        with self._lock:
            handle = self._handle
            self._handle = None
            if handle:
                _lib.logi_vhid_close(handle)

    def __enter__(self) -> "LogitechVirtualHid":
        """Return this open instance; entering does not open a new handle."""
        with self._lock:
            self._require_handle()
            return self

    def __exit__(self, exc_type, exc, tb) -> None:
        """Close the handle on context exit, without suppressing exceptions."""
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
        """The opened bus control-interface path as a copied Python string.

        This is not a child keyboard/mouse HID path. Raises RuntimeError after
        close(); reading the property sends no keyboard or mouse input.
        """
        with self._lock:
            pointer = _lib.logi_vhid_device_path(self._require_handle())
            return pointer.decode("utf-8", "replace") if pointer else ""

    def move(self, dx: int, dy: int) -> None:
        """Send relative virtual-mouse movement in signed HID counts.

        dx and dy must fit signed 32-bit integers; positive values move right
        and down. Counts are not physical pixels: Windows pointer settings can
        affect cursor displacement. Large deltas are split into signed 16-bit
        reports with 1 ms between reports; a failure may leave a partial move.
        Requires mouse=True. A successful report does not prove movement:
        a registered PID gate can suppress X/Y (see pid_gate_active()).
        """
        with self._lock:
            self._check(_lib.logi_vhid_move(self._require_handle(), dx, dy))

    def click(self, button: int = LEFT) -> None:
        """Press and release one virtual-mouse button, held for about 20 ms.

        Use LEFT, RIGHT, MIDDLE, BACK or FORWARD (integers 0..4).
        Requires mouse=True. Out-of-range buttons raise ValueError.
        """
        with self._lock:
            handle = self._require_handle()
            if not 0 <= button <= 4:
                raise ValueError("button must be between 0 and 4")
            self._check(_lib.logi_vhid_click(handle, button))

    def double_click(self, button: int = LEFT) -> None:
        """Send two clicks of a virtual-mouse button, separated by 40 ms.

        Each click holds the button for about 20 ms. Use LEFT, RIGHT, MIDDLE,
        BACK or FORWARD (integers 0..4); out-of-range buttons raise ValueError.
        Requires mouse=True. The target application decides how to interpret
        the clicks; failure may occur after the first click has been delivered.
        """
        with self._lock:
            handle = self._require_handle()
            if not 0 <= button <= 4:
                raise ValueError("button must be between 0 and 4")
            self._check(_lib.logi_vhid_double_click(handle, button))

    def key(self, usage: int) -> None:
        """Press and release one raw HID Keyboard/Keypad-page (0x07) usage.

        This is not a character code or Win32 virtual-key code: for example,
        0x04 is the A key and 0xE1 is Left Shift. The hold lasts about 20 ms.
        Requires keyboard=True. Supply an integer from 1 through 255; zero is
        rejected by the native library with RuntimeError, while values outside
        a byte are converted modulo 256 by ctypes rather than range-checked.
        Other usage IDs are forwarded without checking whether a key exists.
        """
        with self._lock:
            self._check(_lib.logi_vhid_key(self._require_handle(), usage))

    def type_ascii(self, text: str) -> None:
        """Type printable ASCII using virtual HID keys mapped for US layout.

        Accepts U+0020..U+007E, tab, CR and LF. CRLF sends one Enter; isolated
        CR/LF each send Enter. Caps Lock is sampled once and compensated for;
        the target keyboard layout is not changed and must match the US
        mapping for the intended text. This does not use Win32 Unicode input.

        Requires keyboard=True, including for an empty string. Embedded NUL
        raises ValueError before native input. Unsupported characters raise
        RuntimeError when reached: a valid prefix may already have been typed.
        Each character uses an approximately 8 ms key hold and an additional
        8 ms delay (except LF skipped after CR). Text is UTF-8 encoded first;
        unpaired surrogates raise UnicodeEncodeError before native input.
        """
        with self._lock:
            handle = self._require_handle()
            if "\0" in text:
                raise ValueError("text must not contain NUL")
            self._check(_lib.logi_vhid_type_ascii(handle, text.encode("utf-8")))

    def type_unicode(self, text: str) -> None:
        """Type text with Win32 SendInput / KEYEVENTF_UNICODE, not HID reports.

        No virtual keyboard is needed (keyboard=False is allowed), but the
        instance must still have opened the G HUB bus. UTF-16 units are sent as
        key-down/up pairs; supplementary characters use surrogate pairs. The
        receiving application must support this input path. This does not send
        HID Enter/Tab keys or normalize CRLF like type_ascii().

        Embedded NUL raises ValueError. Unpaired surrogates raise
        UnicodeEncodeError during UTF-8 encoding before native input. A native
        SendInput failure raises RuntimeError and may leave a prefix delivered;
        if only a key-down was inserted, its release is attempted before
        returning the error. Windows input restrictions still apply.
        """
        with self._lock:
            handle = self._require_handle()
            if "\0" in text:
                raise ValueError("text must not contain NUL")
            self._check(_lib.logi_vhid_type_unicode(handle, text.encode("utf-8")))

    def set_cursor(self, x: int, y: int, mode: int = MODE_DRIVER) -> None:
        """Move the cursor to virtual-desktop coordinates in physical pixels.

        x and y must fit signed 32-bit integers, not normalized 0..65535 values.
        Monitors left/above the primary can have negative coordinates. All
        modes clamp to the virtual desktop's bounding rectangle and temporarily
        use a physical-pixel DPI context, then attempt to restore the caller's
        context. A DPI-context failure is reported as a native RuntimeError.

        MODE_DRIVER (0, default) requires mouse=True. It probes movement and
        repeatedly sends relative HID reports to reach the target, with at most
        200 correction attempts. It raises RuntimeError if movement stalls or
        the target is not reached; it never falls back to a Win32 mode.

        MODE_WIN32 (1) sends an absolute SendInput mouse event.
        MODE_SET_CURSOR_POS (2) calls SetCursorPos directly. These modes do not
        require mouse=True and are not blocked by the driver's mouse PID gate,
        but still require an open instance and remain subject to Windows
        restrictions. They check API success, not the final cursor position.
        Modes outside 0..2 raise ValueError. Any failure may follow movement.
        """
        with self._lock:
            handle = self._require_handle()
            if not 0 <= mode <= 2:
                raise ValueError("mode must be between 0 and 2")
            self._check(_lib.logi_vhid_set_cursor(handle, x, y, mode))

    def pid_gate_active(self) -> bool:
        """Return whether the driver's registered PID list is nonempty.

        A nonempty list indicates the gate that suppresses virtual mouse X/Y.
        False does not prove the mouse/report path works. This query requires
        only an open bus (mouse=False is allowed), sends no input, and neither
        registers/unregisters a PID nor changes the gate. Query failures raise
        RuntimeError.
        """
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
