// SPDX-License-Identifier: Apache-2.0
//! Explicit old-startup review/removal. Never execute the reviewed old command.
use crate::service_manager::ServiceManager;
use airk_core::identity::{Incarnation, Sha256Digest, framed_digest};
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[path = "legacy_startup_journal.rs"]
mod journal;
use journal::{Intent, Journal, Phase};

#[derive(Clone, Debug, Args, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    #[arg(long)]
    pub root: PathBuf,
    #[arg(long)]
    pub data_dir: PathBuf,
    #[arg(long)]
    pub collection: String,
    #[arg(long)]
    pub executable: PathBuf,
}
#[derive(Subcommand)]
pub enum LegacyCommand {
    /// Read saved ownership and OS state; do not start, stop or convert anything.
    Review {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[command(flatten)]
        target: Target,
    },
    /// Remove only reviewed old login startup; preserve sources/settings/indexes.
    RemoveStartup {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[command(flatten)]
        target: Target,
        #[arg(long)]
        approval: Sha256Digest,
    },
    /// List bounded saved removal identities/phases, including interrupted work.
    List {
        #[arg(long)]
        runtime_dir: PathBuf,
    },
    Show {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        approval: Sha256Digest,
    },
    /// Forget only a completed local receipt; never stop a process or repeat work.
    Forget {
        #[arg(long)]
        runtime_dir: PathBuf,
        #[arg(long)]
        approval: Sha256Digest,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Spec {
    id: String,
    target: Target,
    definition: PathBuf,
    receipt: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct FileProof {
    sha256: Sha256Digest,
    bytes: u64,
    modified_ns: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Observation {
    owned: bool,
    loaded: Option<bool>,
    running: Option<bool>,
    definition_matches: Option<bool>,
    diagnostic: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Preview {
    version: u32,
    namespace: Incarnation,
    epoch: u64,
    spec: Spec,
    definition: Option<FileProof>,
    receipt: Option<FileProof>,
    observation: Observation,
    removal_available: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Review {
    preview: Preview,
    approval: Option<Sha256Digest>,
    workers: crate::legacy_processes::Audit,
}
impl Review {
    fn digest(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"legacy-startup-removal/1.4",
            &[&serde_json::to_vec(&self.preview)?],
        ))
    }
    fn validate(&self, namespace: Incarnation) -> Result<()> {
        ensure!(
            self.preview.version == 1 && self.preview.namespace == namespace,
            "foreign legacy review namespace"
        );
        ensure!(
            self.approval
                == if self.preview.removal_available {
                    Some(self.digest()?)
                } else {
                    None
                },
            "changed legacy review approval"
        );
        ensure!(
            !self.preview.removal_available
                || self.preview.definition.is_some()
                    && self.preview.receipt.is_some()
                    && self.preview.observation.owned
                    && (self.preview.observation.loaded == Some(false)
                        || self.preview.observation.loaded == Some(true)
                            && self.preview.observation.definition_matches == Some(true)),
            "legacy review lacks ownership proof"
        );
        Ok(())
    }
}
trait Backend {
    fn workers(&self) -> crate::legacy_processes::Audit {
        crate::legacy_processes::Audit::unavailable()
    }
    fn spec(&self) -> Result<Spec>;
    fn observe(&self) -> Result<Observation>;
    fn disable(&self) -> Result<()>;
    fn state(&self) -> Result<(Option<bool>, Option<bool>)>;
    fn reload(&self) -> Result<()>;
}
struct Native(ServiceManager);
impl Native {
    fn new(target: &Target) -> Result<Self> {
        Ok(Self(ServiceManager::for_reviewed_legacy(
            &target.root,
            &target.data_dir,
            &target.collection,
            &target.executable,
        )?))
    }
}
impl Backend for Native {
    fn workers(&self) -> crate::legacy_processes::Audit {
        self.spec()
            .map(|spec| crate::legacy_processes::audit(&spec.target.executable))
            .unwrap_or_else(|_| crate::legacy_processes::Audit::unavailable())
    }
    fn spec(&self) -> Result<Spec> {
        let plan = self.0.plan()?;
        ensure!(plan.arguments.len() == 9, "unexpected legacy command shape");
        let (definition, receipt) = self.0.legacy_paths()?;
        Ok(Spec {
            id: plan.id,
            target: Target {
                root: plan.arguments[1].clone().into(),
                data_dir: plan.arguments[3].clone().into(),
                collection: plan.arguments[5].clone(),
                executable: plan.executable,
            },
            definition: definition.into(),
            receipt: receipt.into(),
        })
    }
    fn observe(&self) -> Result<Observation> {
        let owned = self.0.legacy_definition_digest()?.is_some();
        let status = self.0.status()?;
        Ok(Observation {
            owned: owned && status.installed,
            loaded: status.loaded,
            running: status.running,
            definition_matches: status.registered_definition_matches,
            diagnostic: status.registration_verification_error,
        })
    }
    fn disable(&self) -> Result<()> {
        self.0.disable_reviewed_legacy()
    }
    fn state(&self) -> Result<(Option<bool>, Option<bool>)> {
        self.0.legacy_manager_state()
    }
    fn reload(&self) -> Result<()> {
        self.0.reload_reviewed_legacy()
    }
}
fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
fn proof(path: &Path) -> Result<Option<FileProof>> {
    if !exists(path)? {
        return Ok(None);
    }
    crate::runtime_install::safe_file(path)?;
    let metadata = fs::metadata(path)?;
    ensure!(
        metadata.len() <= 128 * 1024,
        "legacy metadata exceeds budget"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 128 * 1024 && bytes.len() as u64 == metadata.len(),
        "legacy metadata changed while reading"
    );
    crate::runtime_install::safe_file(path)?;
    ensure!(
        fs::metadata(path)?.modified()? == metadata.modified()?,
        "legacy metadata changed while reading"
    );
    use sha2::{Digest, Sha256};
    Ok(Some(FileProof {
        sha256: Sha256Digest::from_bytes(Sha256::digest(&bytes).into()),
        bytes: bytes.len() as u64,
        modified_ns: metadata
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
            .to_string(),
    }))
}
fn inspect(journal: &Journal, backend: &impl Backend) -> Result<Review> {
    let spec = backend.spec()?;
    let definition = proof(&spec.definition)?;
    let receipt = proof(&spec.receipt)?;
    let observation = backend.observe().unwrap_or_else(|error| Observation {
        owned: false,
        loaded: None,
        running: None,
        definition_matches: None,
        diagnostic: Some(error.to_string().chars().take(512).collect()),
    });
    let removal_available = definition.is_some()
        && receipt.is_some()
        && observation.owned
        && (observation.loaded == Some(false)
            || observation.loaded == Some(true) && observation.definition_matches == Some(true));
    let mut review = Review {
        workers: backend.workers(),
        preview: Preview {
            version: 1,
            namespace: journal.namespace,
            epoch: journal.epoch(),
            spec,
            definition,
            receipt,
            observation,
            removal_available,
        },
        approval: None,
    };
    if removal_available {
        review.approval = Some(review.digest()?)
    }
    review.validate(journal.namespace)?;
    ensure!(
        serde_json::to_vec(&review)?.len() <= 16384 - 512,
        "legacy preview exceeds budget"
    );
    Ok(review)
}
fn remove(
    control: &Path,
    backend: &impl Backend,
    approval: Sha256Digest,
) -> Result<serde_json::Value> {
    let mut journal = Journal::locked(control)?;
    let mut intent = if let Some(intent) = journal.get(approval)? {
        intent
    } else {
        let review = inspect(&journal, backend)?;
        ensure!(
            review.approval == Some(approval),
            "legacy startup review changed or cannot prove OS ownership; review again"
        );
        let intent = Intent {
            version: 1,
            review,
            phase: Phase::Planned,
        };
        journal.save(&intent)?;
        intent
    };
    ensure!(
        intent.review.preview.spec == backend.spec()?,
        "saved legacy removal belongs to another source, collection or executable"
    );
    if intent.phase == Phase::Complete {
        return Ok(completed(approval));
    }
    let spec = intent.review.preview.spec.clone();
    if intent.phase == Phase::Planned {
        ensure!(
            proof(&spec.definition)? == intent.review.preview.definition
                && proof(&spec.receipt)? == intent.review.preview.receipt,
            "legacy startup files changed after approval"
        );
        backend.disable()?;
        intent.phase = Phase::Disabled;
        journal.save(&intent)?;
    }
    if intent.phase == Phase::Disabled {
        let definition = proof(&spec.definition)?;
        let receipt = proof(&spec.receipt)?;
        ensure!(
            definition.is_none() || definition == intent.review.preview.definition,
            "changed legacy definition preserved"
        );
        ensure!(
            receipt == intent.review.preview.receipt,
            "changed or missing legacy ownership receipt preserved"
        );
        if definition.is_some() {
            let observed = backend.observe()?;
            ensure!(
                observed.owned
                    && (observed.loaded == Some(false)
                        || observed.loaded == Some(true)
                            && observed.definition_matches == Some(true)),
                "OS legacy registration changed after approval"
            );
            if observed.running == Some(true) {
                backend.disable()?;
            }
            let (_, running) = backend.state()?;
            ensure!(
                running == Some(false),
                "OS has not proven the old startup worker stopped"
            );
            fs::remove_file(&spec.definition)?;
        }
        intent.phase = Phase::DefinitionRemoved;
        journal.save(&intent)?;
    }
    if intent.phase == Phase::DefinitionRemoved {
        ensure!(
            !exists(&spec.definition)?,
            "legacy definition reappeared; preserve it for review"
        );
        let (_, running) = backend.state()?;
        ensure!(
            running == Some(false),
            "an unknown legacy worker is running; preserve the pending removal"
        );
        backend.reload()?;
        ensure!(
            backend.state()? == (Some(false), Some(false)),
            "OS has not confirmed legacy startup removal"
        );
        if let Some(receipt) = proof(&spec.receipt)? {
            ensure!(
                Some(receipt) == intent.review.preview.receipt,
                "changed legacy receipt preserved"
            );
            fs::remove_file(&spec.receipt)?;
        }
        intent.phase = Phase::ReceiptRemoved;
        journal.save(&intent)?;
    }
    if intent.phase == Phase::ReceiptRemoved {
        ensure!(
            !exists(&spec.definition)? && !exists(&spec.receipt)?,
            "legacy files reappeared; preserve them for review"
        );
        ensure!(
            backend.state()? == (Some(false), Some(false)),
            "OS registration reappeared before completion"
        );
        journal.advance_epoch(
            intent
                .review
                .preview
                .epoch
                .checked_add(1)
                .context("legacy request epoch exhausted")?,
        )?;
        intent.phase = Phase::Complete;
        journal.save(&intent)?;
    }
    Ok(completed(approval))
}
fn completed(approval: Sha256Digest) -> serde_json::Value {
    serde_json::json!({"approval":approval,"startup_removed":true,"sources_preserved":true,"project_settings_preserved":true,"indexes_retained":true,"unmanaged_old_workers":"not stopped by this operation"})
}
pub(crate) fn review_target(control: &Path, target: &Target) -> Result<Review> {
    inspect(&Journal::saved(control)?, &Native::new(target)?)
}
pub(crate) fn remove_target(
    control: &Path,
    target: &Target,
    approval: Sha256Digest,
) -> Result<serde_json::Value> {
    remove(control, &Native::new(target)?, approval)
}
pub fn run(command: &LegacyCommand) -> Result<()> {
    let value = match command {
        LegacyCommand::Review {
            runtime_dir,
            target,
        } => serde_json::to_value(review_target(runtime_dir, target)?)?,
        LegacyCommand::RemoveStartup {
            runtime_dir,
            target,
            approval,
        } => remove_target(runtime_dir, target, *approval)?,
        LegacyCommand::List { runtime_dir } => Journal::saved(runtime_dir)?.list()?,
        LegacyCommand::Show {
            runtime_dir,
            approval,
        } => serde_json::to_value(
            Journal::saved(runtime_dir)?
                .get(*approval)?
                .context("legacy removal receipt not retained")?,
        )?,
        LegacyCommand::Forget {
            runtime_dir,
            approval,
        } => {
            Journal::locked(runtime_dir)?.forget(*approval)?;
            serde_json::json!({"forgotten":approval,"native_work_repeated":false})
        }
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
#[path = "legacy_startup_tests.rs"]
mod tests;
