// SPDX-License-Identifier: Apache-2.0
//! Native predecessor cleanup after physical drain and strict attach. No RPC
//! exposes this method; active service history remains until a later owner.
use super::*;

impl ServiceRegistry {
    /// The native caller first proves ALL predecessor resources drained and
    /// excludes retained native worker/WDO owners. This is only the durable
    /// exact-row half of that proof, additionally guarded by the actual lease.
    pub fn prune_drained_shared_helpers(
        &mut self,
        lease: &crate::service::ServiceLease,
        current: Incarnation,
        exact: &[SharedHelperDrain],
    ) -> Result<usize> {
        ensure!(
            lease.root() == self.root,
            "shared helper cleanup ownership differs"
        );
        ensure!(
            exact.len() <= MAX_PROVIDER_DRAINS,
            "shared helper cleanup bound exceeded"
        );
        let mut identities = std::collections::BTreeSet::new();
        for record in exact {
            record.bytes()?;
            ensure!(
                record.namespace == self.incarnation()?
                    && record.owner.service != current
                    && record.phase == SharedHelperPhase::Drained
                    && identities.insert(record.owner),
                "shared helper cleanup includes nonterminal/current/duplicate ownership"
            );
        }
        // Borrow the same connection without starting a second transaction.
        // Canonical pending-fence validation below is query-only and observes
        // this immediate transaction, including full intent identity checks.
        let transaction =
            rusqlite::Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)?;
        require_shared_coverage(self)?;
        ensure!(
            self.recorded_service_instance()? == Some(current),
            "shared helper cleanup service changed"
        );
        ensure!(
            self.catalog_pending_fence()?.is_none(),
            "shared helper cleanup retains pending worker obligations"
        );
        for record in exact {
            ensure!(
                self.shared_helper_drain(record.owner)?.as_ref() == Some(record),
                "shared helper cleanup ownership changed"
            );
            let bytes = record.bytes()?;
            let removed = transaction.execute(
                "DELETE FROM service_provider_drains WHERE id=?1 AND payload=?2 AND digest=?3",
                params![record.id.to_string(), bytes, digest(&bytes)],
            )?;
            ensure!(removed == 1, "shared helper cleanup ownership changed");
        }
        transaction.commit()?;
        Ok(exact.len())
    }
}
