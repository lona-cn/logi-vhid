use logi_vhid::{hid, AbsoluteMode, KeyboardLayout, MouseButton, TextMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmptyParams {}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct DesktopParams {
    pub window: Option<TargetWindow>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TargetWindow {
    pub hwnd: String,
    pub pid: u32,
}
impl TargetWindow {
    pub fn handle(&self) -> Result<usize, ToolFailure> {
        let digits = self
            .hwnd
            .strip_prefix("0x")
            .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| invalid("hwnd must be a nonzero 0x-prefixed hexadecimal pointer"))?;
        usize::from_str_radix(digits, 16)
            .ok()
            .filter(|value| *value != 0)
            .ok_or_else(|| invalid("hwnd must fit a nonzero native pointer"))
    }
    pub fn validate(&self) -> Result<(), ToolFailure> {
        self.handle()?;
        if self.pid == 0 {
            return Err(invalid("pid must be nonzero"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CoordinateSpace {
    Screen,
    Client,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PointSpec {
    pub space: CoordinateSpace,
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PositionMode {
    #[default]
    Driver,
    Win32,
    Set,
}
impl PositionMode {
    pub fn core(self) -> AbsoluteMode {
        match self {
            Self::Driver => AbsoluteMode::Driver,
            Self::Win32 => AbsoluteMode::Win32,
            Self::Set => AbsoluteMode::SetCursorPos,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Button {
    #[default]
    Left,
    Right,
    Middle,
    Back,
    Forward,
}
impl Button {
    pub fn core(self) -> MouseButton {
        match self {
            Self::Left => MouseButton::Left,
            Self::Right => MouseButton::Right,
            Self::Middle => MouseButton::Middle,
            Self::Back => MouseButton::Back,
            Self::Forward => MouseButton::Forward,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum MouseAction {
    MoveRelative {
        dx: i16,
        dy: i16,
    },
    MoveTo {
        at: PointSpec,
        mode: Option<PositionMode>,
    },
    Click {
        at: PointSpec,
        button: Option<Button>,
        clicks: Option<u8>,
        hold_ms: Option<u64>,
        mode: Option<PositionMode>,
    },
    Scroll {
        delta: i8,
        at: Option<PointSpec>,
        mode: Option<PositionMode>,
    },
    Drag {
        from: PointSpec,
        to: PointSpec,
        button: Option<Button>,
        hold_ms: Option<u64>,
        mode: Option<PositionMode>,
    },
}
impl MouseAction {
    pub fn hold_ms(&self) -> u64 {
        match self {
            Self::Click { hold_ms, .. } | Self::Drag { hold_ms, .. } => hold_ms.unwrap_or(20),
            _ => 0,
        }
    }
    pub fn mode(&self) -> PositionMode {
        match self {
            Self::MoveRelative { .. } => PositionMode::Driver,
            Self::MoveTo { mode, .. }
            | Self::Click { mode, .. }
            | Self::Scroll { mode, .. }
            | Self::Drag { mode, .. } => mode.unwrap_or_default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct MouseParams {
    pub target: TargetWindow,
    pub action: MouseAction,
    pub timeout_ms: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyParams {
    pub target: TargetWindow,
    pub keys: Vec<KeyName>,
    pub hold_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TextInputMode {
    HidUs,
    HidUk,
    HidDe,
    HidFr,
    Unicode,
}
impl TextInputMode {
    pub fn core(self) -> TextMode {
        match self {
            Self::HidUs => TextMode::Hid(KeyboardLayout::Us),
            Self::HidUk => TextMode::Hid(KeyboardLayout::Uk),
            Self::HidDe => TextMode::Hid(KeyboardLayout::De),
            Self::HidFr => TextMode::Hid(KeyboardLayout::Fr),
            Self::Unicode => TextMode::Win32Unicode,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct TextParams {
    pub target: TargetWindow,
    pub text: String,
    pub mode: TextInputMode,
    pub timeout_ms: Option<u64>,
}

macro_rules! key_names {
    ($($name:ident),+ $(,)?) => {
        #[allow(non_camel_case_types, clippy::upper_case_acronyms)]
        #[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
        pub(crate) enum KeyName { $($name),+ }
        impl KeyName { pub fn usage(self) -> u8 { match self { $(Self::$name=>hid::$name),+ } } }
    };
}
key_names! {
    A,B,C,D,E,F,G,H,I,J,K,L,M,N,O,P,Q,R,S,T,U,V,W,X,Y,Z,
    NUM_0,NUM_1,NUM_2,NUM_3,NUM_4,NUM_5,NUM_6,NUM_7,NUM_8,NUM_9,
    ENTER,ESCAPE,BACKSPACE,TAB,SPACE,MINUS,EQUAL,LEFT_BRACKET,RIGHT_BRACKET,BACKSLASH,
    SEMICOLON,APOSTROPHE,GRAVE,COMMA,PERIOD,SLASH,CAPS_LOCK,
    F1,F2,F3,F4,F5,F6,F7,F8,F9,F10,F11,F12,F24,
    INSERT,HOME,PAGE_UP,DELETE,END,PAGE_DOWN,RIGHT,LEFT,DOWN,UP,NUM_LOCK,
    LEFT_CTRL,LEFT_SHIFT,LEFT_ALT,LEFT_GUI,RIGHT_CTRL,RIGHT_SHIFT,RIGHT_ALT,RIGHT_GUI
}
#[derive(Clone, Debug)]
pub(crate) enum InputRequest {
    Mouse(MouseParams),
    Key(KeyParams),
    Text(TextParams),
}
impl InputRequest {
    pub fn target(&self) -> &TargetWindow {
        match self {
            Self::Mouse(p) => &p.target,
            Self::Key(p) => &p.target,
            Self::Text(p) => &p.target,
        }
    }
    pub fn timeout_ms(&self) -> u64 {
        match self {
            Self::Mouse(p) => p.timeout_ms,
            Self::Key(p) => p.timeout_ms,
            Self::Text(p) => p.timeout_ms,
        }
        .unwrap_or(10_000)
    }
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Text(p) if p.text.is_empty())
    }
    pub fn validate(&self) -> Result<(), ToolFailure> {
        self.target().validate()?;
        let timeout = self.timeout_ms();
        if !(1..=30_000).contains(&timeout) {
            return Err(invalid("timeout_ms must be between 1 and 30000"));
        }
        let hold = match self {
            Self::Key(p) => {
                if p.keys.is_empty() {
                    return Err(invalid("keys must not be empty"));
                }
                let mut seen = [false; 256];
                let mut ordinary = 0;
                for key in &p.keys {
                    let usage = key.usage();
                    if seen[usage as usize] {
                        return Err(invalid("keys must not contain duplicates"));
                    }
                    seen[usage as usize] = true;
                    if !(0xe0..=0xe7).contains(&usage) {
                        ordinary += 1;
                    }
                }
                if ordinary > 6 {
                    return Err(invalid("at most six non-modifier keys are allowed"));
                }
                Some(p.hold_ms.unwrap_or(20))
            }
            Self::Text(p) => {
                if p.text.chars().take(1025).count() > 1024 {
                    return Err(invalid("text exceeds 1024 Unicode scalars"));
                }
                if p.text.contains('\0') {
                    return Err(invalid("text must not contain NUL"));
                }
                None
            }
            Self::Mouse(p) => match &p.action {
                MouseAction::MoveRelative { dx: 0, dy: 0 } => {
                    return Err(invalid("relative movement must be nonzero"))
                }
                MouseAction::Scroll { delta: 0, .. } => {
                    return Err(invalid("scroll delta must be nonzero"))
                }
                MouseAction::Click { clicks, .. } if !matches!(clicks.unwrap_or(1), 1 | 2) => {
                    return Err(invalid("clicks must be 1 or 2"))
                }
                MouseAction::Click { .. } | MouseAction::Drag { .. } => Some(p.action.hold_ms()),
                _ => None,
            },
        };
        if let Some(hold) = hold {
            if !(1..=5000).contains(&hold) {
                return Err(invalid("hold_ms must be between 1 and 5000"));
            }
            if timeout < hold {
                return Err(invalid("timeout_ms must not be shorter than hold_ms"));
            }
        }
        Ok(())
    }
    pub fn selected_backends(&self) -> Vec<BackendKind> {
        match self {
            Self::Key(_) => vec![BackendKind::GhubHidKeyboard],
            Self::Text(p) => vec![if p.mode == TextInputMode::Unicode {
                BackendKind::Win32Unicode
            } else {
                BackendKind::GhubHidKeyboard
            }],
            Self::Mouse(p) => {
                let mut backends = Vec::with_capacity(2);
                let position = match &p.action {
                    MouseAction::MoveRelative { .. } => true,
                    MouseAction::Scroll { at, .. } => at.is_some(),
                    _ => true,
                };
                if position {
                    backends.push(match p.action.mode() {
                        PositionMode::Driver => BackendKind::GhubHidMouse,
                        PositionMode::Win32 => BackendKind::Win32SendinputMouse,
                        PositionMode::Set => BackendKind::Win32SetCursorPos,
                    });
                }
                if matches!(
                    p.action,
                    MouseAction::Click { .. }
                        | MouseAction::Scroll { .. }
                        | MouseAction::Drag { .. }
                ) && !backends.contains(&BackendKind::GhubHidMouse)
                {
                    backends.push(BackendKind::GhubHidMouse);
                }
                backends
            }
        }
    }
}
fn invalid(message: &'static str) -> ToolFailure {
    ToolFailure::new(ErrorCode::InvalidArguments, message)
}

macro_rules! wire_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
        #[serde(rename_all="snake_case")]
        pub(crate) enum $name { $($variant),+ }
    };
}
wire_enum!(ErrorCode {
    InvalidArguments,
    InputDisabled,
    Busy,
    DeviceBusy,
    DriverNotFound,
    DesktopUnavailable,
    TargetChanged,
    PointNotOnTarget,
    PositionNotReached,
    PidGateActive,
    Cancelled,
    TimedOut,
    NativeError,
    CleanupFailed,
    WorkerClosed
});
wire_enum!(NativeKind {
    Io,
    Protocol,
    Unsupported,
    NotFound,
    Cancelled,
    TimedOut
});
wire_enum!(OperationState {
    Completed,
    Rejected,
    Failed,
    Cancelled,
    TimedOut,
    Stopping
});
wire_enum!(Submission {
    None,
    Complete,
    PartialOrUnknown
});
wire_enum!(BackendKind {
    GhubHidKeyboard,
    GhubHidMouse,
    Win32Unicode,
    Win32SendinputMouse,
    Win32SetCursorPos
});
wire_enum!(ReleaseState {
    NotOpened,
    Released,
    Failed,
    Pending
});
wire_enum!(WorkerState {
    Idle,
    Running,
    Stopping,
    Faulted,
    Closed
});
wire_enum!(DriverState {
    Available,
    NotFound,
    Error
});
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct ToolResponse<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<ToolFailure>,
}
impl<T> ToolResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            error: None,
        }
    }
    pub fn failure(error: ToolFailure) -> Self {
        Self {
            ok: false,
            data: None,
            error: Some(error),
        }
    }
    pub fn with_error(data: T, error: ToolFailure) -> Self {
        Self {
            ok: false,
            data: Some(data),
            error: Some(error),
        }
    }
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct ToolFailure {
    pub code: ErrorCode,
    pub message: String,
    pub native: Option<NativeError>,
}
impl ToolFailure {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            native: None,
        }
    }
    pub fn from_core(error: &logi_vhid::Error) -> Self {
        let code = match error {
            logi_vhid::Error::Cancelled => ErrorCode::Cancelled,
            logi_vhid::Error::TimedOut => ErrorCode::TimedOut,
            logi_vhid::Error::NotFound(_) => ErrorCode::DriverNotFound,
            _ => ErrorCode::NativeError,
        };
        let native = NativeError::from_core(error);
        Self {
            code,
            message: native.message.clone(),
            native: Some(native),
        }
    }
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct NativeError {
    pub kind: NativeKind,
    pub context: Option<String>,
    pub os_code: Option<i32>,
    pub message: String,
}
impl NativeError {
    pub fn from_core(error: &logi_vhid::Error) -> Self {
        let (kind, context, os_code) = match error {
            logi_vhid::Error::Io { context, source } => (
                NativeKind::Io,
                Some((*context).into()),
                source.raw_os_error(),
            ),
            logi_vhid::Error::Protocol(_) => (NativeKind::Protocol, None, None),
            logi_vhid::Error::Unsupported(_) => (NativeKind::Unsupported, None, None),
            logi_vhid::Error::NotFound(_) => (NativeKind::NotFound, None, None),
            logi_vhid::Error::Cancelled => (NativeKind::Cancelled, None, None),
            logi_vhid::Error::TimedOut => (NativeKind::TimedOut, None, None),
            _ => (NativeKind::Protocol, None, None),
        };
        let message = if kind == NativeKind::Io && os_code == Some(0) {
            "API reported failure without a useful OS error code; cause unknown".into()
        } else {
            error.to_string()
        };
        Self {
            kind,
            context,
            os_code,
            message,
        }
    }
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct CleanupResult {
    pub keyboard: ReleaseState,
    pub mouse: ReleaseState,
    pub errors: Vec<NativeError>,
}
impl CleanupResult {
    pub fn not_opened() -> Self {
        Self {
            keyboard: ReleaseState::NotOpened,
            mouse: ReleaseState::NotOpened,
            errors: Vec::new(),
        }
    }
    pub fn pending(initialized: bool) -> Self {
        if initialized {
            Self {
                keyboard: ReleaseState::Pending,
                mouse: ReleaseState::Pending,
                errors: Vec::new(),
            }
        } else {
            Self::not_opened()
        }
    }
    pub fn has_failed(&self) -> bool {
        self.keyboard == ReleaseState::Failed || self.mouse == ReleaseState::Failed
    }
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct OperationOutcome {
    pub state: OperationState,
    pub submission: Submission,
    pub selected_backends: Vec<BackendKind>,
    pub cleanup: CleanupResult,
    pub cursor_after: Option<PointDto>,
    pub target_observed: bool,
}
impl OperationOutcome {
    pub fn rejected(backends: Vec<BackendKind>) -> Self {
        Self {
            state: OperationState::Rejected,
            submission: Submission::None,
            selected_backends: backends,
            cleanup: CleanupResult::not_opened(),
            cursor_after: None,
            target_observed: false,
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub(crate) struct PointDto {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Copy, Debug, Serialize, JsonSchema, PartialEq, Eq)]
pub(crate) struct RectDto {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct MonitorDto {
    pub handle: String,
    pub rect: RectDto,
    pub work_rect: RectDto,
    pub primary: bool,
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct WindowDto {
    pub hwnd: String,
    pub pid: u32,
    pub title: String,
    pub rect: RectDto,
    pub client_rect: RectDto,
    pub client_origin: PointDto,
    pub dpi: u32,
    pub session_id: u32,
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct DesktopInfo {
    pub coordinate_space: String,
    pub virtual_desktop: RectDto,
    pub monitors: Vec<MonitorDto>,
    pub cursor: PointDto,
    pub foreground: Option<WindowDto>,
    pub window: Option<WindowDto>,
    pub session_id: u32,
    pub desktop_name: String,
    pub interactive: bool,
    pub observed_at_unix_ms: u64,
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct DeviceInfoDto {
    pub control_path: String,
    pub keyboard_path: Option<String>,
    pub mouse_path: Option<String>,
    pub registered_pids: Vec<u64>,
    pub pid_gate_active: bool,
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct DriverStatus {
    pub state: DriverState,
    pub info: Option<DeviceInfoDto>,
    pub error: Option<NativeError>,
}
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub(crate) struct StatusData {
    pub allow_input: bool,
    pub worker_state: WorkerState,
    pub device_initialized: bool,
    pub driver: DriverStatus,
    pub last_operation: Option<ToolResponse<OperationOutcome>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn key(keys: &[&str]) -> InputRequest {
        InputRequest::Key(
            serde_json::from_value(json!({"target":{"hwnd":"0x1","pid":1},"keys":keys})).unwrap(),
        )
    }
    fn mouse(action: serde_json::Value) -> InputRequest {
        InputRequest::Mouse(
            serde_json::from_value(json!({"target":{"hwnd":"0x1","pid":1},"action":action}))
                .unwrap(),
        )
    }
    #[test]
    fn six_key_slots_are_separate_from_modifiers() {
        assert!(
            key(&["A", "B", "C", "D", "E", "F", "LEFT_CTRL", "RIGHT_SHIFT"])
                .validate()
                .is_ok()
        );
        assert!(key(&["A", "B", "C", "D", "E", "F", "G"])
            .validate()
            .is_err());
        assert!(key(&["A", "A"]).validate().is_err());
        assert!(key(&[]).validate().is_err());
    }
    #[test]
    fn rejects_invalid_target_and_unknown_fields() {
        for hwnd in ["0x0", "1", "0x", "0x10000000000000000", "-0x1"] {
            assert!(TargetWindow {
                hwnd: hwnd.into(),
                pid: 1
            }
            .validate()
            .is_err());
        }
        assert!(TargetWindow {
            hwnd: "0x1".into(),
            pid: 0
        }
        .validate()
        .is_err());
        assert!(serde_json::from_value::<KeyParams>(
            json!({"target":{"hwnd":"0x1","pid":1},"keys":["A"],"approved":true})
        )
        .is_err());
        assert!(serde_json::from_value::<MouseParams>(json!({"target":{"hwnd":"0x1","pid":1},"action":{"type":"scroll","delta":1,"unexpected":true}})).is_err());
        assert!(serde_json::from_value::<KeyName>(json!("a")).is_err());
    }
    #[test]
    fn text_limits_count_unicode_scalars_and_reject_nul() {
        let make = |text: String| {
            InputRequest::Text(TextParams {
                target: TargetWindow {
                    hwnd: "0x1".into(),
                    pid: 1,
                },
                text,
                mode: TextInputMode::Unicode,
                timeout_ms: None,
            })
        };
        assert!(make("😀".repeat(1024)).validate().is_ok());
        assert!(make("😀".repeat(1025)).validate().is_err());
        assert!(make("a\0b".into()).validate().is_err());
        assert!(make(String::new()).is_empty());
    }
    #[test]
    fn rejects_invalid_mouse_boundaries_without_clamping() {
        for action in [
            json!({"type":"move_relative","dx":0,"dy":0}),
            json!({"type":"scroll","delta":0}),
            json!({"type":"click","at":{"space":"screen","x":0,"y":0},"clicks":3}),
            json!({"type":"drag","from":{"space":"screen","x":0,"y":0},"to":{"space":"screen","x":1,"y":1},"hold_ms":0}),
        ] {
            assert!(mouse(action).validate().is_err());
        }
        assert!(
            mouse(json!({"type":"move_to","at":{"space":"screen","x":-1920,"y":-50}}))
                .validate()
                .is_ok()
        );
    }
    #[test]
    fn hold_and_timeout_boundaries_are_checked_before_execution() {
        for (hold, timeout, valid) in [
            (1, 1, true),
            (5000, 5000, true),
            (0, 10000, false),
            (5001, 10000, false),
            (20, 19, false),
            (20, 0, false),
            (20, 30001, false),
        ] {
            let mut request = key(&["A"]);
            let InputRequest::Key(params) = &mut request else {
                unreachable!()
            };
            params.hold_ms = Some(hold);
            params.timeout_ms = Some(timeout);
            assert_eq!(request.validate().is_ok(), valid);
        }
    }
    #[test]
    fn core_io_errors_keep_context_and_os_code() {
        let error = logi_vhid::Error::Io {
            context: "test operation",
            source: std::io::Error::from_raw_os_error(5),
        };
        let native = NativeError::from_core(&error);
        assert_eq!(native.kind, NativeKind::Io);
        assert_eq!(native.context.as_deref(), Some("test operation"));
        assert_eq!(native.os_code, Some(5));
    }
    #[test]
    fn failed_api_with_zero_last_error_is_not_reported_as_success() {
        let error = logi_vhid::Error::Io {
            context: "SendInput",
            source: std::io::Error::from_raw_os_error(0),
        };
        let native = NativeError::from_core(&error);
        assert_eq!(native.os_code, Some(0));
        assert!(native.message.contains("cause unknown"));
        assert_eq!(ToolFailure::from_core(&error).message, native.message);
    }
    #[test]
    fn mixed_mouse_paths_and_response_nulls_are_explicit_on_wire() {
        let request =
            mouse(json!({"type":"click","at":{"space":"screen","x":1,"y":2},"mode":"win32"}));
        assert_eq!(
            serde_json::to_value(request.selected_backends()).unwrap(),
            json!(["win32_sendinput_mouse", "ghub_hid_mouse"])
        );
        let outcome = OperationOutcome::rejected(request.selected_backends());
        let response = serde_json::to_value(ToolResponse::with_error(
            outcome,
            ToolFailure::new(ErrorCode::InputDisabled, "disabled"),
        ))
        .unwrap();
        assert_eq!(response["data"]["submission"], "none");
        assert_eq!(response["data"]["target_observed"], false);
        assert!(response["data"]["cursor_after"].is_null());
        assert!(response["error"]["native"].is_null());
        let success =
            serde_json::to_value(ToolResponse::success(CleanupResult::not_opened())).unwrap();
        assert!(success["error"].is_null());
    }
}
