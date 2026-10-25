// SPDX-License-Identifier: Apache-2.0
//! Run SMAppService only from the exact approved immutable application bundle.
use super::*;
use std::ffi::CString;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn airk_macos_login(
        action: std::ffi::c_int,
        plist: *const std::ffi::c_char,
        bundle: *const std::ffi::c_char,
        observed: *mut std::ffi::c_int,
        error: *mut i64,
    ) -> std::ffi::c_int;
}
// Compile the Rust adapter in host unit tests without pretending to provide the
// unavailable Apple framework. Product commands are rejected on other systems.
#[cfg(not(target_os = "macos"))]
unsafe fn airk_macos_login(
    _: std::ffi::c_int,
    _: *const std::ffi::c_char,
    _: *const std::ffi::c_char,
    _: *mut std::ffi::c_int,
    _: *mut i64,
) -> std::ffi::c_int {
    -1
}
pub(crate) fn channel_name(channel: RuntimeChannel) -> &'static str {
    match channel {
        RuntimeChannel::Stable => "stable",
        RuntimeChannel::Preview => "preview",
    }
}
pub(crate) fn channel(root: &Path) -> Result<RuntimeChannel> {
    Ok(layout::retained_parameters(root)?.1)
}
pub(crate) fn candidate(root: &Path) -> Result<(PathBuf, Approval)> {
    if let Some(candidate) = activation::startup(root)? {
        return Ok(candidate);
    }
    let (_, main, approval) = selected(root)?;
    Ok((main, approval))
}
pub(crate) fn require_start_allowed(root: &Path) -> Result<()> {
    lifecycle::require_start_allowed(root)
}
fn job_scope(text: &str, main: &Path, plist: &Path, channel: RuntimeChannel) -> Result<()> {
    ensure!(
        text.len() <= 65_536,
        "Mac launchd observation exceeds budget"
    );
    let mut depth = 0isize;
    let mut program = None;
    let mut origin = None;
    let mut arguments = Vec::new();
    let mut in_arguments = false;
    let mut arguments_seen = false;
    for line in text.lines() {
        let value = line.trim();
        if depth == 1 {
            if let Some(value) = value.strip_prefix("program = ") {
                ensure!(
                    program.replace(value.to_owned()).is_none(),
                    "duplicate Mac job program"
                );
            } else if let Some(value) = value.strip_prefix("path = ") {
                ensure!(
                    origin.replace(value.to_owned()).is_none(),
                    "duplicate Mac job origin"
                );
            } else if value == "arguments = {" {
                ensure!(!arguments_seen, "duplicate Mac job arguments");
                arguments_seen = true;
                in_arguments = true;
            }
        } else if in_arguments && depth == 2 && value != "}" {
            ensure!(
                arguments.len() < 8 && value.len() <= 4096,
                "Mac job argument budget exceeded"
            );
            arguments.push(value.to_owned());
        }
        if in_arguments && depth == 2 && value == "}" {
            in_arguments = false;
        }
        // Treat unknown/changed native print grammar as unverifiable, never as absence.
        if value.ends_with("{") {
            depth += 1;
        }
        if value == "}" {
            depth -= 1;
        }
        ensure!(
            (0..=16).contains(&depth),
            "unsupported Mac job observation structure"
        );
    }
    ensure!(
        depth == 0
            && program.as_deref() == main.to_str()
            && origin.as_deref() == plist.to_str()
            && arguments
                == [
                    "airk",
                    "runtime",
                    "login-foreground",
                    "--channel",
                    channel_name(channel)
                ],
        "Mac job origin/program/arguments differ from captured signed bundle"
    );
    Ok(())
}
fn observe_job(main: &Path, channel: RuntimeChannel) -> Result<Option<String>> {
    let domain = format!("gui/{}", crate::local_ipc::current_user()?);
    let (domain_available, _) = crate::service_manager::execute_native_observation(
        Path::new("/bin/launchctl"),
        &["print", &domain],
    )?;
    ensure!(
        domain_available,
        "Mac GUI user domain is unavailable; retain on-demand mode"
    );
    let label = format!("io.iokaio.airk.service.{}", channel_name(channel));
    let target = format!("{domain}/{label}");
    let (present, text) = crate::service_manager::execute_native_observation(
        Path::new("/bin/launchctl"),
        &["print", &target],
    )?;
    if !present {
        ensure!(
            text.contains(&format!("Could not find service \"{label}\"")),
            "Mac job absence could not be proven"
        );
        return Ok(None);
    }
    let plist = bundle(main)?
        .join("Contents/Library/LaunchAgents")
        .join(format!("{label}.plist"));
    job_scope(&text, main, &plist, channel)?;
    Ok(Some(text))
}
pub(crate) fn bundle(main: &Path) -> Result<PathBuf> {
    let payload = runtime_pair::payload_root(main)?;
    let expected = payload.join(main_name());
    ensure!(
        main.canonicalize()? == expected.canonicalize()?,
        "Mac login must run inside the approved app bundle"
    );
    Ok(payload.join("AIRK.app"))
}
fn approved_main(root: &Path, identity: &str, full: bool) -> Result<PathBuf> {
    digest(identity)?;
    managed::require_unmanaged(root)?;
    let root = root.canonicalize()?;
    layout::saved_control(&root)?;
    let directory = payload_path(&root, &format!("runtimes/{identity}/airk-install.json"))?
        .parent()
        .context("Mac payload parent missing")?
        .to_path_buf();
    let (approval, expected) = saved_payload_identity(&directory, identity)?;
    let main = directory.join(main_name());
    ensure!(
        runtime_pair::digest_file(&main)? == expected,
        "Mac login executable differs from approved bytes"
    );
    let bytes = read_bounded(&directory.join("airk-install.json"), MAX_MANIFEST)?;
    let inventory: InstallManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        inventory.os == "macos"
            && inventory.arch == std::env::consts::ARCH
            && approval.version == inventory.version,
        "Mac login payload target differs"
    );
    // Keep Stop/status possible when unrelated helper data is damaged. Startup
    // metadata and the executable performing the OS action must still be exact.
    let prefix = "AIRK.app/Contents/";
    for (name, expected) in &inventory.files {
        if name.starts_with(prefix) && name != helper_name() && name != main_name() {
            ensure!(
                runtime_pair::digest_file(&payload_path(&directory, name)?)? == *expected,
                "Mac login bundle metadata changed: {name}"
            );
        }
    }
    for name in [
        "AIRK.app/Contents/Info.plist",
        "AIRK.app/Contents/Library/LaunchAgents/io.iokaio.airk.service.stable.plist",
        "AIRK.app/Contents/Library/LaunchAgents/io.iokaio.airk.service.preview.plist",
    ] {
        ensure!(
            inventory.files.contains_key(name),
            "Mac login bundle resource is missing"
        );
        ensure!(
            fs::metadata(directory.join(name))?.len() <= 16_384,
            "Mac login resource exceeds metadata budget"
        );
    }
    if full {
        verify_runtime(&root, identity)?;
    }
    // Signature checks are independent of the manifest's own checksum. Installation
    // approval remains the distributor/user policy; ad-hoc development signatures
    // do not establish Developer ID or notarization qualification.
    let target = if full { bundle(&main)? } else { main.clone() };
    let output = super::super::service_manager::execute_native_program(
        Path::new("/usr/bin/codesign"),
        &[
            "--verify",
            "--strict",
            target.to_str().context("non-UTF-8 Mac bundle")?,
        ],
    )?;
    ensure!(
        output,
        "Mac login requires an intact signed application/executable"
    );
    Ok(main)
}
pub(crate) fn native(
    root: &Path,
    identity: &str,
    expected_channel: RuntimeChannel,
    action: &str,
) -> Result<serde_json::Value> {
    ensure!(
        channel(root)? == expected_channel,
        "Mac login channel changed"
    );
    let operation = match action {
        "status" => 0,
        "register" => 1,
        "unregister" => 2,
        _ => bail!("unknown Mac login action"),
    };
    if operation != 0 {
        crate::service_manager::verify_macos_login_intent(root, identity, expected_channel)?;
    }
    if operation == 1 {
        require_start_allowed(root)?;
        ensure!(
            quiet(root)?,
            "drain native owners before private Mac registration"
        );
    }
    let main = approved_main(root, identity, operation == 1)?;
    ensure!(
        std::env::current_exe()?.canonicalize()? == main.canonicalize()?,
        "Mac login API must execute from the captured app"
    );
    let prior = observe_job(&main, expected_channel)?;
    if operation == 1 {
        ensure!(
            prior.is_none(),
            "existing launchd label is preserved; inspect ownership before registration"
        );
    }
    let bundle = bundle(&main)?.canonicalize()?;
    let plist = CString::new(format!(
        "io.iokaio.airk.service.{}.plist",
        channel_name(expected_channel)
    ))?;
    let bundle = CString::new(bundle.to_str().context("non-UTF-8 Mac app")?)?;
    let mut observed = -1;
    let mut error = 0;
    // SAFETY: the bridge receives live NUL-terminated strings and correctly
    // sized writable outputs. It validates NSBundle identity before OS effects.
    let result = unsafe {
        airk_macos_login(
            operation,
            plist.as_ptr(),
            bundle.as_ptr(),
            &mut observed,
            &mut error,
        )
    };
    match result {
        0 => (),
        -1 => bail!("Mac login requires macOS 13 or later"),
        -2 => bail!("Mac login app or fixed plist identity mismatch"),
        -3 => bail!(
            "SMAppService {action} failed with native error {error}; inspect Login Items and native policy"
        ),
        -5 => bail!(
            "Mac login consent is disabled or pending; approve explicitly in System Settings before Start"
        ),
        _ => bail!("SMAppService returned an unsupported state"),
    }
    let state = match observed {
        0 => "not_registered",
        1 => "enabled",
        2 => "requires_approval",
        3 => "not_found",
        _ => bail!("unknown Mac registration state"),
    };
    if observed == 1 {
        observe_job(&main, expected_channel)?;
    }
    if observed == 0 {
        ensure!(
            observe_job(&main, expected_channel)?.is_none(),
            "launchd job remains without verifiable Service Management ownership; preserve it for diagnosis"
        );
    }
    Ok(serde_json::json!({"version":1,"runtime":identity,"channel":expected_channel,"state":state}))
}
pub(crate) fn target_main(root: &Path, identity: &str) -> Result<PathBuf> {
    approved_main(root, identity, false)
}
pub(crate) fn foreground_root(expected_channel: RuntimeChannel) -> Result<PathBuf> {
    let current = std::env::current_exe()?.canonicalize()?;
    let payload = runtime_pair::payload_root(&current)?;
    let identity = payload
        .file_name()
        .and_then(|name| name.to_str())
        .context("Mac login payload identity missing")?;
    digest(identity)?;
    let runtimes = payload.parent().context("Mac runtimes parent missing")?;
    ensure!(
        runtimes.file_name().is_some_and(|name| name == "runtimes"),
        "Mac login is not installed in owned runtime storage"
    );
    let root = runtimes
        .parent()
        .context("Mac installation root missing")?
        .to_path_buf();
    ensure!(
        channel(&root)? == expected_channel,
        "Mac foreground channel differs"
    );
    crate::service_manager::verify_macos_login_intent(&root, identity, expected_channel)?;
    let (selected, _) = startup_selected(&root)?;
    ensure!(
        selected.canonicalize()? == current,
        "Mac login entry is not the active startup candidate"
    );
    Ok(root)
}
pub(crate) fn quiet(root: &Path) -> Result<bool> {
    match quiescent(root) {
        Ok(guard) => {
            drop(guard);
            Ok(true)
        }
        Err(error)
            if matches!(
                error.downcast_ref::<std::fs::TryLockError>(),
                Some(std::fs::TryLockError::WouldBlock)
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launchd_scope_refuses_foreign_origin_arguments_and_nested_or_duplicate_identity()
    -> Result<()> {
        let main = Path::new("/fixture path/é/AIRK.app/Contents/MacOS/airk");
        let plist = Path::new(
            "/fixture path/é/AIRK.app/Contents/Library/LaunchAgents/io.iokaio.airk.service.preview.plist",
        );
        let text = format!(
            "gui/501/io.iokaio.airk.service.preview = {{\n\tpath = {}\n\tprogram = {}\n\targuments = {{\n\t\tairk\n\t\truntime\n\t\tlogin-foreground\n\t\t--channel\n\t\tpreview\n\t}}\n\tenvironment = {{\n\t\tprogram = /foreign/nested\n\t}}\n}}\n",
            plist.display(),
            main.display()
        );
        job_scope(&text, main, plist, RuntimeChannel::Preview)?;
        assert!(job_scope(&text, main, plist, RuntimeChannel::Stable).is_err());
        assert!(
            job_scope(
                &text.replace(&main.display().to_string(), "/foreign/airk"),
                main,
                plist,
                RuntimeChannel::Preview
            )
            .is_err()
        );
        assert!(
            job_scope(
                &text.replace(&plist.display().to_string(), "/foreign/agent.plist"),
                main,
                plist,
                RuntimeChannel::Preview
            )
            .is_err()
        );
        assert!(
            job_scope(
                &text.replace("\t\tlogin-foreground", "\t\tserve"),
                main,
                plist,
                RuntimeChannel::Preview
            )
            .is_err()
        );
        assert!(
            job_scope(
                &text.replace("\targuments = {", "\targuments = {\n\t}\n\targuments = {"),
                main,
                plist,
                RuntimeChannel::Preview
            )
            .is_err()
        );
        let nested_only = text.replacen(&format!("\tprogram = {}\n", main.display()), "", 1);
        assert!(job_scope(&nested_only, main, plist, RuntimeChannel::Preview).is_err());
        assert!(job_scope(&format!("{text}}}"), main, plist, RuntimeChannel::Preview).is_err());
        assert!(job_scope(&"x".repeat(65_537), main, plist, RuntimeChannel::Preview).is_err());
        Ok(())
    }
}
