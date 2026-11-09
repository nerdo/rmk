//! Firmware data: a few bytes the firmware keeps in RMK's storage across
//! restarts, opaque to RMK, as the split state is. A firmware with settings
//! of its own, such as its lighting, saves them here. See
//! `docs/features/firmware_data.md`.

use postcard::experimental::max_size::MaxSize;
use serde::{Deserialize, Serialize};

/// Bytes in [`FirmwareData`], as many as the split state carries, so one
/// firmware value can be both shared and saved.
pub const FIRMWARE_DATA_LEN: usize = 16;

/// Firmware-owned bytes, saved as they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, MaxSize)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct FirmwareData(pub [u8; FIRMWARE_DATA_LEN]);

/// Save `data`, replacing what was saved before. Returns once the storage
/// task has it queued; call it from a task running beside RMK's.
pub async fn save_firmware_data(data: FirmwareData) {
    super::queue_firmware_data(data).await;
}

/// The data saved last, kept through restarts; `None` when none was ever
/// saved. Waits on the storage task, so call it from a task running beside
/// RMK's, never before RMK starts.
pub async fn read_firmware_data() -> Option<FirmwareData> {
    super::read_saved_firmware_data().await
}

// Spec: docs/features/firmware_data.md (this commit).
#[cfg(test)]
mod tests {
    use embassy_embedded_hal::adapter::BlockingAsync;
    use embassy_futures::select::{Either, select};

    use super::{FIRMWARE_DATA_LEN, FirmwareData, read_firmware_data, save_firmware_data};
    use crate::config::{BehaviorConfig, PositionalConfig, StorageConfig};
    use crate::core_traits::Runnable;
    use crate::keymap::KeymapData;
    use crate::simulator::flash::InMemoryFlash;
    use crate::test_support::test_block_on;
    use crate::types::action::KeyAction;

    const LIGHTS: FirmwareData = FirmwareData([7; FIRMWARE_DATA_LEN]);
    const DIMMER: FirmwareData = FirmwareData([3; FIRMWARE_DATA_LEN]);

    /// Boot a keyboard over `flash` and run `work` beside its storage task:
    /// one power-on, until `work` ends.
    async fn boot<T>(flash: &InMemoryFlash, work: impl Future<Output = T>) -> T {
        let data = Box::leak(Box::new(KeymapData::new([[[KeyAction::No]]])));
        let (_keymap, mut storage) = crate::initialize_keymap_and_storage(
            data,
            BlockingAsync::new(flash.clone()),
            &StorageConfig::default(),
            Box::leak(Box::new(BehaviorConfig::default())),
            Box::leak(Box::new(PositionalConfig::<1, 1>::default())),
        )
        .await;
        match select(storage.run(), work).await {
            Either::First(_) => unreachable!("the storage task never ends"),
            Either::Second(out) => out,
        }
    }

    // "Data saved before a restart reads back after it."
    #[test]
    fn saved_data_reads_back_after_a_restart() {
        test_block_on(async {
            let flash = InMemoryFlash::new();
            boot(&flash, async {
                save_firmware_data(LIGHTS).await;
                // A read queues behind the save, so the save is done here.
                read_firmware_data().await;
            })
            .await;
            assert_eq!(boot(&flash, read_firmware_data()).await, Some(LIGHTS));
        });
    }

    // "The last data saved is what reads back."
    #[test]
    fn the_last_save_is_what_reads_back() {
        test_block_on(async {
            let flash = InMemoryFlash::new();
            let read = boot(&flash, async {
                save_firmware_data(LIGHTS).await;
                save_firmware_data(DIMMER).await;
                read_firmware_data().await
            })
            .await;
            assert_eq!(read, Some(DIMMER));
        });
    }

    // "A keyboard run with no storage task, as a scenario without flash runs,
    // finds no data rather than waiting for a reply."
    #[test]
    fn with_no_storage_task_a_read_finds_nothing() {
        test_block_on(async {
            let read = select(crate::channel::drain_flash_channel_for_test(), read_firmware_data()).await;
            assert!(matches!(read, Either::Second(None)));
        });
    }

    // "With nothing ever saved, there is no data."
    #[test]
    fn nothing_saved_reads_as_none() {
        test_block_on(async {
            let flash = InMemoryFlash::new();
            assert_eq!(boot(&flash, read_firmware_data()).await, None);
        });
    }
}
