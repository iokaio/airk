// SPDX-License-Identifier: Apache-2.0
//! Protected endpoint publication and bounded native setup inputs.
use super::{InstallationBinding, read_private, write_private};
use airk_core::{
    identity::Sha256Digest,
    service_http::{EndpointDescriptor, MAX_ENDPOINT_BYTES},
};
use anyhow::{Result, ensure};
use std::path::Path;

const DESCRIPTOR: &str = "endpoint.json";

fn matches_binding(value: &EndpointDescriptor, binding: &InstallationBinding) -> Result<()> {
    crate::http_endpoint::validate_binding(binding)?;
    value.validate()?;
    ensure!(
        value.installation == binding.installation
            && value.user == binding.user
            && value.namespace == binding.namespace,
        "HTTP endpoint belongs to a different installation"
    );
    Ok(())
}

pub fn load_descriptor(
    root: &Path,
    binding: &InstallationBinding,
) -> Result<Option<EndpointDescriptor>> {
    crate::http_endpoint::validate_binding(binding)?;
    let Some(bytes) = read_private(root, DESCRIPTOR, MAX_ENDPOINT_BYTES)? else {
        return Ok(None);
    };
    let descriptor = EndpointDescriptor::decode(&bytes)?;
    matches_binding(&descriptor, binding)?;
    Ok(Some(descriptor))
}

/// Caller holds the service lease and serializes publication/renewal transitions.
/// Filesystem replacement is atomic; this preimage check is not a cross-process lock.
pub fn publish_descriptor(
    root: &Path,
    binding: &InstallationBinding,
    next: &EndpointDescriptor,
    expected: Option<Sha256Digest>,
) -> Result<()> {
    matches_binding(next, binding)?;
    let previous = load_descriptor(root, binding)?;
    ensure!(
        previous
            .as_ref()
            .map(EndpointDescriptor::digest)
            .transpose()?
            == expected
            && next.predecessor_digest == expected,
        "HTTP endpoint publication preimage changed"
    );
    match previous.as_ref() {
        Some(previous) => {
            ensure!(
                next.generation >= previous.generation,
                "HTTP endpoint generation regressed"
            );
            if next.generation == previous.generation {
                ensure!(
                    next.service_instance == previous.service_instance
                        && next.executable_sha256 == previous.executable_sha256
                        && next.url == previous.url
                        && next.ca_sha256 == previous.ca_sha256,
                    "HTTP endpoint identity changed without a new generation"
                );
            }
        }
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
