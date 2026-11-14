// SPDX-License-Identifier: Apache-2.0
//! Private retained-edit record contract (§3.13), before writer consumers.
//! Validation checks claims and bindings, never filesystem ownership or authority.
use airk_core::identity::{Incarnation, Sha256Digest};
use serde::{Deserialize, Serialize};

pub(super) const ROOT_SCHEMA: &str = "airk.agent-config-edit-root/1";
pub(super) const RECEIPT_SCHEMA: &str = "airk.agent-config-edit/1";
pub(super) const MAX_RECEIPT_BYTES: usize = 64 * 1024;
pub(super) const MAX_INPUT_BYTES: u64 = 1024 * 1024;
pub(super) const MAX_TRANSACTIONS: u64 = 16;
pub(super) const MAX_ROOT_BYTES: u64 = 64 * 1024 * 1024;
pub(super) const RESERVATION_BYTES: u64 = 2 * MAX_INPUT_BYTES + 128 * 1024;

type Validation = Result<(), &'static str>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Owner {
    Unix { uid: u32 },
    Windows { sid: String },
}

impl Owner {
    fn validate(&self) -> Validation {
        if let Self::Windows { sid } = self {
            let parts: Vec<_> = sid.split('-').collect();
            let canonical = |s: &str| {
                !s.is_empty()
                    && (s == "0" || !s.starts_with('0'))
                    && s.bytes().all(|b| b.is_ascii_digit())
            };
            if !(4..=18).contains(&parts.len())
                || parts[0] != "S"
                || parts[1] != "1"
                || !parts[2..].iter().all(|s| canonical(s))
                || parts[2].parse::<u64>().map_or(true, |v| v >= 1 << 48)
                || parts[3..].iter().any(|s| s.parse::<u32>().is_err())
            {
                return Err("invalid owner SID");
            }
        }
        Ok(())
    }

