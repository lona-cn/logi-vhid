#ifndef LOGI_VHID_H
#define LOGI_VHID_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct LogiVhidHandle LogiVhidHandle;

/*
 * Windows x64 C ABI. Calls are synchronous; input goes to the current desktop,
 * not to a specified window. Open calls require the G HUB virtual bus driver.
 *
 * A non-NULL handle is owned by the caller and must be closed exactly once.
 * The caller must serialize ALL operations on the same handle, including
 * queries and close; no operation may overlap another operation or close.
 * A handle must not be used after close. Closing NULL is a no-op.
 * Close only releases the control handle: it does not send key/button release
 * reports or remove the shared virtual devices. Separate handles do not provide
 * independent virtual keyboards/mice; avoid interleaving their input.
 */

/*
 * Open with both devices, or select them explicitly: 0 disables a device and
 * any nonzero value enables it. Both may be 0 to open only the control bus.
 * Selected devices are created if needed, then checked for HID presence;
 * opening can wait up to two seconds for each selected device.
 * Returns NULL on failure; call logi_vhid_last_error() on the same thread.
 * An open failure may leave virtual devices already created on the bus.
 * HID keyboard/mouse calls require that device to be enabled. Unicode text
 * and Win32 cursor modes do not, but still require a live handle in this API.
 */
LogiVhidHandle *logi_vhid_open(void);
LogiVhidHandle *logi_vhid_open_options(unsigned char keyboard, unsigned char mouse);
void logi_vhid_close(LogiVhidHandle *handle);

/*
 * Calling-thread NUL-terminated UTF-8 error text, owned by the library;
 * do not free or modify it. Initially empty; successful calls do not clear it.
 * Valid until the next library error on this thread or this thread exits.
 * Errors on other threads do not change or invalidate it. Read/copy it after
 * a reported failure, not as a success/failure test by itself.
 */
const char *logi_vhid_last_error(void);

/*
 * Library-owned, NUL-terminated UTF-8 control-device path; valid only until
 * this handle is closed. Do not free or modify it. Finish reading/copying it
 * before permitting close. A NULL handle returns NULL without setting an error.
 */
const char *logi_vhid_device_path(LogiVhidHandle *handle);

/*
 * Input functions below return 0 on success and -1 on error; on error, read
 * logi_vhid_last_error() on the calling thread. A NULL handle returns -1.
 * Any non-NULL handle must be live. Failure may follow partial input; input
 * already delivered is not undone, and attempted cleanup can also fail.
 */
/* Relative HID counts, not guaranteed pixels; Windows pointer settings apply. */
int logi_vhid_move(LogiVhidHandle *handle, int dx, int dy);
/* Click at the current cursor position: press then release. button: 0 left,
 * 1 right, 2 middle, 3 back, 4 forward. Other values return -1. */
int logi_vhid_click(LogiVhidHandle *handle, unsigned char button);
/* Two clicks at the current cursor position; application recognition may vary. */
int logi_vhid_double_click(LogiVhidHandle *handle, unsigned char button);
/* Press and release one HID Keyboard/Keypad usage (page 0x07), NOT a Win32
 * virtual-key code or a character code. For example, A is 0x04. */
int logi_vhid_key(LogiVhidHandle *handle, unsigned char usage);
/*
 * text must point to readable, NUL-terminated UTF-8 bytes and remain valid and
 * unchanged for the complete call. The library does not retain the pointer.
 * NULL or invalid UTF-8 returns -1. The first NUL terminates the text.
 */
/* US-layout HID input: printable ASCII plus tab, CR and LF; CRLF is one Enter.
 * The target input layout must match US. Unsupported text may fail after a
 * prefix has been typed. The function does not switch the target's layout. */
int logi_vhid_type_ascii(LogiVhidHandle *handle, const char *text);
/* Win32 SendInput Unicode events, not G HUB keyboard reports. Accepts non-ASCII
 * and non-BMP text. CR/LF/tab are sent literally, not as HID Enter/Tab presses.
 * Success reports event insertion, not that the target application accepted it. */
int logi_vhid_type_unicode(LogiVhidHandle *handle, const char *text);

/*
 * Absolute physical screen pixels, including negative multi-monitor positions.
 * Coordinates are clamped to the virtual desktop's bounding rectangle.
 * mode: 0 driver (relative HID correction, verifies arrival, no fallback),
 * 1 Win32 SendInput, 2 SetCursorPos. Other values return -1.
 * Win32 modes do not verify arrival. No mode activates or focuses a window.
 */
int logi_vhid_set_cursor(LogiVhidHandle *handle, int x, int y, unsigned char mode);

/* 1 = PID gate active (driver zeroes relative mouse X/Y), 0 = inactive,
 * -1 = error (including NULL handle). This query does not change driver state.
 * A successful relative move call alone does not prove the cursor moved. */
int logi_vhid_pid_gate(LogiVhidHandle *handle);

#ifdef __cplusplus
}
#endif

#endif
