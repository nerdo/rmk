//! The keyboard's toggles as anyone can read them: whether Caps Word is on,
//! and whether USB NKRO is. For firmware that shows them, such as on a
//! display.

use core::cell::Cell;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Instant;

use crate::RawMutex;

/// The toggles, as the keyboard last left them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct KeyboardStatus {
    /// Caps Word is on and has not timed out.
    pub caps_word: bool,
    /// USB NKRO is on (always `false` without the `usb_nkro` feature).
    pub nkro: bool,
}

/// The toggles now.
pub fn keyboard_status() -> KeyboardStatus {
    KeyboardStatus {
        caps_word: CAPS_WORD_UNTIL.lock(|until| until.get().is_some_and(|until| Instant::now() < until)),
        nkro: NKRO.load(Ordering::Relaxed),
    }
}

/// When Caps Word times out unless another key keeps it on; `None` while it
/// is off. A deadline rather than a flag, since it times out without an
/// event.
static CAPS_WORD_UNTIL: Mutex<RawMutex, Cell<Option<Instant>>> = Mutex::new(Cell::new(None));

static NKRO: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_caps_word_until(until: Option<Instant>) {
    CAPS_WORD_UNTIL.lock(|cell| cell.set(until));
}

pub(crate) fn set_nkro(on: bool) {
    NKRO.store(on, Ordering::Relaxed);
}
