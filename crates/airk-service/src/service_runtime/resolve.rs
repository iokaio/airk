// SPDX-License-Identifier: Apache-2.0
//! M1 `ResolveProject` (split step 3c.1): the registered project a binding,
//! or a root, data root and collection, name. The read-only `saved` lookup of
//! `service_frontend`, unchanged; nothing is registered, started or elected.
//! A catalog session (step 3c.4c) resolves through `resolve_scoped`, as the
//! CLI resolved it before connecting.
use airk_core::service::{ControlResponse, ResolveTarget};
use std::path::Path;

/// The refusal `saved` raises for a project the registry does not hold.
const NOT_REGISTERED: &str = "project is not registered with this service";

pub(super) async fn response(control: &Path, target: ResolveTarget) -> ControlResponse {
    let resolved = match &target {
        ResolveTarget::Catalog { request } => {
            return match airk_frontend::service_frontend::resolve_scoped(
                None,
                Some(control),
                request,
            )
            .await
            {
                Ok(selected) => ControlResponse::Resolved {
                    registration: selected.project.registration,
                },
                Err(error) => ControlResponse::RegistrationRefused {
                    message: super::tools_call::failure_message(&error),
                },
            };
        }
        ResolveTarget::Binding { project } => airk_frontend::service_frontend::saved(
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
            airk_frontend::service_frontend::saved(
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
