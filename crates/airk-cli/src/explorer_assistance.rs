// SPDX-License-Identifier: Apache-2.0
//! Start the VS Code extension's Explorer Assistance MCP server for an external agent.
//!
//! The extension writes a launch descriptor naming its bundled server and the
//! JavaScript runtime VS Code itself uses. This module only validates that
//! descriptor and hands the agent's stdio to that process. It never opens project
//! stores, contacts the AIRK service or calls retrieval, so no MCP bytes pass
//! through AIRK code and no system Node installation is needed.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const SCHEMA: &str = "airk-explorer-assistance-launch/1";
const DESCRIPTOR_LIMIT: u64 = 16 * 1024;
const PACKAGE_LIMIT: u64 = 1024 * 1024;
const SERVER_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    schema: String,
    extension_version: String,
    extension_root: PathBuf,
    server: PathBuf,
    server_sha256: String,
    runtime: Runtime,
    written_unix_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Runtime {
    path: PathBuf,
    /// VS Code's Electron executable runs as Node only with ELECTRON_RUN_AS_NODE.
    electron: bool,
}

/// A validated hand-off: the runtime executable runs exactly the bundled server.
#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub runtime: PathBuf,
    pub server: PathBuf,
    pub electron: bool,
    /// The descriptor's directory; the server reads per-window attachment descriptors below it.
    pub assistance_dir: PathBuf,
}

fn regular_file(path: &Path, limit: u64, what: &str) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("explorer_assistance_{what}_unavailable"))?;
    ensure!(
        metadata.file_type().is_file(),
        "explorer_assistance_{what}_not_a_regular_file"
    );
    ensure!(
        metadata.len() <= limit,
        "explorer_assistance_{what}_oversize"
    );
    Ok(())
}

fn read_bounded(path: &Path, limit: u64, what: &str) -> Result<Vec<u8>> {
    regular_file(path, limit, what)?;
    let bytes =
        fs::read(path).with_context(|| format!("explorer_assistance_{what}_unavailable"))?;
    ensure!(
        bytes.len() as u64 <= limit,
        "explorer_assistance_{what}_oversize"
    );
    Ok(bytes)
}

/// Owner-only descriptor and directory: another local user cannot redirect the launch.
#[cfg(unix)]
fn owner_only(descriptor: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: geteuid takes no pointers and has no failure mode.
    let user = unsafe { libc::geteuid() };
    for (path, directory) in [
        (descriptor, false),
        (
            descriptor
                .parent()
                .context("explorer_assistance_descriptor_invalid")?,
            true,
        ),
    ] {
        let metadata = fs::symlink_metadata(path)?;
        ensure!(
            metadata.uid() == user && metadata.mode() & 0o022 == 0,
            "explorer_assistance_descriptor_invalid: {} must be owned by this user and not group or world writable",
            if directory { "directory" } else { "descriptor" }
        );
    }
    Ok(())
}

