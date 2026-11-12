// SPDX-License-Identifier: Apache-2.0
//! Project registration (split step 3c.4b): M4a plans the canonical
//! registration and M4b prepares the reserved storage; `init` runs them
//! around the existing `ReserveProject` exactly as `airk init` did.
use crate::contract::{ErrorLine, ExitClass};
use airk_core::service::{
    ControlMethod, ControlResponse, ProjectBinding, ProjectRegistration, RegisteredProject,
    RegistrationStorage,
};
use std::path::Path;

/// A refusal with `airk`'s `operation_failed` message.
pub fn failed(message: &str) -> ErrorLine {
    ErrorLine::new(ExitClass::Refused, "operation_failed", false, message)
}

fn refused(response: &ControlResponse) -> ErrorLine {
    match response {
        ControlResponse::RegistrationRefused { message } => {
            let mut line = failed(message);
            line.native = serde_json::to_value(response).ok();
            line
        }
        other => crate::service::refusal(other),
    }
}

/// The absolute UTF-8 form of a path in this process's working folder; the
/// service's working folder is not the caller's.
pub fn absolute(path: &Path) -> Option<String> {
    std::path::absolute(path)
        .ok()
        .and_then(|path| path.to_str().map(str::to_owned))
}

/// M4a: the canonical registration for a root, data root and collection.
pub fn plan(
    control: &Path,
    version: &str,
    root: &Path,
    data: &Path,
    collection: &str,
    storage: RegistrationStorage,
) -> Result<ProjectRegistration, ErrorLine> {
    let (Some(root), Some(data_root)) = (absolute(root), absolute(data)) else {
        return Err(ErrorLine::new(
            ExitClass::Usage,
            "usage",
            false,
            "--root and --data-dir must be UTF-8 paths",
        ));
    };
    let method = ControlMethod::PlanRegistration {
        root,
        data_root,
        collection: collection.into(),
        storage,
    };
    match crate::service::request(control, method, version)? {
        ControlResponse::RegistrationPlan { registration } => Ok(registration),
        other => Err(refused(&other)),
    }
}

/// M4b: create the storage the registry reserved for the project.
pub fn prepare_storage(
    control: &Path,
    version: &str,
    project: ProjectBinding,
) -> Result<ProjectRegistration, ErrorLine> {
    match crate::service::request(control, ControlMethod::PrepareStorage { project }, version)? {
        ControlResponse::StoragePrepared { registration } => Ok(registration),
        other => Err(refused(&other)),
    }
}

/// What `init` ended with: the acknowledged project, or the response the
/// service gave instead of `Registered` (printed, as `airk` printed it).
pub enum Init {
    Registered(RegisteredProject),
    NotAcknowledged(Box<ControlResponse>),
}

/// `airk init`'s sequence: plan (storage may be missing), check a requested
/// binding, reserve, check the acknowledgement, then prepare the storage.
pub fn init(
    control: &Path,
    version: &str,
    root: &Path,
    data: &Path,
    collection: &str,
    requested: Option<ProjectBinding>,
) -> Result<Init, ErrorLine> {
    let registration = plan(
        control,
        version,
        root,
        data,
        collection,
        RegistrationStorage::Planned,
    )?;
    if requested.is_some_and(|binding| binding != registration.binding) {
        return Err(failed(
            "requested project binding differs from canonical registration",
        ));
    }
    let response = crate::service::request(
        control,
        ControlMethod::ReserveProject {
            project: registration.clone(),
            setup_watch: None,
            only_if_missing: false,
        },
        version,
    )?;
    let ControlResponse::Registered { project } = response else {
        return Ok(Init::NotAcknowledged(Box::new(response)));
    };
    if project.registration != registration {
        return Err(failed("crossed native registration"));
    }
    if prepare_storage(control, version, registration.binding)? != registration {
        return Err(failed("changed prepared storage identity"));
    }
    Ok(Init::Registered(project))
}
