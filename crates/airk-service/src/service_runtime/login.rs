// SPDX-License-Identifier: Apache-2.0
//! M21 (U101): login startup status, install and uninstall, served on
//! Windows through the setup role's registration (`setup_role::login_*`:
//! the same `ServiceManager::for_installation` identity and effects the
//! installer uses). macOS reuses the bundle-scoped SMAppService adapter; Linux
//! answers `supported: false` on this service-facing management surface. Blocking.
use airk_core::storage_admin::{LoginStartupAction, LoginStartupStatus, STORAGE_SCHEMA};
use anyhow::{Result, bail};
use std::path::Path;

/// Failure code when an error carries none.
pub(super) const UNAVAILABLE: &str = "login_startup_unavailable";

/// Pure: the answer on a platform the service cannot manage.
fn unsupported() -> LoginStartupStatus {
    LoginStartupStatus {
        schema: STORAGE_SCHEMA,
        supported: false,
        registered: false,
        command: None,
        detail: Some(
            "login startup is managed on Windows and macOS in this build; start the service with `airkctl service start`"
                .into(),
        ),
    }
}

/// Pure: the status after an action, from the observed registration.
fn observed(registered: bool, command: String) -> LoginStartupStatus {
    LoginStartupStatus {
        schema: STORAGE_SCHEMA,
        supported: true,
        registered,
        command: registered.then_some(command),
        detail: None,
    }
}

/// Pure: a service that does not serve an installation has nothing to
/// register; only a status read answers it.
fn not_installed(action: LoginStartupAction) -> Result<LoginStartupStatus> {
    match action {
        LoginStartupAction::Status => Ok(LoginStartupStatus {
            schema: STORAGE_SCHEMA,
            supported: true,
            registered: false,
            command: None,
            detail: Some(
                "this service does not run from an AIRK Service installation's state folder".into(),
            ),
        }),
        LoginStartupAction::Install | LoginStartupAction::Uninstall => bail!(
            "login_startup_not_installed: this service does not run from an AIRK Service installation's state folder"
        ),
    }
}

/// M21 for the service whose control root is `control`. Install and
/// uninstall converge (already registered / absent are successes).
pub(super) fn login_startup(
    action: LoginStartupAction,
    control: &Path,
) -> Result<LoginStartupStatus> {
    if !cfg!(any(windows, target_os = "macos")) {
        return Ok(unsupported());
    }
    let Some(target) = crate::setup_role::login_target(control)? else {
        return not_installed(action);
    };
    match action {
        LoginStartupAction::Status => {}
        LoginStartupAction::Install => {
            crate::setup_role::login_install(&target)?;
        }
        LoginStartupAction::Uninstall => {
            crate::setup_role::login_uninstall(&target)?;
        }
    }
    Ok(observed(
        crate::setup_role::login_registered(&target)?,
        target.command(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_keep_the_contract_shape() -> Result<()> {
        let off = unsupported();
        assert!(!off.supported && !off.registered && off.command.is_none());
        assert!(off.detail.is_some());
        let on = observed(true, "\"C:/AIRK/service/airk.exe\" __airk_role".into());
        assert!(on.supported && on.registered);
        assert_eq!(
            on.command.as_deref(),
            Some("\"C:/AIRK/service/airk.exe\" __airk_role")
        );
        let none = observed(false, "ignored".into());
        assert!(none.supported && !none.registered && none.command.is_none());
        let status = not_installed(LoginStartupAction::Status)?;
        assert!(status.supported && !status.registered);
        for action in [LoginStartupAction::Install, LoginStartupAction::Uninstall] {
            let error = not_installed(action).unwrap_err();
            assert_eq!(
                super::super::storage::failure_code(&error, UNAVAILABLE),
                "login_startup_not_installed"
            );
        }
        for shape in [off, on, none, status] {
            let text = serde_json::to_string(&shape)?;
            assert_eq!(serde_json::from_str::<LoginStartupStatus>(&text)?, shape);
        }
        Ok(())
    }

    #[test]
    fn uninstall_never_ends_the_serving_instance() {
        assert!(!crate::setup_role::login_uninstall_allowed(Some(true)));
        assert!(crate::setup_role::login_uninstall_allowed(Some(false)));
        assert!(crate::setup_role::login_uninstall_allowed(None));
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    #[test]
    fn other_platforms_answer_unsupported_for_every_action() -> Result<()> {
        let control = Path::new("/nonexistent/airk-state");
        for action in [
            LoginStartupAction::Status,
            LoginStartupAction::Install,
            LoginStartupAction::Uninstall,
        ] {
            assert_eq!(login_startup(action, control)?, unsupported());
        }
        Ok(())
    }
}