/// Validate the descriptor and resolve exactly what will run. Nothing is spawned here.
pub fn plan(descriptor: &Path) -> Result<Launch> {
    ensure!(
        descriptor.is_absolute(),
        "explorer_assistance_descriptor_invalid: the descriptor path must be absolute"
    );
    ensure!(
        descriptor.file_name() == Some("launch-1.json".as_ref())
            && descriptor.parent().and_then(Path::file_name)
                == Some("explorer-assistance".as_ref()),
        "explorer_assistance_descriptor_invalid: unexpected descriptor location (expected …/explorer-assistance/launch-1.json)"
    );
    let bytes = read_bounded(descriptor, DESCRIPTOR_LIMIT, "descriptor")?;
    #[cfg(unix)]
    owner_only(descriptor)?;
    let value: Descriptor =
        serde_json::from_slice(&bytes).context("explorer_assistance_descriptor_invalid")?;
    ensure!(
        value.schema == SCHEMA,
        "explorer_assistance_descriptor_invalid: unsupported schema"
    );
    ensure!(
        !value.extension_version.is_empty()
            && value.extension_version.len() <= 64
            && value.written_unix_ms > 0,
        "explorer_assistance_descriptor_invalid"
    );
    ensure!(
        value.extension_root.is_absolute()
            && value.server.is_absolute()
            && value.runtime.path.is_absolute(),
        "explorer_assistance_descriptor_invalid: paths must be absolute"
    );
    let root = fs::canonicalize(&value.extension_root)
        .context("explorer_assistance_extension_unavailable: open VS Code once to refresh the launch descriptor")?;
    let manifest: serde_json::Value = serde_json::from_slice(&read_bounded(
        &root.join("package.json"),
        PACKAGE_LIMIT,
        "package",
    )?)
    .context("explorer_assistance_package_mismatch")?;
    ensure!(
        manifest["publisher"] == "iokaio"
            && manifest["name"] == "airk"
            && manifest["version"] == value.extension_version.as_str(),
        "explorer_assistance_package_mismatch: the descriptor does not name this AIRK extension"
    );
    let server = fs::canonicalize(&value.server)
        .context("explorer_assistance_server_unavailable: open VS Code once to refresh the launch descriptor")?;
    ensure!(
        server == root.join("out").join("explorerAssistance.js"),
        "explorer_assistance_server_mismatch: unexpected server location"
    );
    let digest = Sha256::digest(read_bounded(&server, SERVER_LIMIT, "server")?);
    let expected = value.server_sha256.to_ascii_lowercase();
    ensure!(
        expected.len() == 64
            && digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
                == expected,
        "explorer_assistance_server_mismatch: digest differs from the descriptor"
    );
    let runtime = fs::canonicalize(&value.runtime.path)
        .context("explorer_assistance_runtime_unavailable: open VS Code once to refresh the launch descriptor")?;
    regular_file(&runtime, u64::MAX, "runtime")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&runtime)?.permissions().mode();
        ensure!(
            mode & 0o111 != 0 && mode & 0o022 == 0,
            "explorer_assistance_runtime_unavailable: the runtime must be executable and not group or world writable"
        );
    }
    let assistance_dir = fs::canonicalize(descriptor.parent().unwrap_or(descriptor))?;
    Ok(Launch {
        runtime,
        server,
        electron: value.runtime.electron,
        assistance_dir,
    })
}

/// The exact process: runtime + server + its state directory, inherited stdio and working directory, no Node options.
pub fn command(launch: &Launch) -> Command {
    let mut command = Command::new(&launch.runtime);
    command
        .arg(&launch.server)
        .arg("--assistance-dir")
        .arg(&launch.assistance_dir)
        .env_remove("NODE_OPTIONS")
        .env_remove("NODE_PATH");
    if launch.electron {
        command.env("ELECTRON_RUN_AS_NODE", "1");
    } else {
        command.env_remove("ELECTRON_RUN_AS_NODE");
    }
    command
}

