// SPDX-License-Identifier: Apache-2.0
//! M1 `ResolveProject` (split step 3c.1): the registered project a binding,
//! or a root, data root and collection, name. The read-only `saved` lookup of
//! `service_frontend`, unchanged; nothing is registered, started or elected.
use airk_core::service::{ControlResponse, ResolveTarget};
use std::path::Path;

/// The refusal `saved` raises for a project the registry does not hold.
const NOT_REGISTERED: &str = "project is not registered with this service";

pub(super) fn response(control: &Path, target: ResolveTarget) -> ControlResponse {
    let resolved = match &target {
        // The catalog target is served in step 3c.4c-2; until then the typed refusal.
        ResolveTarget::Catalog { .. } => {
            return ControlResponse::Failure {
                code: "not_yet_served".into(),
            };
        }
        ResolveTarget::Binding { project } => crate::service_frontend::saved(
            control,
            Some(*project),
            Path::new(""),
            Path::new(""),
            "",
        ),
        ResolveTarget::Registration {
            root,
            data_root,
            collection,
        } => {
            if !Path::new(root).is_absolute() || !Path::new(data_root).is_absolute() {
                return ControlResponse::Failure {
                    code: "project_location_relative".into(),
                };
            }
            crate::service_frontend::saved(
                control,
                None,
                Path::new(root),
                Path::new(data_root),
                collection,
            )
        }
    };
    match resolved {
        Ok(registration) => ControlResponse::Resolved { registration },
        Err(error) if format!("{error:#}").starts_with(NOT_REGISTERED) => {
            ControlResponse::Failure {
                code: "project_not_registered".into(),
            }
        }
        Err(_) => ControlResponse::Failure {
            code: "project_unresolved".into(),
        },
    }
}
