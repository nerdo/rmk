# Split state

A split keyboard's peripheral learns some of the central's state on its own:
the active layer, the lock LEDs, sleep, and the connection status. Firmware
that adds state of its own, such as whether underglow is on and which effect
it runs, or whether Caps Word is active, needs the peripheral to know it too,
so the peripheral's LEDs or display can follow.

The split state is one small value, owned by your firmware, that RMK keeps the
same on both halves.

## How it works

- The central calls `rmk::split::set_split_state` with a `SplitState`: 16 bytes
  whose meaning is yours. RMK keeps it and publishes it as a `SplitStateEvent`
  on the central.
- RMK sends the value to every connected peripheral, and again to each
  peripheral when its split connection starts, so a peripheral that connects
  late, or reconnects, still gets the current value.
- The peripheral publishes each value it receives as a `SplitStateEvent`.
- `rmk::split::current_split_state` returns the latest value on either half,
  for code that starts after it was set.

It is state, not a stream of messages: only the latest value matters, and a
newer value replaces an older one that was not delivered yet. That keeps it in
line with how RMK syncs the layer, and costs no queues. The 16 bytes fit
inside the largest existing split message, so key traffic does not grow.

## Example

Encode your state into the 16 bytes on the central:

```rust
use rmk::split::{SplitState, set_split_state};

let mut state = SplitState::default();
state.0[0] = underglow_on as u8;
state.0[1] = effect_index;
set_split_state(state);
```

React to it on either half with a processor:

```rust
use rmk::event::SplitStateEvent;
use rmk::macros::processor;

#[processor(subscribe = [SplitStateEvent])]
pub struct Underglow { /* ... */ }

impl Underglow {
    async fn on_split_state_event(&mut self, event: SplitStateEvent) {
        let on = event.0.0[0] != 0;
        // ...
    }
}
```

Each subscriber counts against the event's `subs`. The default leaves room
for one of your processors; for more, raise `[event.split_state].subs` in
`keyboard.toml` to their number (see [Event configuration](../configuration/event)).
RMK adds its own subscribers on top.

The split state is available with the `split` feature.
