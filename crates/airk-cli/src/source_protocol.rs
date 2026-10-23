// SPDX-License-Identifier: Apache-2.0
//! Private watcher lifecycle/configuration/events only; no retrieval or source bytes.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{ProjectBinding, ProjectRegistration},
    service_watch::{WatchBackend, WatchFailure},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Init {
    pub protocol: u32,
    pub version: String,
    pub runtime: PathBuf,
    pub service: Incarnation,
    pub worker: Incarnation,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Activate {
        service: Incarnation,
        worker: Incarnation,
    },
    Begin {
        registration: ProjectRegistration,
        configuration_epoch: u64,
        configuration_observed: bool,
        additional_roots: u16,
        digest: Sha256Digest,
    },
    Root {
        project: ProjectBinding,
        configuration_epoch: u64,
        path: String,
    },
    Commit {
        project: ProjectBinding,
        configuration_epoch: u64,
    },
    Remove {
        project: ProjectBinding,
        configuration_epoch: u64,
    },
    Acknowledge {
        project: ProjectBinding,
        configuration_epoch: u64,
        sequence: u64,
    },
    Stop,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case", deny_unknown_fields)]
pub enum Reply {
    Ready {
        service: Incarnation,
        worker: Incarnation,
    },
    Activated,
    Observation {
        project: ProjectBinding,
        configuration_epoch: u64,
        native_roots: u16,
        backend: WatchBackend,
        failure: Option<WatchFailure>,
    },
    Event {
        project: ProjectBinding,
        configuration_epoch: u64,
        sequence: u64,
    },
    Pulse {
        watched_directories: u16,
        projects: u16,
        pending_events: u16,
    },
}

pub fn binding_digest(
    registration: &ProjectRegistration,
    epoch: u64,
    observed: bool,
    roots: &[String],
) -> anyhow::Result<Sha256Digest> {
    let bytes = serde_json::to_vec(&(registration, epoch, observed, roots))?;
    anyhow::ensure!(
        bytes.len() <= 128 * 1024,
        "watch binding exceeds metadata budget"
    );
    Ok(airk_core::identity::framed_digest(
        b"airk-source-watch-binding/1",
        &[&bytes],
    ))
}
