# Firmware data

RMK saves its own settings, such as the keymap, the Bluetooth bonds and the
USB NKRO switch, and restores them at each start. Firmware that adds settings
of its own, such as its lighting, needs the same.

Firmware data is a small value, owned by your firmware, that RMK saves in its
storage and keeps through restarts. It needs the `storage` feature.

## How it works

- `rmk::storage::save_firmware_data` saves a `FirmwareData`: 16 bytes whose
  meaning is yours. Each save replaces the one before.
- `rmk::storage::read_firmware_data` returns the value saved last, or `None`
  when none was ever saved.

Both wait on RMK's storage task, which starts with the rest of RMK. Call them
from a task that runs beside RMK's, such as a processor registered with
`#[register_processor(run)]`. A processor's constructor runs before RMK
starts, so a read there would wait forever.

Sixteen bytes is the size of the [split state](split_state.md), so one value
can be both shared with the peripherals and saved.

## Example

```rust
use rmk::storage::{FirmwareData, read_firmware_data, save_firmware_data};

// At the start of the processor's run:
let settings = read_firmware_data().await.map_or(MySettings::default(), MySettings::from_bytes);

// Whenever a setting changes:
save_firmware_data(FirmwareData(settings.to_bytes())).await;
```

The saved value is erased with the rest of RMK's storage, for example when
the firmware's RMK features change.
