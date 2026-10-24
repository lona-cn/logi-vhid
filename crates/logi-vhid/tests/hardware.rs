use std::thread::sleep;
use std::time::{Duration, Instant};

use logi_vhid::{
    hid, monitor_from_point, AbsoluteMode, Error, LogitechVirtualHid, OpenOptions, Result, Sequence,
};

fn wait_for(mut observed: impl FnMut() -> Result<bool>, description: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if observed()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::Protocol(format!(
                "did not observe {description} within one second"
            )));
        }
        sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "requires Logitech G HUB and sends real desktop input; run explicitly"]
fn hardware_smoke() {
    const VK_F24: i32 = 0x87;
    let mut device = LogitechVirtualHid::open_with_options(OpenOptions::both())
        .expect("open Logitech virtual HID bus");
    device
        .ensure_mouse_available()
        .expect("virtual mouse must not be disabled by a registered PID");
    assert!(
        !LogitechVirtualHid::is_key_down(VK_F24),
        "release F24 before running the hardware test"
    );
    let before = LogitechVirtualHid::cursor_pos().expect("read cursor");
    let monitor = monitor_from_point(before).expect("find cursor monitor");
    let dx = if monitor.rect.right - before.x > 64 {
        32
    } else {
        -32
    };

    let result = (|| -> Result<()> {
        device.mouse_move_relative(dx, 0)?;
        wait_for(
            || Ok(LogitechVirtualHid::cursor_pos()? != before),
            "cursor movement",
        )?;
        device.key_down(hid::F24)?;
        wait_for(
            || Ok(LogitechVirtualHid::is_key_down(VK_F24)),
            "F24 key down",
        )?;
        device.key_up(hid::F24)?;
        wait_for(
            || Ok(!LogitechVirtualHid::is_key_down(VK_F24)),
            "F24 key up",
        )?;
        Ok(())
    })();

    // Attempt both cleanups before reporting an input or observation failure.
    let released = device.release_all();
    let restored = Sequence::new()
        .move_absolute(before.x, before.y, AbsoluteMode::SetCursorPos)
        .run(&mut device)
        .and_then(|()| {
            wait_for(
                || Ok(LogitechVirtualHid::cursor_pos()? == before),
                "the original cursor position",
            )
        });
    result.expect("observe virtual HID input");
    released.expect("release test keys");
    restored.expect("restore cursor position");
}
