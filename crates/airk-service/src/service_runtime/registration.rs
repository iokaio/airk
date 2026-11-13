// SPDX-License-Identifier: Apache-2.0
//! M4a/M4b (split step 3c.4b): registration planning and reserved storage
//! through the CLI's canonical functions, unchanged. A refusal carries the
//! full error chain the CLI printed.
use airk_core::service::{ControlResponse, ProjectBinding, RegistrationStorage};
use std::path::Path;

fn refused(error: &anyhow::Error) -> ControlResponse {
    ControlResponse::RegistrationRefused {
        message: super::tools_call::failure_message(error),
    }
}

/// M4a: `planned_registration` (storage may be missing) or `registration`
/// (storage must exist), as the CLI chose for the command.
pub(super) fn plan(
    root: &str,
    data_root: &str,
    collection: &str,
    storage: RegistrationStorage,
) -> ControlResponse {
    let (root, data) = (Path::new(root), Path::new(data_root));
    if !root.is_absolute() || !data.is_absolute() {
        return ControlResponse::RegistrationRefused {
            message: "registration paths must be absolute".into(),
        };
    }
    let planned = match storage {
        RegistrationStorage::Planned => {
            airk_host::service_registration::prepare(root, data, collection)
        }
        RegistrationStorage::Existing => {
            airk_kernel::service_projects::registration(root, data, collection)
        }
    };
    match planned {
        Ok(registration) => ControlResponse::RegistrationPlan { registration },
        Err(error) => refused(&error),
    }
}

/// M4b: create the storage the registry reserved for the project.
pub(super) fn prepare_storage(control: &Path, project: ProjectBinding) -> ControlResponse {
    match airk_host::service_registration::prepare_storage(control, project) {
        Ok(registration) => ControlResponse::StoragePrepared { registration },
        Err(error) => refused(&error),
    }
}
