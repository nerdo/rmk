//! USB NKRO across a restart: the toggle's choice is stored, and a keyboard
//! built again over the same flash starts with it.
//!
//! Spec: the user's choice (2026-11-08), "yes it should remember": a switch
//! turned off stays off after a reboot; NKRO is on when nothing was stored.

use rmk::hid::nkro::{NkroKeyboardReport, NkroReport};
use rmk::hid::{KeyboardReport, Report};
use rmk::simulator::SimKeyboard;
use rmk::simulator::flash::InMemoryFlash;
use rmk::test_support::test_block_on;
use rmk::types::keycode::HidKeyCode;
use rmk::{k, kbctrl};

/// The NKRO toggle at (0,0) and A at (0,1).
fn keyboard() -> [[[rmk::types::action::KeyAction; 2]; 1]; 1] {
    [[[kbctrl!(NkroToggle), k!(A)]]]
}

fn six_key(keycodes: &[HidKeyCode]) -> Report {
    let mut report = KeyboardReport::default();
    for (slot, keycode) in report.keycodes.iter_mut().zip(keycodes) {
        *slot = *keycode as u8;
    }
    Report::KeyboardReport(report)
}

fn nkro(keycodes: &[HidKeyCode]) -> Report {
    let Report::KeyboardReport(six_key) = six_key(keycodes) else {
        unreachable!()
    };
    Report::NkroKeyboardReport(NkroReport {
        bitmap: NkroKeyboardReport::from_usages(0, keycodes.iter().map(|k| *k as u8)),
        six_key,
    })
}

/// Boot once over `flash`, tap the NKRO toggle `taps` times, and wait for
/// storage to settle: the session before the restart.
async fn boot_and_toggle(flash: &InMemoryFlash, taps: usize) {
    let mut keyboard = SimKeyboard::builder(keyboard()).build_with_flash(flash.clone()).await;
    for _ in 0..taps {
        keyboard.tap(0, 0, 20).delay(20);
    }
    keyboard.wait_storage().run().await;
}

// "A switch turned off stays off after a restart: A arrives in the six-key report."
#[test]
fn nkro_switched_off_stays_off_after_a_restart() {
    test_block_on(async {
        let flash = InMemoryFlash::new();
        boot_and_toggle(&flash, 1).await;
        let mut keyboard = SimKeyboard::builder(keyboard()).build_with_flash(flash).await;
        keyboard
            .press(0, 1)
            .expect_exact_report(six_key(&[HidKeyCode::A]))
            .release(0, 1)
            .expect_exact_report(six_key(&[]))
            .run()
            .await;
    });
}

// "A switch turned off and on again is on after a restart: A arrives in the bitmap."
#[test]
fn nkro_switched_off_and_on_again_is_on_after_a_restart() {
    test_block_on(async {
        let flash = InMemoryFlash::new();
        boot_and_toggle(&flash, 2).await;
        let mut keyboard = SimKeyboard::builder(keyboard()).build_with_flash(flash).await;
        keyboard
            .press(0, 1)
            .expect_exact_report(nkro(&[HidKeyCode::A]))
            .release(0, 1)
            .expect_exact_report(nkro(&[]))
            .run()
            .await;
    });
}