pub fn run(descriptor: &Path) -> Result<()> {
    let launch = plan(descriptor)?;
    crate::native_process::forward_stdio(command(&launch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Fixture {
        _directory: tempfile::TempDir,
        root: PathBuf,
        descriptor: PathBuf,
        server: PathBuf,
        runtime: PathBuf,
    }

    fn fixture() -> Result<Fixture> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("iokaio.airk-1.5.0");
        fs::create_dir_all(root.join("out"))?;
        fs::write(
            root.join("package.json"),
            json!({"publisher":"iokaio","name":"airk","version":"1.5.0"}).to_string(),
        )?;
        let server = root.join("out").join("explorerAssistance.js");
        fs::write(&server, b"process.exit(0)\n")?;
        let runtime = directory.path().join("runtime-node");
        fs::write(&runtime, b"fixture")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755))?;
        }
        let storage = directory.path().join("storage").join("explorer-assistance");
        fs::create_dir_all(&storage)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&storage, fs::Permissions::from_mode(0o700))?;
        }
        let descriptor = storage.join("launch-1.json");
        Ok(Fixture {
            _directory: directory,
            root,
            descriptor,
            server,
            runtime,
        })
    }

    fn write(fixture: &Fixture, patch: impl FnOnce(&mut serde_json::Value)) -> Result<()> {
        let digest = Sha256::digest(fs::read(&fixture.server)?)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let mut value = json!({"schema":SCHEMA,"extension_version":"1.5.0","extension_root":fixture.root,
            "server":fixture.server,"server_sha256":digest,"runtime":{"path":fixture.runtime,"electron":true},"written_unix_ms":1});
        patch(&mut value);
        fs::write(&fixture.descriptor, value.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&fixture.descriptor, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    #[test]
    fn a_valid_descriptor_resolves_the_bundled_server_and_runtime() -> Result<()> {
        let fixture = fixture()?;
        write(&fixture, |_| {})?;
        let launch = plan(&fixture.descriptor)?;
        assert_eq!(launch.server, fs::canonicalize(&fixture.server)?);
        assert_eq!(launch.runtime, fs::canonicalize(&fixture.runtime)?);
        assert!(launch.electron);
        let command = command(&launch);
        let envs: Vec<_> = command.get_envs().collect();
        assert!(envs.contains(&("ELECTRON_RUN_AS_NODE".as_ref(), Some("1".as_ref()))));
        assert!(envs.contains(&("NODE_OPTIONS".as_ref(), None)));
        assert_eq!(
            launch.assistance_dir,
            fs::canonicalize(fixture.descriptor.parent().unwrap())?
        );
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                launch.server.as_os_str(),
                "--assistance-dir".as_ref(),
                launch.assistance_dir.as_os_str()
            ]
        );
        Ok(())
    }

    #[test]
    fn tampered_or_misplaced_descriptors_are_refused_before_anything_runs() -> Result<()> {
        let fixture = fixture()?;
        type Patch = Box<dyn FnOnce(&mut serde_json::Value)>;
        let cases: Vec<(Patch, &str)> = vec![
            (
                Box::new(|value| value["schema"] = json!("other/1")),
                "unsupported schema",
            ),
            (
                Box::new(|value| value["unexpected"] = json!(true)),
                "descriptor_invalid",
            ),
            (
                Box::new(|value| value["server_sha256"] = json!("0".repeat(64))),
                "server_mismatch",
            ),
            (
                Box::new(|value| value["extension_version"] = json!("9.9.9")),
                "package_mismatch",
            ),
            (
                Box::new(|value| value["runtime"]["path"] = json!("relative/node")),
                "must be absolute",
            ),
            (
                Box::new(|value| value["server"] = value["extension_root"].clone()),
                "server_mismatch",
            ),
        ];
        for (patch, expected) in cases {
            write(&fixture, patch)?;
            let error = plan(&fixture.descriptor).unwrap_err();
            assert!(
                format!("{error:#}").contains(expected),
                "{expected}: {error:#}"
            );
        }
        write(&fixture, |_| {})?;
        fs::write(&fixture.server, b"changed")?;
        assert!(
            format!("{:#}", plan(&fixture.descriptor).unwrap_err()).contains("server_mismatch")
        );
        let misplaced = fixture.root.join("launch-1.json");
        assert!(
            format!("{:#}", plan(&misplaced).unwrap_err())
                .contains("unexpected descriptor location")
        );
        assert!(
            format!(
                "{:#}",
                plan(Path::new("explorer-assistance/launch-1.json")).unwrap_err()
            )
            .contains("absolute")
        );
        Ok(())
    }

    #[test]
    fn the_forwarder_never_reaches_service_store_kernel_or_retrieval_code() {
        let source = include_str!("explorer_assistance.rs");
        let module = source.split("#[cfg(test)]").next().unwrap();
        for forbidden in [
            "service_runtime",
            "service_frontend",
            "project_client",
            "local_ipc",
            "airk_store",
            "airk_kernel",
            "airk_mcp",
            "user_paths",
            "service_ipc",
        ] {
            assert!(!module.contains(forbidden), "{forbidden}");
        }
    }
}
