// SPDX-License-Identifier: Apache-2.0
//! Version-two shared-helper records in the existing provider ownership table.
//! Version-one worker records and their canonical serialized bytes are unchanged.
use super::*;
use anyhow::Context;
#[path = "service_provider_shared_gc.rs"]
mod gc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedHelperIdentity {
    pub service: Incarnation,
    pub helper: Incarnation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedHelperPhase {
    Admitting,
    Bound,
    Drained,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedHelperRole {
    SharedEmbeddingHelper,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub enum SharedHelperContainment {
    Unix {
        guardian_pid: Option<u32>,
        group: Option<u32>,
        pin_device: u64,
        pin_inode: u64,
    },
    Windows {
        job_name: String,
        user_sid: String,
        session: u32,
        created: bool,
        leader_pid: Option<u32>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedHelperDrain {
    pub version: u32,
    pub id: Incarnation,
    pub namespace: Incarnation,
    pub role: SharedHelperRole,
    pub owner: SharedHelperIdentity,
    pub boot: ProviderBoot,
    pub containment: SharedHelperContainment,
    pub phase: SharedHelperPhase,
}

/// This is a typed, complete observation, not a worker identity synthesized for
/// a global helper. Readers select a role explicitly or retain both variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProviderOwnership {
    Worker(ProviderDrain),
    SharedHelper(SharedHelperDrain),
}

pub fn shared_helper_job_name(
    namespace: Incarnation,
    owner: SharedHelperIdentity,
    user_sid: &str,
) -> Result<String> {
    ensure!(
        user_sid.starts_with("S-1-")
            && user_sid.len() <= 256
            && user_sid[4..]
                .split('-')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())),
        "invalid native helper user SID"
    );
    let user = digest(user_sid.as_bytes());
    Ok(format!(
        "Global\\AIRK-provider-v2-{namespace}-{}-{}-{}",
        owner.service,
        owner.helper,
        &user[..16]
    ))
}

fn pid_valid(value: u32) -> bool {
    value > 1 && value <= i32::MAX as u32
}

impl SharedHelperDrain {
    pub(super) fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.version == 2 && self.id == self.owner.helper,
            "invalid shared helper ownership identity"
        );
        self.boot.validate()?;
        let bound = match &self.containment {
            SharedHelperContainment::Unix {
                guardian_pid,
                group,
                pin_inode,
                ..
            } => {
                ensure!(
                    guardian_pid.is_none_or(pid_valid)
                        && *pin_inode > 0
                        && group.is_none_or(pid_valid)
                        && (group.is_none() || guardian_pid.is_some()),
                    "invalid native shared guardian identity"
                );
                group.is_some()
            }
            SharedHelperContainment::Windows {
                job_name,
                user_sid,
                leader_pid,
                created,
                ..
            } => {
                ensure!(
                    *job_name == shared_helper_job_name(self.namespace, self.owner, user_sid)?
                        && leader_pid.is_none_or(pid_valid)
                        && (leader_pid.is_none() || *created),
                    "invalid native shared helper job identity"
                );
                leader_pid.is_some()
            }
        };
        ensure!(
            match self.phase {
                SharedHelperPhase::Admitting => !bound,
                SharedHelperPhase::Bound => bound,
                SharedHelperPhase::Drained => true,
            },
            "shared helper phase does not match native containment"
        );
        let bytes = serde_json::to_vec(self)?;
        ensure!(bytes.len() <= 4096, "shared helper metadata exceeds bound");
        Ok(bytes)
    }
}

fn require_shared_coverage(registry: &ServiceRegistry) -> Result<()> {
    let state = registry
        .catalog_state()?
        .context("shared helper ownership requires activated catalog authority")?;
    ensure!(
        state.activated && state.authority_contract == 1,
        "shared helper ownership requires actual authority-1 activation"
    );
    Ok(())
}

pub(super) fn decode_shared(
    id: String,
    bytes: Vec<u8>,
    hash: String,
    namespace: Incarnation,
) -> Result<SharedHelperDrain> {
    ensure!(
        bytes.len() <= 4096 && hash == digest(&bytes),
        "shared helper metadata digest mismatch"
    );
    let record: SharedHelperDrain = serde_json::from_slice(&bytes)?;
    ensure!(
        record.id.to_string() == id && record.namespace == namespace && record.bytes()? == bytes,
        "shared helper metadata ownership mismatch"
    );
    Ok(record)
}

