#ifndef LOGI_VHID_H
#define LOGI_VHID_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct LogiVhidHandle LogiVhidHandle;

/*
 * A non-NULL handle is owned by the caller and must be closed exactly once.
 * The caller must serialize ALL operations on the same handle, including
 * queries and close; no operation may overlap another operation or close.
 * A handle must not be used after close. Closing NULL is a no-op.
 */

/* Open with both devices, or select them explicitly. */
LogiVhidHandle *logi_vhid_open(void);
LogiVhidHandle *logi_vhid_open_options(unsigned char keyboard, unsigned char mouse);
void logi_vhid_close(LogiVhidHandle *handle);

/*
 * Calling-thread error text, owned by the library; do not free or modify it.
 * Initially empty; successful calls do not clear it. Valid until the next
 * library error on this thread or this thread exits. Errors on other threads
 * do not change or invalidate it.
 */
const char *logi_vhid_last_error(void);

/*
 * Library-owned, NUL-terminated path; valid only until this handle is closed.
 * Do not free or modify it. Finish reading/copying it before permitting close.
 */
const char *logi_vhid_device_path(LogiVhidHandle *handle);

/* All input functions return 0 on success and -1 on error. */
int logi_vhid_move(LogiVhidHandle *handle, int dx, int dy);
int logi_vhid_click(LogiVhidHandle *handle, unsigned char button);
int logi_vhid_double_click(LogiVhidHandle *handle, unsigned char button);
int logi_vhid_key(LogiVhidHandle *handle, unsigned char usage);
/*
 * text must point to readable, NUL-terminated UTF-8 bytes and remain valid and
 * unchanged for the complete call. The library does not retain the pointer.
 */
int logi_vhid_type_ascii(LogiVhidHandle *handle, const char *text);
int logi_vhid_type_unicode(LogiVhidHandle *handle, const char *text);

/* mode: 0 driver, 1 Win32 SendInput, 2 SetCursorPos */
int logi_vhid_set_cursor(LogiVhidHandle *handle, int x, int y, unsigned char mode);

/* 1 = PID gate active, 0 = inactive, -1 = error. */
int logi_vhid_pid_gate(LogiVhidHandle *handle);

/* Mouse buttons: 0 left, 1 right, 2 middle, 3 back, 4 forward. */

#ifdef __cplusplus
}
#endif

#endif
