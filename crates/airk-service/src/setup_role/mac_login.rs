// SPDX-License-Identifier: Apache-2.0
//! I4 ownership adapter around the existing bundle-scoped SMAppService bridge.
use super::*;
use airk_installer::service_manager::MacLoginState;

fn checked(install_id: Incarnation) -> Result<(Installed, PathBuf)> {
    ensure!(
        cfg!(target_os = "macos"),
        "login_startup_unsupported: this adapter requires macOS"
    );
    let pointer = files::pointer_path()?;
    verify_existing(&pointer)?;
    let installed =
        files::read_installation()?.context("login_startup_not_installed: no current I4")?;
    ensure!(
        installed.record.install_id == install_id,
        "login_startup_identity_mismatch: I4 changed"
    );
    let service = Path::new(&installed.record.locations.program)
        .join(&installed.record.executables.airk.path);
    let own = own_image()?;
    ensure!(
        own.0 == canonical_or_same(&service) && own.1 == installed.record.executables.airk.sha256,
        "login_startup_identity_mismatch: helper must execute from exact placed app"
    );
    let bundle = service
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("Mac app topology missing")?
        .to_path_buf();
    ensure!(
        bundle.file_name() == Some(std::ffi::OsStr::new("AIRK.app")),
        "Mac app topology differs"
    );
    #[cfg(target_os = "macos")]
    {
        let text = bundle.to_str().context("non-UTF-8 Mac bundle")?;
        let output = airk_process::bounded_command::execute(
            &airk_process::bounded_command::Invocation::new(
                "/usr/bin/codesign",
                &["--verify", "--strict", text],
            ),
        )?;
        ensure!(
            output.success,
            "login_startup_identity_mismatch: signed bundle resources changed"
        );
    }
    Ok((installed, bundle))
}

pub(super) fn direct(install_id: Incarnation, action: &str) -> Result<MacLoginState> {
    let (installed, bundle) = checked(install_id)?;
    airk_installer::service_manager::macos_i4_login(&bundle, &installed.record.channel, action)
}

pub(super) fn apply(
    airk: &Path,
    install_id: Incarnation,
    action: &str,
    operation: Option<&Path>,
) -> Result<MacLoginState> {
    if canonical_or_same(airk) == own_image()?.0 {
        return direct(install_id, action);
    }
    let operation = operation
        .context("login_startup_owner_unproven: external setup requires original operation")?;
    let operation = operation.to_str().context("non-UTF-8 setup operation")?;
    let id = install_id.to_string();
    let output =
        airk_process::bounded_command::execute(&airk_process::bounded_command::Invocation::new(
            airk,
            &[
                "__airk_role",
                "service-login-control",
                "--install-id",
                &id,
                "--action",
                action,
                "--operation",
                operation,
            ],
        ))?;
    ensure!(output.success, "login_startup_failed: {}", output.text);
    match output.text.trim() {
        "enabled" => Ok(MacLoginState::Enabled),
        "not_registered" => Ok(MacLoginState::NotRegistered),
        "requires_approval" => Ok(MacLoginState::RequiresApproval),
        "not_found" => Ok(MacLoginState::NotFound),
        _ => bail!("login_startup_failed: invalid helper status"),
    }
}

/// Exact original-operation child; parent setup retains install.lock.
pub fn control(install_id: Incarnation, action: &str, operation_path: &Path) -> Result<()> {
    ensure!(
        matches!(action, "status" | "register" | "unregister"),
        "invalid Mac login action"
    );
    let raw = files::read_operation(operation_path)?;
    let preview: SetupOperation = serde_json::from_slice(&raw)?;
    let pointer = files::pointer_path()?;
    let admitted = transaction::admission_time(&pointer, &preview, files::digest_bytes(&raw))?
        .context("login_startup_owner_unproven: no matching active setup")?;
    let operation = SetupOperation::decode(&raw, admitted)
        .map_err(|error| anyhow::anyhow!("{}", error.code()))?;
    let mut owner = transaction::Owner::load(&pointer, &operation)?
        .context("login_startup_owner_unproven: active setup already settled")?;
    owner.authorize_login(install_id, action)?;
    let state = direct(install_id, action)?;
    owner.revalidate()?;
    println!(
        "{}",
        match state {
            MacLoginState::Enabled => "enabled",
            MacLoginState::NotRegistered => "not_registered",
            MacLoginState::RequiresApproval => "requires_approval",
            MacLoginState::NotFound => "not_found",
        }
    );
    Ok(())
}