impl ServiceRegistry {
    pub fn provider_ownerships(&self) -> Result<Vec<ProviderOwnership>> {
        let namespace = self.incarnation()?;
        let mut statement = self.connection.prepare("SELECT substr(id,1,33),substr(payload,1,4097),substr(digest,1,65) FROM service_provider_drains ORDER BY id LIMIT 129")?;
        let mut rows = statement.query([])?;
        let mut records = Vec::new();
        let mut shared_coverage_checked = false;
        #[derive(Deserialize)]
        struct Header {
            version: u32,
        }
        while let Some(row) = rows.next()? {
            ensure!(
                records.len() < MAX_PROVIDER_DRAINS,
                "provider drain metadata count exceeded"
            );
            let bytes: Vec<u8> = row.get(1)?;
            ensure!(bytes.len() <= 4096, "provider drain metadata exceeds bound");
            let header: Header = serde_json::from_slice(&bytes)?;
            records.push(match header.version {
                1 => ProviderOwnership::Worker(decode(row.get(0)?, bytes, row.get(2)?, namespace)?),
                2 => {
                    if !shared_coverage_checked {
                        require_shared_coverage(self)?;
                        shared_coverage_checked = true;
                    }
                    ProviderOwnership::SharedHelper(decode_shared(
                        row.get(0)?,
                        bytes,
                        row.get(2)?,
                        namespace,
                    )?)
                }
                _ => anyhow::bail!("unsupported provider ownership record version"),
            });
        }
        Ok(records)
    }

    /// Explicit worker-role selection for legacy guardian ownership. Native
    /// aggregate resource/recovery reads must use provider_ownerships instead.
    pub fn worker_provider_drains(&self) -> Result<Vec<ProviderDrain>> {
        Ok(self
            .provider_ownerships()?
            .into_iter()
            .filter_map(|record| match record {
                ProviderOwnership::Worker(record) => Some(record),
                ProviderOwnership::SharedHelper(_) => None,
            })
            .collect())
    }

    pub fn shared_helper_drains(&self) -> Result<Vec<SharedHelperDrain>> {
        require_shared_coverage(self)?;
        Ok(self
            .provider_ownerships()?
            .into_iter()
            .filter_map(|record| match record {
                ProviderOwnership::SharedHelper(record) => Some(record),
                ProviderOwnership::Worker(_) => None,
            })
            .collect())
    }

