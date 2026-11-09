# N-key rollover (USB)

A standard keyboard report carries six keys at once, plus the modifiers. With
the `usb_nkro` feature, the USB keyboard interface reports every held key
instead: a bitmap of the Keyboard/Keypad usages 0x04 to 0xDD.

```toml title="Cargo.toml"
rmk = { version = "0.9", features = ["usb_nkro", "..."] }
```

## How it works

- **USB, report protocol** (what an operating system uses): every held key.
- **USB, boot protocol** (what a BIOS or boot menu asks for): the standard
  six-key report. The keyboard switches when the host asks and returns to
  report protocol after a USB reset, as the HID specification requires.
- **Bluetooth and dongles**: the standard six-key report, so every key,
  including F13 to F24, keeps working there.

Without NKRO, a seventh key held with six others is still never lost: it is
sent as soon as one of the six is released.

## Switching it

NKRO is on at start. Bind `NkroToggle` to switch it on and off while the
keyboard runs; Vial shows the key as `NK_TOGG`. With the `storage` feature the
keyboard saves each switch and starts the next time the way it was left.

```toml title="keyboard.toml"
[[keymap.layer]]
keys = """
NkroToggle ...
"""
```

While it is off, the USB keyboard interface keeps its NKRO layout but reports
at most six keys.
