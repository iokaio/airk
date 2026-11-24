// SPDX-License-Identifier: Apache-2.0
//! Protected endpoint publication and bounded native setup inputs.
//!
//! Local trust plan C: a record an HTTPS build wrote is read through the
//! legacy arm of `SavedEndpoint`. It carries forward its port and
//! generation, and the next publication (over plain HTTP) chains to its
//! digest; it is never served or returned as a current descriptor.
use super::{InstallationBinding, read_private, write_private};
use airk_core::{
    identity::Sha256Digest,
    service_http::{EndpointDescriptor, MAX_ENDPOINT_BYTES, SavedEndpoint},
};
use anyhow::{Result, ensure};
use std::path::Path;

const DESCRIPTOR: &str = "endpoint.json";

fn matches_binding(value: &SavedEndpoint, binding: &InstallationBinding) -> Result<()> {
    crate::http_endpoint::validate_binding(binding)?;
    if let SavedEndpoint::Current(value) = value {
        value.validate()?;
    }
    let (installation, user, namespace) = value.binding();
    ensure!(
        installation == binding.installation
            && user == binding.user
            && namespace == binding.namespace,
        "HTTP endpoint belongs to a different installation"
    );
    Ok(())
}

/// The saved record, current or legacy, bound to this installation.
pub fn load_saved(root: &Path, binding: &InstallationBinding) -> Result<Option<SavedEndpoint>> {
    crate::http_endpoint::validate_binding(binding)?;
    let Some(bytes) = read_private(root, DESCRIPTOR, MAX_ENDPOINT_BYTES)? else {
        return Ok(None);
    };
    let saved = SavedEndpoint::decode(&bytes)?;
    matches_binding(&saved, binding)?;
    Ok(Some(saved))
}

/// The current descriptor; `None` when there is none or only a legacy HTTPS
/// record, which no current reader may use as an endpoint.
pub fn load_descriptor(
    root: &Path,
    binding: &InstallationBinding,
) -> Result<Option<EndpointDescriptor>> {
    Ok(load_saved(root, binding)?.and_then(SavedEndpoint::into_current))
}

/// Caller holds the service lease and serializes publication/renewal transitions.
/// Filesystem replacement is atomic; this preimage check is not a cross-process lock.
/// The predecessor may be a legacy HTTPS record (an upgrade's first start).
pub fn publish_descriptor(
    root: &Path,
    binding: &InstallationBinding,
    next: &EndpointDescriptor,
    expected: Option<Sha256Digest>,
) -> Result<()> {
    matches_binding(&SavedEndpoint::Current(next.clone()), binding)?;
    let previous = load_saved(root, binding)?;
    ensure!(
        previous.as_ref().map(SavedEndpoint::digest).transpose()? == expected
            && next.predecessor_digest == expected,
        "HTTP endpoint publication preimage changed"
    );
    match previous.as_ref() {
        Some(SavedEndpoint::Current(previous)) => {
            ensure!(
                next.generation >= previous.generation,
                "HTTP endpoint generation regressed"
            );
            if next.generation == previous.generation {
                ensure!(
                    next.service_instance == previous.service_instance
                        && next.executable_sha256 == previous.executable_sha256
                        && next.url == previous.url,
                    "HTTP endpoint identity changed without a new generation"
                );
            }
        }
        Some(SavedEndpoint::Legacy(previous)) => ensure!(
            next.generation > previous.generation,
            "an upgraded HTTP endpoint needs a new generation"
        ),
        None => ensure!(
            next.generation == 1,
            "first HTTP endpoint generation must be one"
        ),
    }
    write_private(
        root,
        DESCRIPTOR,
        &serde_json::to_vec(next)?,
        MAX_ENDPOINT_BYTES,
        previous.is_some(),
    )
}