    pub fn shared_helper_drain(
        &self,
        owner: SharedHelperIdentity,
    ) -> Result<Option<SharedHelperDrain>> {
        require_shared_coverage(self)?;
        let raw: Option<(String,Vec<u8>,String)> = self.connection.query_row(
            "SELECT substr(id,1,33),substr(payload,1,4097),substr(digest,1,65) FROM service_provider_drains WHERE id=?1",
            [owner.helper.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        let Some((id, bytes, hash)) = raw else {
            return Ok(None);
        };
        let record = decode_shared(id, bytes, hash, self.incarnation()?)?;
        ensure!(
            record.owner == owner,
            "foreign shared helper service identity"
        );
        Ok(Some(record))
    }

    /// Native adapter validated the per-helper pin or compiled global job
    /// identity. Reserve BEFORE job creation or any guardian/helper process;
    /// successful job creation/limits require their own durable transition.
    pub fn begin_shared_helper_drain(
        &mut self,
        owner: SharedHelperIdentity,
        containment: SharedHelperContainment,
        boot: ProviderBoot,
    ) -> Result<SharedHelperDrain> {
        require_shared_coverage(self)?;
        if let SharedHelperContainment::Unix {
            guardian_pid,
            group,
            ..
        } = &containment
        {
            ensure!(
                guardian_pid.is_none() && group.is_none(),
                "shared guardian reservation must precede process creation"
            );
        }
        if let SharedHelperContainment::Windows {
            created,
            leader_pid,
            ..
        } = &containment
        {
            ensure!(
                !created && leader_pid.is_none(),
                "shared job reservation must precede native creation"
            );
        }
        ensure!(
            self.recorded_service_instance()? == Some(owner.service),
            "shared helper belongs to another native service instance"
        );
        let record = SharedHelperDrain {
            version: 2,
            id: owner.helper,
            namespace: self.incarnation()?,
            role: SharedHelperRole::SharedEmbeddingHelper,
            owner,
            boot,
            containment,
            phase: SharedHelperPhase::Admitting,
        };
        let bytes = record.bytes()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 =
            transaction.query_row("SELECT count(*) FROM service_provider_drains", [], |row| {
                row.get(0)
            })?;
        ensure!(
            (0..MAX_PROVIDER_DRAINS as i64).contains(&count),
            "provider drain admission limit reached"
        );
        transaction.execute(
            "INSERT INTO service_provider_drains VALUES(?1,?2,?3)",
            params![record.id.to_string(), bytes, digest(&bytes)],
        )?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn bind_shared_helper(
        &mut self,
        record: &SharedHelperDrain,
        native_id: u32,
    ) -> Result<SharedHelperDrain> {
        ensure!(
            record.phase == SharedHelperPhase::Admitting && pid_valid(native_id),
            "invalid shared helper binding transition"
        );
        let mut next = record.clone();
        match &mut next.containment {
            SharedHelperContainment::Unix { group, .. } => *group = Some(native_id),
            SharedHelperContainment::Windows { leader_pid, .. } => *leader_pid = Some(native_id),
        }
        next.phase = SharedHelperPhase::Bound;
        self.replace_shared_helper(record, &next)?;
        Ok(next)
    }

    /// Native caller owns a newly created, noncolliding job with the required
    /// limits and user ACL. Commit/reconcile this transition BEFORE atomic spawn.
    pub fn mark_shared_job_created(
        &mut self,
        record: &SharedHelperDrain,
    ) -> Result<SharedHelperDrain> {
        ensure!(
            record.phase == SharedHelperPhase::Admitting,
            "shared helper is already bound"
        );
        let mut next = record.clone();
        let SharedHelperContainment::Windows {
            created,
            leader_pid,
            ..
        } = &mut next.containment
        else {
            anyhow::bail!("shared job creation requires Windows containment");
        };
        ensure!(
            !*created && leader_pid.is_none(),
            "shared helper job is already created"
        );
        *created = true;
        self.replace_shared_helper(record, &next)?;
        Ok(next)
    }

    /// Parent binds its exact newly created guardian before sending Init. All
    /// startup actors retain the inherited pin while the group remains unbound.
    pub fn bind_shared_guardian(
        &mut self,
        record: &SharedHelperDrain,
        guardian_pid: u32,
    ) -> Result<SharedHelperDrain> {
        ensure!(
            record.phase == SharedHelperPhase::Admitting && pid_valid(guardian_pid),
            "invalid shared guardian binding transition"
        );
        let mut next = record.clone();
        let SharedHelperContainment::Unix {
            guardian_pid: old,
            group,
            ..
        } = &mut next.containment
        else {
            anyhow::bail!("shared guardian binding requires Unix containment");
        };
        ensure!(
            old.is_none() && group.is_none(),
            "shared guardian is already bound"
        );
        *old = Some(guardian_pid);
        self.replace_shared_helper(record, &next)?;
        Ok(next)
    }

    /// Only actual native group/job completion (or the guarded no-execution
    /// startup proof) allows this transition. Time, EOF and absence alone do not.
    /// Keep the terminal row for prepared receipts and current-service history.
    pub fn complete_shared_helper_drain(
        &mut self,
        record: &SharedHelperDrain,
    ) -> Result<SharedHelperDrain> {
        let mut next = record.clone();
        next.phase = SharedHelperPhase::Drained;
        self.replace_shared_helper(record, &next)?;
        Ok(next)
    }

    fn replace_shared_helper(
        &mut self,
        old: &SharedHelperDrain,
        next: &SharedHelperDrain,
    ) -> Result<()> {
        require_shared_coverage(self)?;
        ensure!(
            old.namespace == self.incarnation()?
                && old.owner == next.owner
                && old.id == next.id
                && old.namespace == next.namespace,
            "foreign shared helper transition"
        );
        let before = old.bytes()?;
        let after = next.bytes()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current: (Vec<u8>,String) = transaction.query_row("SELECT substr(payload,1,4097),substr(digest,1,65) FROM service_provider_drains WHERE id=?1",
            [old.id.to_string()],|row|Ok((row.get(0)?,row.get(1)?)))?;
        ensure!(
            current.1 == digest(&current.0) && (current.0 == before || current.0 == after),
            "shared helper ownership changed"
        );
        if current.0 != after {
            let changed = transaction.execute("UPDATE service_provider_drains SET payload=?1,digest=?2 WHERE id=?3 AND payload=?4 AND digest=?5",
                params![after,digest(&after),old.id.to_string(),before,digest(&before)])?;
            ensure!(changed == 1, "shared helper ownership changed");
        }
        transaction.commit()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "service_provider_shared_tests.rs"]
mod tests;
