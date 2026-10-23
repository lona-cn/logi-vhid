use logi_vhid::{
    find_window_by_title, AbsoluteMode, Error, KeyboardLayout, LogitechVirtualHid, MouseButton,
    OpenOptions, Result, TextMode,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    validate_args(&args)?;
    let command = args.first().map(String::as_str).unwrap_or("help");
    match command {
        "info" => info(),
        "cursor" => cursor(),
        "type" => type_text(&args),
        "move" => move_relative(&args),
        "move-to" => move_absolute(&args),
        "click" => click(&args, false),
        "double-click" => click(&args, true),
        "key" => key(&args),
        "combo" => combo(&args),
        "window" => window_click(&args),
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        other => Err(Error::Unsupported(format!(
            "unknown command {other:?}; run with --help"
        ))),
    }
}

fn cursor() -> Result<()> {
    let point = LogitechVirtualHid::cursor_pos()?;
    println!("({},{})", point.x, point.y);
    Ok(())
}

fn info() -> Result<()> {
    let device = LogitechVirtualHid::open()?;
    let info = device.device_info()?;
    println!("control: {}", info.control_path);
    println!(
        "keyboard: {}",
        info.keyboard_path.as_deref().unwrap_or("<missing>")
    );
    println!(
        "mouse:    {}",
        info.mouse_path.as_deref().unwrap_or("<missing>")
    );
    println!("pids:     {:?}", info.registered_pids);
    println!("pid gate: {}", info.pid_gate_active());
    Ok(())
}

fn type_text(args: &[String]) -> Result<()> {
    let mut text = None;
    let mut index = 1usize;
    while index < args.len() {
        if args[index] == "--mode" {
            index += 2;
            continue;
        }
        if args[index].starts_with("--") {
            index += 1;
            continue;
        }
        text = Some(args[index].as_str());
        break;
    }
    let text = text.ok_or_else(|| Error::Unsupported("type needs a text argument".to_string()))?;
    let mode = match option(args, "--mode").unwrap_or("us") {
        "unicode" => TextMode::Win32Unicode,
        "us" => TextMode::Hid(KeyboardLayout::Us),
        "uk" => TextMode::Hid(KeyboardLayout::Uk),
        "de" => TextMode::Hid(KeyboardLayout::De),
        "fr" => TextMode::Hid(KeyboardLayout::Fr),
        other => {
            return Err(Error::Unsupported(format!(
                "unknown text mode {other:?}; use us, uk, de, fr, or unicode"
            )))
        }
    };
    let mut device = LogitechVirtualHid::open()?;
    device.type_text(text, mode)?;
    println!("typed: {text}");
    Ok(())
}

fn move_relative(args: &[String]) -> Result<()> {
    let dx = parse_i32(args.get(1), "dx")?;
    let dy = parse_i32(args.get(2), "dy")?;
    let mut device = LogitechVirtualHid::open_with_options(OpenOptions::mouse_only())?;
    device.mouse_move_relative(dx, dy)?;
    let point = LogitechVirtualHid::cursor_pos()?;
    println!("moved by ({dx},{dy}); cursor now ({},{})", point.x, point.y);
    Ok(())
}

fn move_absolute(args: &[String]) -> Result<()> {
    let x = parse_i32(args.get(1), "x")?;
    let y = parse_i32(args.get(2), "y")?;
    let mode = parse_mode(option(args, "--mode").unwrap_or("driver"))?;
    let mut device = LogitechVirtualHid::open_with_options(OpenOptions::mouse_only())?;
    device.set_cursor_pos(x, y, mode)?;
    let point = LogitechVirtualHid::cursor_pos()?;
    println!("cursor now ({},{})", point.x, point.y);
    Ok(())
}

fn click(args: &[String], double: bool) -> Result<()> {
    let x = parse_i32(args.get(1), "x")?;
    let y = parse_i32(args.get(2), "y")?;
    let button = parse_button(option(args, "--button").unwrap_or("left"))?;
    let mode = parse_mode(option(args, "--mode").unwrap_or("driver"))?;
    let mut device = LogitechVirtualHid::open_with_options(OpenOptions::mouse_only())?;
    if double {
        device.double_click_at(x, y, button, mode)?;
        println!("double-click {button:?} at ({x},{y})");
    } else {
        device.click_at(x, y, button, mode)?;
        println!("click {button:?} at ({x},{y})");
    }
    Ok(())
}

fn key(args: &[String]) -> Result<()> {
    let usage = parse_u8(args.get(1), "usage")?;
    let mut device = LogitechVirtualHid::open_with_options(OpenOptions::keyboard_only())?;
    device.press(usage)?;
    println!("pressed usage 0x{usage:02X}");
    Ok(())
}

fn combo(args: &[String]) -> Result<()> {
    let text = args
        .get(1)
        .ok_or_else(|| Error::Unsupported("combo needs comma-separated usages".to_string()))?;
    let mut usages = Vec::new();
    for part in text.split(',') {
        usages.push(parse_u8(Some(&part.to_string()), "usage")?);
    }
    let mut device = LogitechVirtualHid::open_with_options(OpenOptions::keyboard_only())?;
    device.press_combo(&usages)?;
    println!("pressed combo {usages:?}");
    Ok(())
}

fn window_click(args: &[String]) -> Result<()> {
    let title = args
        .get(1)
        .ok_or_else(|| Error::Unsupported("window needs a title substring".to_string()))?;
    let x = parse_i32(args.get(2), "x")?;
    let y = parse_i32(args.get(3), "y")?;
    let button = parse_button(option(args, "--button").unwrap_or("left"))?;
    let mode = parse_mode(option(args, "--mode").unwrap_or("driver"))?;
    let window = find_window_by_title(title)?;
    let mut device = LogitechVirtualHid::open_with_options(OpenOptions::mouse_only())?;
    device.click_window_relative(window, x, y, button, mode)?;
    println!("clicked window {window:#x} at client ({x},{y})");
    Ok(())
}

