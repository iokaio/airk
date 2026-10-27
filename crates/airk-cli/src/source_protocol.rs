// SPDX-License-Identifier: Apache-2.0
//! Private watcher lifecycle/configuration/events only; no retrieval or source bytes.
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    service::{ProjectBinding, ProjectRegistration},
    service_watch::{WatchBackend, WatchFailure, WatchTraversal},
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
        #[serde(default)]
        traversal: Option<WatchTraversal>,
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

pub fn valid_pulse(directories: u16, projects: u16, pending_events: u16) -> bool {
    directories <= airk_core::service_watch::MAX_WATCH_DIRECTORIES
        && projects <= 1024
        && pending_events <= 1024
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcher_pulses_accept_expanded_capacity_and_reject_out_of_bound_counts() {
        assert!(valid_pulse(4 * 601, 4, 4));
        assert!(valid_pulse(
            airk_core::service_watch::MAX_WATCH_DIRECTORIES,
            1024,
            1024
        ));
        assert!(!valid_pulse(
            airk_core::service_watch::MAX_WATCH_DIRECTORIES + 1,
            4,
            4
        ));
        assert!(!valid_pulse(0, 1025, 0));
        assert!(!valid_pulse(0, 4, 1025));
    }
}