    fn matches(&self, identity: &NativeIdentity) -> bool {
        matches!(
            (self, identity),
            (Self::Unix { .. }, NativeIdentity::Unix { .. })
                | (Self::Windows { .. }, NativeIdentity::Windows { .. })
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "platform", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum NativeIdentity {
    Unix {
        device: u64,
        inode: u64,
    },
    Windows {
        volume_serial: u64,
        file_id: Incarnation,
    },
}

impl NativeIdentity {
    fn same_volume(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unix { device: a, .. }, Self::Unix { device: b, .. }) => a == b,
            (
                Self::Windows {
                    volume_serial: a, ..
                },
                Self::Windows {
                    volume_serial: b, ..
                },
            ) => a == b,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RootMarker {
    pub schema: String,
    pub owner: Owner,
    pub root: NativeIdentity,
    pub parent: NativeIdentity,
}

impl RootMarker {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err("root marker exceeds bound".into());
        }
        let record: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        record.validate().map_err(str::to_owned)?;
        Ok(record)
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate().map_err(str::to_owned)?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err("root marker exceeds bound".into());
        }
        Ok(bytes)
    }

    pub fn validate(&self) -> Validation {
        if self.schema != ROOT_SCHEMA {
            return Err("unsupported root schema");
        }
        self.owner.validate()?;
        if !self.owner.matches(&self.root)
            || !self.owner.matches(&self.parent)
            || !self.root.same_volume(&self.parent)
            || self.root == self.parent
        {
            return Err("invalid root binding");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Snapshot {
    pub identity: NativeIdentity,
    pub length: u64,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Operation {
    Connect,
    Repair,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Primitive {
    ReplaceFile,
    RenameExchange,
    RenameSwap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum State {
    Prepared,
    PublicationStarted,
    PublishedRetained,
    ConflictRetained,
    OutcomeUnknown,
    RefusedRetained,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum IncompleteReason {
    Oversized,
    Changing,
    Unreadable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Observation {
    Absent,
    Complete {
        file: Snapshot,
    },
    Incomplete {
        identity: Option<NativeIdentity>,
        length: Option<u64>,
        observed_bytes: u64,
        reason: IncompleteReason,
    },
}

impl Observation {
    fn matches(&self, snapshot: &Snapshot) -> bool {
        matches!(self, Self::Complete { file } if file == snapshot)
    }

    fn validate(&self, root: &NativeIdentity) -> Validation {
        match self {
            Self::Absent => Ok(()),
            Self::Complete { file } => {
                if file.length > MAX_INPUT_BYTES
                    || !file.identity.same_volume(root)
                    || &file.identity == root
                {
                    return Err("invalid complete observation");
                }
                Ok(())
            }
            Self::Incomplete {
                identity,
                length,
                observed_bytes,
                reason,
            } => {
                if *observed_bytes > MAX_INPUT_BYTES
                    || identity
                        .as_ref()
                        .is_some_and(|id| !id.same_volume(root) || id == root)
                    || (*reason == IncompleteReason::Oversized
                        && length.is_none_or(|n| n <= MAX_INPUT_BYTES))
                {
                    return Err("invalid incomplete observation");
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct After {
    pub target: Observation,
    pub prepared: Observation,
    pub before: Observation,
    pub native_error: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Receipt {
    pub schema: String,
    pub transaction: Incarnation,
    pub owner: Owner,
    pub root: NativeIdentity,
    pub parent: NativeIdentity,
    pub target_basename: String,
    pub operation: Operation,
    pub original: Snapshot,
    pub prepared: Snapshot,
    pub primitive: Primitive,
    pub state: State,
    pub sequence: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<After>,
}

impl Receipt {
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err("receipt exceeds bound".into());
        }
        let record: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        record.validate().map_err(str::to_owned)?;
        Ok(record)
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate().map_err(str::to_owned)?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err("receipt exceeds bound".into());
        }
        Ok(bytes)
    }

    pub fn validate(&self) -> Validation {
        if self.schema != RECEIPT_SCHEMA {
            return Err("unsupported receipt schema");
        }
        RootMarker {
            schema: ROOT_SCHEMA.into(),
            owner: self.owner.clone(),
            root: self.root.clone(),
            parent: self.parent.clone(),
        }
        .validate()?;
        let basename = &self.target_basename;
        if basename.is_empty()
            || basename.len() > 255
            || matches!(basename.as_str(), "." | "..")
            || basename.ends_with(['.', ' '])
            || basename
                .chars()
                .any(|c| c.is_control() || "/\\:<>\"|?*".contains(c))
        {
            return Err("invalid target basename");
        }
        if matches!(self.owner, Owner::Windows { .. }) {
            let stem = basename
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || ["COM", "LPT"].iter().any(|prefix| {
                    stem.strip_prefix(prefix).is_some_and(|suffix| {
                        matches!(
                            suffix,
                            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                        )
                    })
                })
            {
                return Err("reserved target basename");
            }
        }
        if self.original.length > MAX_INPUT_BYTES
            || self.prepared.length > MAX_INPUT_BYTES
            || !self.original.identity.same_volume(&self.root)
            || !self.prepared.identity.same_volume(&self.root)
            || self.original.identity == self.prepared.identity
            || [&self.original.identity, &self.prepared.identity]
                .into_iter()
                .any(|id| id == &self.root || id == &self.parent)
            || (self.primitive == Primitive::ReplaceFile)
                != matches!(self.owner, Owner::Windows { .. })
        {
            return Err("invalid publication identity");
        }
        match self.state {
            State::Prepared if self.sequence == 0 && self.after.is_none() => return Ok(()),
            State::PublicationStarted if self.sequence == 1 && self.after.is_none() => {
                return Ok(());
            }
            State::Prepared | State::PublicationStarted => {
                return Err("invalid pre-publication state");
            }
            _ if self.sequence < 2 => return Err("invalid observed sequence"),
            _ => (),
        }
        let after = self
            .after
            .as_ref()
            .ok_or("missing post-publication observations")?;
        for observation in [&after.target, &after.prepared, &after.before] {
            observation.validate(&self.root)?;
        }
        let retained = if self.primitive == Primitive::ReplaceFile {
            &after.before
        } else {
            &after.prepared
        };
        if self.primitive != Primitive::ReplaceFile && after.before != Observation::Absent {
            return Err("exchange has no before artifact");
        }
        match self.state {
            State::PublishedRetained
                if !after.target.matches(&self.prepared) || !retained.matches(&self.original) =>
            {
                Err("publication does not match observed identities")
            }
            State::ConflictRetained
                if !after.target.matches(&self.prepared)
                    || matches!(
                        retained,
                        Observation::Absent | Observation::Incomplete { identity: None, .. }
                    )
                    || retained.matches(&self.original) =>
            {
                Err("conflict has no displaced difference")
            }
            State::RefusedRetained
                if !after.prepared.matches(&self.prepared)
                    || *retained != Observation::Absent
                        && self.primitive == Primitive::ReplaceFile
                    || after.target.matches(&self.prepared) =>
            {
                Err("refusal does not prove retained proposal")
            }
            _ => Ok(()),
        }
    }

    pub fn validate_binding(
        &self,
        marker: &RootMarker,
        transaction: Incarnation,
        target: &str,
    ) -> Validation {
        marker.validate()?;
        self.validate()?;
        if self.owner != marker.owner
            || self.root != marker.root
            || self.parent != marker.parent
            || self.transaction != transaction
            || self.target_basename != target
        {
            return Err("retention binding changed");
        }
        Ok(())
    }

    pub fn validate_successor(&self, next: &Self) -> Validation {
        self.validate()?;
        next.validate()?;
        let mut lineage = next.clone();
        lineage.state = self.state;
        lineage.sequence = self.sequence;
        lineage.after = self.after.clone();
        if lineage != *self
            || self.sequence.checked_add(1) != Some(next.sequence)
            || !matches!(
                (self.state, next.state),
                (State::Prepared, State::PublicationStarted)
                    | (
                        State::PublicationStarted | State::OutcomeUnknown,
                        State::PublishedRetained
                            | State::ConflictRetained
                            | State::OutcomeUnknown
                            | State::RefusedRetained
                    )
                    | (
                        State::PublishedRetained | State::ConflictRetained | State::RefusedRetained,
                        State::ConflictRetained | State::OutcomeUnknown
                    )
            )
        {
            return Err("invalid receipt successor");
        }
        Ok(())
    }
}

/// Metadata admission only; the caller must lock, reobserve and reserve before
/// effects. This does not impose a hard cap on writes through open descriptors.
pub(super) fn admit(
    transactions: u64,
    observed_bytes: u64,
    reserved_bytes: u64,
    available_bytes: u64,
) -> Validation {
    if transactions >= MAX_TRANSACTIONS
        || available_bytes < RESERVATION_BYTES
        || observed_bytes
            .checked_add(reserved_bytes)
            .and_then(|n| n.checked_add(RESERVATION_BYTES))
            .is_none_or(|n| n > MAX_ROOT_BYTES)
    {
        return Err("retention admission exceeded");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
