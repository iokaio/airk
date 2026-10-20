// SPDX-License-Identifier: Apache-2.0
//! Explicit producer and OCR writes share the current coordinator.
use crate::Kernel;
use airk_core::{Snapshot, changes::PublicationReason};
use anyhow::Result;
impl Kernel {
    pub(crate) fn publish_v2(
        &self,
        writer: &airk_store::WriterLock,
        snapshot: &Snapshot,
        reservation: airk_store::quota::Reservation,
        _reason: PublicationReason,
    ) -> Result<u64> {
        self.publish_current_projection(writer, snapshot, &reservation)
    }
}
