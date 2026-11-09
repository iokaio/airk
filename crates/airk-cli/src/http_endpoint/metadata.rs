// SPDX-License-Identifier: Apache-2.0
//! Protected endpoint publication and bounded native setup inputs.
use super::{InstallationBinding, read_private, write_private};
use airk_core::{
    identity::Sha256Digest,
    service_http::{EndpointDescriptor, HttpSelection, MAX_CONNECTION_BYTES, MAX_ENDPOINT_BYTES},
};
use anyhow::{Result, ensure};
use std::path::Path;

const DESCRIPTOR: &str = "endpoint.json";

fn matches_binding(value: &EndpointDescriptor, binding: &InstallationBinding) -> Result<()> {
    binding.validate()?;
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
    binding.validate()?;
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

/// A setup selection is inert, but must be read from the exact protected file.
pub fn read_selection_file(path: &Path) -> Result<HttpSelection> {
    ensure!(path.is_absolute(), "HTTP selection file must be absolute");
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("HTTP selection parent missing"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("HTTP selection filename invalid"))?;
    let bytes = read_private(parent, name, MAX_CONNECTION_BYTES)?
        .ok_or_else(|| anyhow::anyhow!("HTTP selection file unavailable"))?;
    let selection: HttpSelection = serde_json::from_slice(&bytes)?;
    selection.validate()?;
    Ok(selection)
}

/// Legacy migration consumes only the exact protected native registration file.
pub fn read_registration_file(path: &Path) -> Result<airk_core::service::ProjectRegistration> {
    ensure!(
        path.is_absolute(),
        "HTTP registration file must be absolute"
    );
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("HTTP registration parent missing"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("HTTP registration filename invalid"))?;
    let bytes = read_private(
        parent,
        name,
        airk_core::service::MAX_PROJECT_REGISTRATION_BYTES,
    )?
    .ok_or_else(|| anyhow::anyhow!("HTTP registration file unavailable"))?;
    let registration: airk_core::service::ProjectRegistration = serde_json::from_slice(&bytes)?;
    registration.validate()?;
    Ok(registration)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyRegistrationInput {
    pub registration: airk_core::service::ProjectRegistration,
    pub configuration_receipt: Sha256Digest,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyRegistrations {
    pub schema: u32,
    pub registrations: Vec<LegacyRegistrationInput>,
    pub no_default_project: bool,
}
impl LegacyRegistrations {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == 1
                && self.no_default_project
                && (2..=32).contains(&self.registrations.len()),
            "legacy union requires 2..32 original bindings and no default project"
        );
        let mut bindings = std::collections::BTreeSet::new();
        for member in &self.registrations {
            member.registration.validate()?;
            ensure!(
                bindings.insert(member.registration.binding),
                "duplicate legacy registration"
            );
        }
        Ok(())
    }
}
pub fn read_registrations_file(path: &Path) -> Result<LegacyRegistrations> {
    ensure!(
        path.is_absolute(),
        "HTTP registrations file must be absolute"
    );
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("HTTP registrations parent missing"))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::anyhow!("HTTP registrations filename invalid"))?;
    let bytes = read_private(
        parent,
        name,
        airk_core::service_http::MAX_HTTP_SELECTION_BYTES,
    )?
    .ok_or_else(|| anyhow::anyhow!("HTTP registrations file unavailable"))?;
    let input: LegacyRegistrations = serde_json::from_slice(&bytes)?;
    input.validate()?;
    Ok(input)
}
