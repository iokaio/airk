// SPDX-License-Identifier: Apache-2.0
//! Independent permanent-policy fence on a request-held native reader.
use super::CurrentRead;
use airk_core::{identity::Sha256Digest, source_policy::SourcePolicy};
use anyhow::{Result, ensure};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub type NativePolicyPredicate = fn(&SourcePolicy, &str, &str, bool) -> bool;

/// Authenticated source projection frozen before this reader is exposed.
pub(super) struct ReadSources {
    pub primary_root: String,
    pub aliases: std::collections::BTreeMap<String, String>,
    pub has_configuration: bool,
}

impl ReadSources {
    fn public(&self) -> bool {
        !airk_core::source_policy::source_root_is_reserved(&self.primary_root)
            && self
                .aliases
                .values()
                .all(|root| !airk_core::source_policy::source_root_is_reserved(root))
    }
}
#[derive(Clone, Copy, Debug)]
pub struct SourcePolicyReadUnavailable;
impl std::fmt::Display for SourcePolicyReadUnavailable {
    fn fmt(&self, output: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        output.write_str(
            "source_policy_unavailable: permanent policy removal is pending, mismatched or revoked",
        )
    }
}
impl std::error::Error for SourcePolicyReadUnavailable {}
impl SourcePolicyReadUnavailable {
    pub fn receipt(self) -> serde_json::Value {
        serde_json::json!({"error":{"code":"source_policy_unavailable","retryable":false,"message":self.to_string()}})
    }
}
pub(super) struct ReadPolicy {
    pub policy: Arc<SourcePolicy>,
    pub restricts: bool,
    primary_root: String,
    aliases: std::collections::BTreeMap<String, String>,
    predicate: NativePolicyPredicate,
    revoked: Arc<AtomicBool>,
}

impl CurrentRead {
    pub fn attach_source_policy_fence(&self, fence: Arc<AtomicBool>) -> Result<()> {
        if let Err(fence) = self.source_policy_fence.set(fence) {
            ensure!(
                self.source_policy_fence
                    .get()
                    .is_some_and(|existing| Arc::ptr_eq(existing, &fence)),
                "source policy reader fence changed"
            );
        }
        self.require_source_policy_live()
    }
    /// Native host predicate is separate from Project scope and remains active
    /// for direct CLI/admin reads. An attached reader can never change policies.
    pub fn attach_source_policy(
        &self,
        policy: Arc<SourcePolicy>,
        predicate: NativePolicyPredicate,
        revoked: Arc<AtomicBool>,
    ) -> Result<()> {
        policy.validate()?;
        self.require_source_policy_live()?;
        let sources = self
            .frozen_sources
            .get()
            .ok_or(SourcePolicyReadUnavailable)?;
        ensure!(sources.has_configuration, SourcePolicyReadUnavailable);
        let primary_root = sources.primary_root.clone();
        let aliases = sources.aliases.clone();
        let restricts = !policy.denies.is_empty()
            || policy.positive_includes.is_some()
            || !predicate(&policy, &primary_root, "", true)
            || !aliases
                .values()
                .all(|root| predicate(&policy, root, "", true));
        let candidate = ReadPolicy {
            policy,
            predicate,
            revoked,
            restricts,
            primary_root,
            aliases,
        };
        if let Err(candidate) = self.source_policy.set(candidate) {
            let current = self.source_policy.get().expect("initialized source policy");
            ensure!(
                current.policy == candidate.policy
                    && current.primary_root == candidate.primary_root
                    && current.aliases == candidate.aliases
                    && Arc::ptr_eq(&current.revoked, &candidate.revoked),
                "source policy reader cannot be rebound"
            );
        }
        self.require_source_policy_live()
    }

    pub(super) fn require_source_policy_live(&self) -> Result<()> {
        ensure!(
            self.frozen_sources.get().is_some_and(ReadSources::public),
            airk_core::scoped_reads::ScopeReadRefused
        );
        ensure!(
            self.source_policy_fence
                .get()
                .is_none_or(|fence| !fence.load(Ordering::Acquire)),
            SourcePolicyReadUnavailable
        );
        ensure!(
            self.source_policy
                .get()
                .is_none_or(|policy| !policy.revoked.load(Ordering::Acquire)),
            SourcePolicyReadUnavailable
        );
        Ok(())
    }

    pub fn source_policy_digest(&self) -> Result<Option<Sha256Digest>> {
        self.require_source_policy_live()?;
        self.source_policy
            .get()
            .map(|policy| policy.policy.identity())
            .transpose()
    }

    pub fn source_policy_allows_source(&self, root: &str, path: &str) -> bool {
        self.require_source_policy_live().is_ok()
            && !airk_core::source_policy::source_root_is_reserved(root)
            && !airk_core::source_policy::source_root_is_reserved(path)
            && self
                .source_policy
                .get()
                .is_none_or(|policy| (policy.predicate)(&policy.policy, root, path, false))
    }

    pub(super) fn source_policy_allows_path(&self, path: &str) -> bool {
        if airk_core::source_policy::source_root_is_reserved(path) {
            return false;
        }
        let Some(policy) = self.source_policy.get() else {
            return true;
        };
        if !policy.restricts {
            return true;
        }
        // Posting/lexical selectors may invoke this while holding their SQLite
        // connection mutex. Source projection is frozen at attachment; never
        // reenter collection_state or issue another database read per candidate.
        let Ok(source) = crate::roots::source_location(&policy.primary_root, &policy.aliases, path)
        else {
            return false;
        };
        (policy.predicate)(&policy.policy, &source.root, &source.relative_path, false)
    }
}
