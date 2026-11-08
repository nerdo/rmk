//! The firmware's split state: one small value the central keeps the same on
//! its peripherals, as RMK does with the layer. See
//! `docs/features/split_state.md`.

use core::cell::Cell;

use embassy_sync::blocking_mutex::Mutex;
use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};

use crate::RawMutex;
use crate::event::{SplitStateEvent, publish_event};

/// Bytes in a [`SplitState`]. Sixteen, plus the message tag, is as large as the
/// largest existing split message, so carrying it never pads key traffic.
pub const SPLIT_STATE_LEN: usize = 16;

/// Firmware-owned state, opaque to RMK.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, MaxSize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct SplitState(pub [u8; SPLIT_STATE_LEN]);

/// Set the split state on the central: publish it here, and send it to every
/// peripheral now and whenever its split connection starts.
pub fn set_split_state(state: SplitState) {
    update(state);
}

/// Keep `state` as this half's latest and publish it. The central does this
/// on [`set_split_state`]; a peripheral on each state it receives.
pub(crate) fn update(state: SplitState) {
    LATEST.lock(|latest| latest.set(Some(state)));
    publish_event(SplitStateEvent(state));
}

/// The latest split state on this half: set here on the central, or received
/// on a peripheral. `None` until there is one.
pub fn current_split_state() -> Option<SplitState> {
    LATEST.lock(|latest| latest.get())
}

/// This half's latest split state, for a session that starts after it was set.
static LATEST: Mutex<RawMutex, Cell<Option<SplitState>>> = Mutex::new(Cell::new(None));

// Spec: docs/features/split_state.md, "How it works" (this commit).
#[cfg(test)]
mod tests {
    use core::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::vec::Vec;

    use embassy_futures::select::{Either, select};
    use embassy_time::Timer;

    use super::{SPLIT_STATE_LEN, SplitState, current_split_state, set_split_state};
    use crate::event::{EventSubscriber, SplitStateEvent, SubscribableEvent};
    use crate::split::driver::{PeripheralManager, SplitDriverError, SplitReader, SplitWriter};
    use crate::split::peripheral::SplitPeripheral;
    use crate::split::{PeripheralMatrixConfig, SplitMessage};
    use crate::test_support::test_block_on;

    const RED: SplitState = SplitState([1; SPLIT_STATE_LEN]);
    const BLUE: SplitState = SplitState([2; SPLIT_STATE_LEN]);

    /// One end of a split link: replays `incoming`, then waits forever, and
    /// records every message written.
    struct FakeLink {
        incoming: VecDeque<SplitMessage>,
        sent: Rc<RefCell<Vec<SplitMessage>>>,
    }

    impl SplitReader for FakeLink {
        async fn read(&mut self) -> Result<SplitMessage, SplitDriverError> {
            match self.incoming.pop_front() {
                Some(message) => Ok(message),
                None => core::future::pending().await,
            }
        }
    }

    impl SplitWriter for FakeLink {
        async fn write(&mut self, message: &SplitMessage) -> Result<usize, SplitDriverError> {
            self.sent.borrow_mut().push(*message);
            Ok(0)
        }
    }

    fn link(incoming: Vec<SplitMessage>) -> (FakeLink, Rc<RefCell<Vec<SplitMessage>>>) {
        let sent = Rc::new(RefCell::new(Vec::new()));
        (
            FakeLink {
                incoming: incoming.into(),
                sent: sent.clone(),
            },
            sent,
        )
    }

    fn central_session(link: FakeLink) -> impl core::future::Future<Output = ()> {
        let config = PeripheralMatrixConfig {
            rows: 1,
            cols: 1,
            row_offset: 0,
            col_offset: 0,
        };
        PeripheralManager::new(
            link,
            0,
            config,
            #[cfg(feature = "dfu_split")]
            crate::split::driver::UpdatePolicy::MatchHash,
        )
        .run()
    }

    fn states_sent(sent: &Rc<RefCell<Vec<SplitMessage>>>) -> Vec<SplitState> {
        let sent = sent.borrow();
        let states = sent.iter().filter_map(|m| match m {
            SplitMessage::State(state) => Some(*state),
            _ => None,
        });
        states.collect()
    }

    // "RMK keeps it and publishes it as a SplitStateEvent on the central."
    #[test]
    fn setting_the_split_state_publishes_it_on_the_central() {
        test_block_on(async {
            let mut sub = SplitStateEvent::subscriber();
            set_split_state(RED);
            assert_eq!(sub.next_event().await, SplitStateEvent(RED));
        });
    }

    // "current_split_state returns the latest value on either half."
    #[test]
    fn the_latest_set_split_state_is_current_on_the_central() {
        set_split_state(RED);
        set_split_state(BLUE);
        assert_eq!(current_split_state(), Some(BLUE));
    }

    // "...and again to each peripheral when its split connection starts."
    #[test]
    fn a_split_session_starts_by_sending_the_current_state() {
        set_split_state(RED);
        let (link, sent) = link(Vec::new());
        test_block_on(async {
            let session = central_session(link);
            select(session, Timer::after_millis(50)).await;
        });
        assert_eq!(states_sent(&sent), [RED]);
    }

    // "RMK sends the value to every connected peripheral."
    #[test]
    fn a_split_state_set_during_a_session_is_sent() {
        let (link, sent) = link(Vec::new());
        test_block_on(async {
            let session = central_session(link);
            let later = async {
                Timer::after_millis(10).await;
                set_split_state(BLUE);
                Timer::after_millis(40).await;
            };
            select(session, later).await;
        });
        assert_eq!(states_sent(&sent), [BLUE]);
    }

    // "The peripheral publishes each value it receives as a SplitStateEvent."
    #[test]
    fn a_received_split_state_is_published_on_the_peripheral() {
        let (link, _) = link(vec![SplitMessage::State(RED)]);
        let received = test_block_on(async {
            let mut sub = SplitStateEvent::subscriber();
            let mut peripheral = SplitPeripheral::new(link);
            match select(peripheral.run(), sub.next_event()).await {
                Either::First(()) => None,
                Either::Second(event) => Some(event),
            }
        });
        assert_eq!(received, Some(SplitStateEvent(RED)));
    }

    // "current_split_state returns the latest value on either half."
    #[test]
    fn a_received_split_state_is_current_on_the_peripheral() {
        let (link, _) = link(vec![SplitMessage::State(BLUE)]);
        test_block_on(async {
            let mut peripheral = SplitPeripheral::new(link);
            select(peripheral.run(), Timer::after_millis(50)).await;
        });
        assert_eq!(current_split_state(), Some(BLUE));
    }
}
