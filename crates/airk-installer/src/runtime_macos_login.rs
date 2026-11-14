// SPDX-License-Identifier: Apache-2.0
//! Run SMAppService only from the exact approved immutable application bundle.
use super::*;
use std::ffi::CString;

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn airk_macos_login(
        action: std::ffi::c_int,
        split: std::ffi::c_int,
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
    Ok(airk_host::install_layout::retained_parameters(root)?.1)
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
pub(crate) const LEGACY_ENTRY: &str = "login-foreground";
pub(crate) const DIRECT_ENTRY: &str = "login-serve";

fn entry_for_version(version: &str) -> Result<&'static str> {
    ensure!(version.len() <= 64, "invalid Mac login runtime version");
    let numbers = version
        .split('.')
        .map(|part| {
            ensure!(
                !part.is_empty()
                    && part.bytes().all(|byte| byte.is_ascii_digit())
                    && (part.len() == 1 || !part.starts_with('0')),
                "invalid Mac login runtime version"
            );
            Ok(part.parse::<u64>()?)
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(numbers.len() == 3, "invalid Mac login runtime version");
    Ok(if numbers.as_slice() >= [1, 8, 14].as_slice() {
        DIRECT_ENTRY
    } else {
        LEGACY_ENTRY
    })
}

pub(crate) fn entry(root: &Path, identity: &str) -> Result<&'static str> {
    let directory = payload_path(root, &format!("runtimes/{identity}/airk-install.json"))?
        .parent()
        .context("Mac payload parent missing")?
        .to_path_buf();
    let (approval, _) = saved_payload_identity(&directory, identity)?;
    entry_for_version(&approval.version)
}

fn job_scope(
    text: &str,
    main: &Path,
    plist: &Path,
    channel: RuntimeChannel,
    entry: &str,
) -> Result<()> {
    ensure!(
        matches!(entry, LEGACY_ENTRY | DIRECT_ENTRY),
        "invalid Mac login entry"
    );
    verify_job_scope(
        text,
        main,
        plist,
        &["airk", "runtime", entry, "--channel", channel_name(channel)],
    )
}
pub(crate) fn verify_job_scope(
    text: &str,
    main: &Path,
    plist: &Path,
    expected_arguments: &[&str],
) -> Result<()> {
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
                arguments.len() < 32 && value.len() <= 4096,
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
            && arguments == expected_arguments,
        "Mac job origin/program/arguments differ from captured ownership"
    );
    Ok(())
}
fn job_pid(text: &str) -> Result<Option<u32>> {
    ensure!(
        text.len() <= 65_536,
        "Mac launchd observation exceeds budget"
    );
    let mut depth = 0isize;
    let mut pid = None;
    for line in text.lines() {
        let value = line.trim();
        if depth == 1
            && let Some(value) = value.strip_prefix("pid = ")
        {
            let observed = value.parse::<u32>()?;
            ensure!(
                observed != 0 && pid.replace(observed).is_none(),
                "invalid or duplicate Mac job PID"
            );
        }
        if value.ends_with('{') {
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
    ensure!(depth == 0, "unsupported Mac job observation structure");
    Ok(pid)
}

fn observe_job(main: &Path, channel: RuntimeChannel, entry: &str) -> Result<Option<String>> {
    let domain = format!("gui/{}", crate::local_ipc::current_user()?);
    let (domain_available, _) = airk_host::native_command::execute_native_observation(
        Path::new("/bin/launchctl"),
        &["print", &domain],
    )?;
    ensure!(
        domain_available,
        "Mac GUI user domain is unavailable; use the explicit direct foreground workflow"
    );
    let label = format!("io.iokaio.airk.service.{}", channel_name(channel));
    let target = format!("{domain}/{label}");
    let (present, text) = airk_host::native_command::execute_native_observation(
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
    job_scope(&text, main, &plist, channel, entry)?;
    Ok(Some(text))
}

pub(crate) struct ObservedJob {
    pub process_id: Option<u32>,
}

pub(crate) fn observed_job(
    root: &Path,
    identity: &str,
    channel: RuntimeChannel,
) -> Result<Option<ObservedJob>> {
    let main = approved_main(root, identity, false)?;
    observe_job(&main, channel, entry(root, identity)?)?
        .map(|text| {
            Ok(ObservedJob {
                process_id: job_pid(&text)?,
            })
        })
        .transpose()
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
    airk_host::install_layout::saved_control(&root)?;
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
    let output = airk_host::native_command::execute_native_program(
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
pub fn native(
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
    let entry = entry(root, identity)?;
    let prior = observe_job(&main, expected_channel, entry)?;
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
            0,
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
        observe_job(&main, expected_channel, entry)?;
    }
    if observed == 0 {
        ensure!(
            observe_job(&main, expected_channel, entry)?.is_none(),
            "launchd job remains without verifiable Service Management ownership; preserve it for diagnosis"
        );
    }
    Ok(serde_json::json!({"version":1,"runtime":identity,"channel":expected_channel,"state":state}))
}
pub(crate) fn target_main(root: &Path, identity: &str) -> Result<PathBuf> {
    approved_main(root, identity, false)
}
pub fn foreground_root(expected_channel: RuntimeChannel) -> Result<PathBuf> {
    startup_root(expected_channel, LEGACY_ENTRY, false)
}

pub fn direct_root(expected_channel: RuntimeChannel) -> Result<PathBuf> {
    startup_root(expected_channel, DIRECT_ENTRY, true)
}

fn startup_root(
    expected_channel: RuntimeChannel,
    expected_entry: &str,
    require_manager_process: bool,
) -> Result<PathBuf> {
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
    ensure!(
        entry(&root, identity)? == expected_entry,
        "Mac login command differs from the approved runtime layout"
    );
    let (selected, _) = startup_selected(&root)?;
    ensure!(
        selected.canonicalize()? == current,
        "Mac login entry is not the active startup candidate"
    );
    if require_manager_process {
        let text = observe_job(&current, expected_channel, DIRECT_ENTRY)?
            .context("Mac direct service has no observed owned launchd job")?;
        ensure!(
            job_pid(&text)? == Some(std::process::id()),
            "Mac direct service is not the process launched by the captured registration"
        );
    }
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
    fn sealed_entry_retains_predecessor_definitions_and_rejects_noncanonical_versions() -> Result<()>
    {
        for version in ["1.4.0", "1.8.7", "1.8.13"] {
            assert_eq!(entry_for_version(version)?, LEGACY_ENTRY);
        }
        for version in ["1.8.14", "1.9.0", "2.0.0"] {
            assert_eq!(entry_for_version(version)?, DIRECT_ENTRY);
        }
        for version in [
            "",
            "1.8",
            "1.8.14.0",
            "01.8.14",
            "1.8.+14",
            "1.8.14-beta",
            "1.8.18446744073709551616",
        ] {
            assert!(entry_for_version(version).is_err(), "{version}");
        }
        Ok(())
    }
    #[test]
    fn launchd_pid_uses_only_the_unique_top_level_process() -> Result<()> {
        assert_eq!(
            job_pid("job = {\n pid = 42\n child = {\n pid = 99\n }\n}\n")?,
            Some(42)
        );
        assert_eq!(job_pid("job = {\n child = {\n pid = 99\n }\n}\n")?, None);
        for text in [
            "job = {\n pid = 0\n}\n",
            "job = {\n pid = 42\n pid = 42\n}\n",
            "job = {\n pid = nope\n}\n",
            "job = {\n pid = 42\n",
        ] {
            assert!(job_pid(text).is_err());
        }
        Ok(())
    }
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
        job_scope(&text, main, plist, RuntimeChannel::Preview, LEGACY_ENTRY)?;
        let direct = text.replace("login-foreground", "login-serve");
        job_scope(&direct, main, plist, RuntimeChannel::Preview, DIRECT_ENTRY)?;
        assert!(job_scope(&direct, main, plist, RuntimeChannel::Preview, LEGACY_ENTRY).is_err());
        assert!(job_scope(&text, main, plist, RuntimeChannel::Preview, DIRECT_ENTRY).is_err());
        assert!(job_scope(&text, main, plist, RuntimeChannel::Stable, LEGACY_ENTRY).is_err());
        assert!(
            job_scope(
                &text.replace(&main.display().to_string(), "/foreign/airk"),
                main,
                plist,
                RuntimeChannel::Preview,
                LEGACY_ENTRY
            )
            .is_err()
        );
        assert!(
            job_scope(
                &text.replace(&plist.display().to_string(), "/foreign/agent.plist"),
                main,
                plist,
                RuntimeChannel::Preview,
                LEGACY_ENTRY
            )
            .is_err()
        );
        assert!(
            job_scope(
                &text.replace("\t\tlogin-foreground", "\t\tserve"),
                main,
                plist,
                RuntimeChannel::Preview,
                LEGACY_ENTRY
            )
            .is_err()
        );
        assert!(
            job_scope(
                &text.replace("\targuments = {", "\targuments = {\n\t}\n\targuments = {"),
                main,
                plist,
                RuntimeChannel::Preview,
                LEGACY_ENTRY
            )
            .is_err()
        );
        let nested_only = text.replacen(&format!("\tprogram = {}\n", main.display()), "", 1);
        assert!(
            job_scope(
                &nested_only,
                main,
                plist,
                RuntimeChannel::Preview,
                LEGACY_ENTRY
            )
            .is_err()
        );
        assert!(
            job_scope(
                &format!("{text}}}"),
                main,
                plist,
                RuntimeChannel::Preview,
                LEGACY_ENTRY
            )
            .is_err()
        );
        assert!(
            job_scope(
                &"x".repeat(65_537),
                main,
                plist,
                RuntimeChannel::Preview,
                LEGACY_ENTRY
            )
            .is_err()
        );
        Ok(())
    }
}
