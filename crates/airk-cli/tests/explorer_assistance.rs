// SPDX-License-Identifier: Apache-2.0
//! The Explorer Assistance forwarder hands stdio to the extension's server and nothing else:
//! no service, store or AIRK state is touched, and the server's exit code is the CLI's.
use anyhow::{Result, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn node() -> Option<PathBuf> {
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Fixture {
    directory: tempfile::TempDir,
    descriptor: PathBuf,
}

fn fixture(runtime: &Path, server_source: &str) -> Result<Fixture> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("extension");
    fs::create_dir_all(root.join("out"))?;
    fs::write(
        root.join("package.json"),
        json!({"publisher":"iokaio","name":"airk","version":"1.5.0"}).to_string(),
    )?;
    let server = root.join("out").join("explorerAssistance.js");
    fs::write(&server, server_source)?;
    let storage = directory.path().join("storage").join("explorer-assistance");
    fs::create_dir_all(&storage)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&storage, fs::Permissions::from_mode(0o700))?;
    }
    let descriptor = storage.join("launch-1.json");
    fs::write(
        &descriptor,
        json!({"schema":"airk-explorer-assistance-launch/1","extension_version":"1.5.0","extension_root":root,"server":server,
            "server_sha256":hex(server_source.as_bytes()),"runtime":{"path":runtime,"electron":false},"written_unix_ms":1})
        .to_string(),
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&descriptor, fs::Permissions::from_mode(0o600))?;
    }
    Ok(Fixture {
        directory,
        descriptor,
    })
}

/// An isolated invocation: empty home and state directories, a never-installed service root.
fn command(fixture: &Fixture, home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_airk"));
    command
        .arg("--service-install-dir")
        .arg(fixture.directory.path().join("never-installed"))
        .args(["explorer-assistance", "--descriptor"])
        .arg(&fixture.descriptor)
        .current_dir(fixture.directory.path())
        .env("NODE_OPTIONS", "--require=./should-not-load.js")
        .env("ELECTRON_RUN_AS_NODE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for variable in [
        "HOME",
        "USERPROFILE",
        "APPDATA",
        "LOCALAPPDATA",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
    ] {
        command.env(variable, home);
    }
    command
}

#[test]
fn the_forwarder_hands_stdio_to_the_bundled_server_and_returns_its_exit_code() -> Result<()> {
    let Some(node) = node() else {
        println!("skipped: node is not on PATH; the forwarder hand-off needs a JavaScript runtime");
        return Ok(());
    };
    let server = "const rl=require('node:readline').createInterface({input:process.stdin});\
        rl.once('line',line=>{process.stdout.write(JSON.stringify({echo:line,args:process.argv.slice(2),cwd:process.cwd(),node_options:process.env.NODE_OPTIONS??null,electron:process.env.ELECTRON_RUN_AS_NODE??null})+'\\n');process.exit(7);});";
    let fixture = fixture(&node, server)?;
    let home = fixture.directory.path().join("home");
    fs::create_dir_all(&home)?;
    let mut child = command(&fixture, &home).spawn()?;
    child.stdin.take().unwrap().write_all(b"hello-explorer\n")?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.code() == Some(7),
        "exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(reply["echo"], "hello-explorer");
    assert_eq!(reply["args"][0], "--assistance-dir");
    assert_eq!(
        fs::canonicalize(reply["args"][1].as_str().unwrap())?,
        fs::canonicalize(fixture.descriptor.parent().unwrap())?
    );
    assert_eq!(
        fs::canonicalize(reply["cwd"].as_str().unwrap())?,
        fs::canonicalize(fixture.directory.path())?
    );
    assert_eq!(reply["node_options"], serde_json::Value::Null);
    assert_eq!(
        reply["electron"],
        serde_json::Value::Null,
        "a non-Electron runtime does not get ELECTRON_RUN_AS_NODE"
    );
    assert_eq!(
        fs::read_dir(&home)?.count(),
        0,
        "no AIRK user state was created"
    );
    assert!(
        !fixture.directory.path().join("never-installed").exists(),
        "the service installation was never touched"
    );
    Ok(())
}

#[test]
fn a_rejected_descriptor_never_starts_the_runtime() -> Result<()> {
    // The "runtime" here would leave a marker if it ever ran.
    let directory = tempfile::tempdir()?;
    let marker = directory.path().join("ran");
    let runtime = if cfg!(windows) {
        std::env::var_os("COMSPEC").map(PathBuf::from)
    } else {
        Some(PathBuf::from("/bin/sh"))
    };
    let Some(runtime) = runtime.filter(|path| path.is_file()) else {
        return Ok(());
    };
    let fixture = fixture(&runtime, &format!("touch {}", marker.display()))?;
    // Tamper with the server after the descriptor recorded its digest.
    let server = fixture
        .directory
        .path()
        .join("extension")
        .join("out")
        .join("explorerAssistance.js");
    fs::write(&server, "changed")?;
    let home = fixture.directory.path().join("home");
    fs::create_dir_all(&home)?;
    let output = command(&fixture, &home).stdin(Stdio::null()).output()?;
    ensure!(
        !output.status.success(),
        "a tampered server must be refused"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("explorer_assistance_server_mismatch"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.exists());
    assert!(
        output.stdout.is_empty(),
        "nothing is written to the MCP stream on refusal"
    );
    Ok(())
}