fn validate_args(args: &[String]) -> Result<()> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    let (required, allow_mode, allow_button) = match command {
        "help" | "--help" | "-h" | "info" | "cursor" => (0, false, false),
        "type" => (1, true, false),
        "move" => (2, false, false),
        "move-to" => (2, true, false),
        "click" | "double-click" => (2, true, true),
        "key" | "combo" => (1, false, false),
        "window" => (3, true, true),
        other => {
            return Err(Error::Unsupported(format!(
                "unknown command {other:?}; run with --help"
            )))
        }
    };
    let mut positional = 0;
    let mut mode_seen = false;
    let mut button_seen = false;
    let mut index = 1;
    while let Some(arg) = args.get(index) {
        if arg.starts_with("--") {
            let seen = match arg.as_str() {
                "--mode" if allow_mode => &mut mode_seen,
                "--button" if allow_button => &mut button_seen,
                _ => {
                    return Err(Error::Unsupported(format!(
                        "unknown option {arg:?} for {command}"
                    )))
                }
            };
            if *seen {
                return Err(Error::Unsupported(format!("duplicate option {arg:?}")));
            }
            if command != "type" && positional != required {
                return Err(Error::Unsupported(
                    "put positional arguments before options".to_string(),
                ));
            }
            let value = args
                .get(index + 1)
                .filter(|value| !value.starts_with("--"))
                .ok_or_else(|| Error::Unsupported(format!("missing value for {arg}")))?;
            if value.is_empty() {
                return Err(Error::Unsupported(format!("missing value for {arg}")));
            }
            *seen = true;
            index += 2;
        } else {
            positional += 1;
            if positional > required {
                return Err(Error::Unsupported(format!(
                    "unexpected argument {arg:?} for {command}"
                )));
            }
            index += 1;
        }
    }
    if positional != required {
        return Err(Error::Unsupported(format!(
            "{command} needs {required} positional argument(s); run with --help"
        )));
    }
    Ok(())
}

fn option<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}

fn parse_i32(value: Option<&String>, name: &str) -> Result<i32> {
    value
        .ok_or_else(|| Error::Unsupported(format!("missing {name}")))?
        .parse()
        .map_err(|_| Error::Unsupported(format!("invalid {name}")))
}

fn parse_u8(value: Option<&String>, name: &str) -> Result<u8> {
    let text = value.ok_or_else(|| Error::Unsupported(format!("missing {name}")))?;
    let (radix, digits) = if let Some(hex) = text.strip_prefix("0x") {
        (16, hex)
    } else {
        (10, text.as_str())
    };
    u8::from_str_radix(digits, radix)
        .map_err(|_| Error::Unsupported(format!("invalid {name}: {text:?}")))
}

fn parse_button(value: &str) -> Result<MouseButton> {
    match value {
        "left" => Ok(MouseButton::Left),
        "right" => Ok(MouseButton::Right),
        "middle" => Ok(MouseButton::Middle),
        "back" => Ok(MouseButton::Back),
        "forward" => Ok(MouseButton::Forward),
        other => Err(Error::Unsupported(format!(
            "unknown mouse button {other:?}"
        ))),
    }
}

fn parse_mode(value: &str) -> Result<AbsoluteMode> {
    match value {
        "driver" => Ok(AbsoluteMode::Driver),
        "win32" => Ok(AbsoluteMode::Win32),
        "set" | "setcursorpos" => Ok(AbsoluteMode::SetCursorPos),
        other => Err(Error::Unsupported(format!(
            "unknown absolute mode {other:?}; use driver, win32, or set"
        ))),
    }
}

fn print_help() {
    println!(
        "logi-vhid-demo <command>

Commands:
  info                         Show control/HID paths and PID gate state
  cursor                       Print the current cursor position
  type <text> [--mode MODE]    Type text (MODE: us, uk, de, fr, unicode)
  move <dx> <dy>               Relative mouse movement
  move-to <x> <y> [--mode M]   Absolute cursor movement
  click <x> <y> [--button B] [--mode M]
  double-click <x> <y> [--button B] [--mode M]
  key <usage>                  Press a HID usage (decimal or 0x hex)
  combo <u1,u2,...>            Press a chord and release in reverse
  window <title> <x> <y> [--button B] [--mode M]
                               Click a client-relative point in a window

Absolute modes: driver (default), win32, set
Mouse buttons: left, right, middle, back, forward
"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_arguments_before_opening_a_device() {
        for case in [
            vec!["move-to", "100", "100", "--mode"],
            vec!["click", "100", "100", "--buton", "right"],
            vec!["click", "100", "100", "--button", "--mode", "set"],
            vec!["move", "1", "2", "3"],
            vec!["type", "hello", "--mode", "us", "--mode", "unicode"],
            vec!["info", "--mode", "set"],
            vec!["cursor", "unexpected"],
        ] {
            let args: Vec<String> = case.iter().map(|arg| (*arg).to_string()).collect();
            assert!(validate_args(&args).is_err(), "accepted {case:?}");
        }
    }

    #[test]
    fn accepts_documented_options_and_negative_coordinates() {
        for case in [
            vec!["move", "-10", "-5"],
            vec!["click", "-100", "100", "--button", "right", "--mode", "set"],
            vec!["type", "--mode", "unicode", "中文"],
        ] {
            let args: Vec<String> = case.iter().map(|arg| (*arg).to_string()).collect();
            assert!(validate_args(&args).is_ok(), "rejected {case:?}");
        }
    }
}
