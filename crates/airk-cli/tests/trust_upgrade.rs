// SPDX-License-Identifier: Apache-2.0
//! Local trust plan C (23 November 2026): an installation an HTTPS build left
//! becomes a working plain-HTTP one. On a real reviewed service the test
//! stops the service, rewrites its control folder into the old shape (the
//! `https://` endpoint record with its CA and leaf digests, the CA, the TLS
//! bundle, a retired key, a pending rotation journal and an `installed`
//! ownership receipt), runs setup's legacy trust retirement with an injected
//! trust runner (the OS store is never touched), restarts the service and
//! checks that:
//! - exactly the receipt's CA was removed, by its DER SHA-256, from
//!   CurrentUser\Root, and the TLS files are gone by exact name;
//! - the same port now serves plain HTTP, as the next endpoint generation
//!   chained to the old record's digest;
//! - an agent config naming the old `https://` URL is listed as an endpoint
//!   mismatch and `airkctl agents repair` rewrites it to the live `http://`
//!   URL, stripping its old bearer and helper fields;
//! - a receipt in phase `prepared` is reported as residue and not acted on.
mod common;
use airk_cli::http_endpoint::legacy_trust::{self, LegacyTrust};
use airk_core::{
    identity::framed_digest,
    service::{ControlMethod, ControlResponse},
    service_http::EndpointDescriptor,
};
use airk_process::cert_trust::Host;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

const CA: &str = "5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a5a";

fn endpoint(service: &common::Service) -> Result<EndpointDescriptor> {
    match service.request(ControlMethod::HttpEndpoint) {
        ControlResponse::HttpEndpoint {
            endpoint: Some(endpoint),
        } => Ok(*endpoint),
        other => anyhow::bail!("no HTTP endpoint: {}", serde_json::to_string(&other)?),
    }
}

fn names(root: &Path) -> Result<Vec<String>> {
    let mut names = std::fs::read_dir(root)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

/// Write an owner-private record the way the old build did.
fn private(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let replace = root.join(name).exists();
    airk_cli::ipc::private_record::write_private(root, name, bytes, 65_536, replace)
}

/// One plain-HTTP request on a fresh loopback connection.
fn get_tools(port: u16) -> Result<String> {
    let mut socket = std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))?;
    socket.set_read_timeout(Some(std::time::Duration::from_secs(30)))?;
    write!(
        socket,
        "GET /api/v1/tools HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )?;
    let mut reply = String::new();
    socket.take(4 << 20).read_to_string(&mut reply)?;
    Ok(reply)
}

#[test]
fn an_old_https_installation_upgrades_to_plain_http_and_its_owned_ca_is_removed() -> Result<()> {
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    let service = common::Service::reviewed(source.path(), data.path(), "trust-upgrade");
    let control: PathBuf = service.control.clone();
    let root = control.join("http-1");
    let before = endpoint(&service)?;
    let port = before.url.port();
    assert_eq!(
        before.url.to_string(),
        format!("http://127.0.0.1:{port}/mcp")
    );
    let user = airk_cli::local_ipc::current_user()?;

    // The old build's shape, written while the service is stopped.
    let mut retired: Option<LegacyTrust> = None;
    let mut scripts = Vec::new();
    let legacy_bytes = service.with_stopped_owner(|_| {
        let mut old = serde_json::to_value(endpoint_on_disk(&root)?)?;
        old["url"] = json!(format!("https://127.0.0.1:{port}/mcp"));
        // Field order of the old descriptor: ca and leaf digests after the URL.
        let old = json!({
            "schema": old["schema"], "installation": old["installation"], "user": old["user"],
            "namespace": old["namespace"], "generation": old["generation"],
            "service_instance": old["service_instance"],
            "executable_sha256": old["executable_sha256"], "url": old["url"],
            "ca_sha256": CA, "certificate_sha256": "6b".repeat(32),
            "state": "stopping", "predecessor_digest": old["predecessor_digest"],
        });
        let bytes = serde_json::to_vec(&old)?;
        private(&root, "endpoint.json", &bytes)?;
        private(
            &root,
            "ca.pem",
            b"-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----\n",
        )?;
        private(
            &root,
            "tls-identity.json",
            br#"{"schema":1,"private":"key"}"#,
        )?;
        private(&root, "retired-private-key.json", b"{}")?;
        private(&root, "issuer-rotation.json", br#"{"phase":"stopping"}"#)?;
        private(&root, "issuer-rotation.lock", b"")?;
        private(&root, "user-trust-1.lock", b"")?;
        let receipt = json!({"schema":1,"binding":{"installation":before.installation,
            "user":user,"namespace":before.namespace},"ca_sha256":CA,
            "store":"CurrentUser/Root","phase":"installed"});
        private(&root, "user-trust-1.json", &serde_json::to_vec(&receipt)?)?;
        // Setup's upgrade step, with the OS trust command injected.
        retired = Some(legacy_trust::retire_with(
            &control,
            Host::Windows,
            &mut |_, args| {
                scripts.push(args.last().cloned().unwrap_or_default());
                Ok((true, r#"{"outcome":"removed"}"#.to_owned()))
            },
        )?);
        Ok(bytes)
    })?;
    let retired = retired.context("retirement did not run")?;
    assert_eq!(scripts.len(), 1, "exactly one trust command");
    assert!(
        scripts[0].contains("('Root','CurrentUser')"),
        "{}",
        scripts[0]
    );
    assert!(
        scripts[0].ends_with(&format!("'{CA}'")),
        "removal by the receipt's digest"
    );
    assert_eq!(retired.certificate, Some((CA.to_owned(), "removed")));
    assert!(retired.residue.is_empty(), "{:?}", retired.residue);
    for gone in [
        "ca.pem",
        "tls-identity.json",
        "retired-private-key.json",
        "issuer-rotation.json",
        "issuer-rotation.lock",
        "user-trust-1.json",
        "user-trust-1.lock",
    ] {
        assert!(
            retired.deleted.contains(&gone),
            "{gone}: {:?}",
            retired.deleted
        );
    }

    // The restarted service carries the legacy record forward over HTTP.
    let after = endpoint(&service)?;
    assert_eq!(after.url.port(), port, "the same port is kept");
    assert_eq!(
        after.url.to_string(),
        format!("http://127.0.0.1:{port}/mcp")
    );
    assert_eq!(after.generation, before.generation + 1);
    assert_eq!(
        after.predecessor_digest,
        Some(framed_digest(b"airk-http-endpoint/1", &[&legacy_bytes]))
    );
    let names = names(&root)?;
    for gone in ["ca.pem", "tls-identity.json", "user-trust-1.json"] {
        assert!(!names.iter().any(|name| name == gone), "{names:?}");
    }
    let reply = get_tools(port)?;
    ensure!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    ensure!(reply.contains("airk.local-api/1"), "{reply}");

    // An agent config the old build wrote is AIRK's and is repaired.
    // The retained-edit fixture of the live agents test (tools.rs): an
    // owner-private project in an isolated Git repository, so repair can keep
    // its recovery copy under the ignored `.airk/`.
    let project = tempfile::tempdir()?;
    let folder = project.path().canonicalize()?.join("private");
    airk_cli::local_ipc::prepare_root(&folder)?;
    let folder = PathBuf::from(
        folder
            .to_str()
            .context("non-UTF8 project")?
            .strip_prefix(r"\\?\")
            .unwrap_or(folder.to_str().context("non-UTF8 project")?),
    );
    let git = std::env::split_paths(&std::env::var_os("PATH").context("PATH")?)
        .map(|directory| directory.join(if cfg!(windows) { "git.exe" } else { "git" }))
        .find(|candidate| candidate.is_absolute() && candidate.is_file())
        .context("Git is required for the retained-edit fixture")?
        .canonicalize()?;
    let git_run = |arguments: &[&str]| -> Result<()> {
        use airk_process::bounded_command::{Invocation, execute_with_timeout};
        let mut args = vec![
            "--no-optional-locks",
            "-c",
            "core.fsmonitor=false",
            "-C",
            folder.to_str().context("non-UTF8 project")?,
        ];
        args.extend_from_slice(arguments);
        let output = execute_with_timeout(
            &Invocation::new(&git, &args),
            std::time::Duration::from_secs(5),
        )?;
        ensure!(output.success, "fixture Git setup failed: {}", output.text);
        Ok(())
    };
    git_run(&["init", "--quiet", "--template="])?;
    let excludes = folder.join("empty-excludes");
    std::fs::write(&excludes, b"")?;
    git_run(&[
        "config",
        "core.excludesFile",
        excludes.to_str().context("excludes")?,
    ])?;
    std::fs::write(folder.join(".gitignore"), b".airk/\n")?;
    std::fs::write(
        folder.join("package.json"),
        br#"{"name":"airk-trust-upgrade-fixture","version":"0.0.0","packageManager":"npm@11.12.1","files":["payload"],"main":"payload/main.js"}"#,
    )?;
    std::fs::create_dir(folder.join("payload"))?;
    std::fs::write(folder.join("payload/main.js"), b"fixture\n")?;
    let file = folder.join(".mcp.json");
    let old_url = format!("https://127.0.0.1:{port}/mcp");
    std::fs::write(
        &file,
        serde_json::to_string_pretty(&json!({"mcpServers":{"airk":{"type":"http","url":old_url,
            "headers":{"X-AIRK-Response-Mode":"terse","Authorization":"Bearer retired"},
            "headersHelper":"retired airkctl agents credential --connection-id <connection-id> --format header-json"},
            "other":{"command":"node","args":["x"]}}}))?,
    )?;
    let ctl = |arguments: &[&str]| -> Result<Value> {
        let output = common::ctl::Ctl::new()
            .args(["--output", "json", "--service-runtime-dir"])
            .arg(&control)
            .arg("agents")
            .args(arguments)
            .output()?;
        ensure!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(serde_json::from_slice(&output.stdout)?)
    };
    let folder_text = folder.to_str().context("non-UTF8 project")?;
    let listed = ctl(&["list", "--project", folder_text, "--no-user"])?;
    let findings = listed["findings"].as_array().context("findings")?;
    assert_eq!(findings.len(), 1, "{listed}");
    assert_eq!(findings[0]["url"], old_url.as_str(), "{listed}");
    let repaired = ctl(&["repair", "--project", folder_text, "--no-user"])?;
    assert_eq!(repaired["actions"][0]["changed"], true, "{repaired}");
    let text: Value = serde_json::from_str(&std::fs::read_to_string(&file)?)?;
    assert_eq!(text["mcpServers"]["airk"]["url"], after.url.to_string());
    assert!(text["mcpServers"]["airk"].get("headersHelper").is_none());
    assert!(
        text["mcpServers"]["airk"]["headers"]
            .get("Authorization")
            .is_none()
    );
    assert_eq!(
        text["mcpServers"]["other"],
        json!({"command":"node","args":["x"]})
    );
    // Now current: the live endpoint, no finding.
    let listed = ctl(&["list", "--project", folder_text, "--no-user"])?;
    assert_eq!(listed["findings"], json!([]), "{listed}");

    // A receipt of an interrupted trust change is residue, never acted on.
    let receipt = json!({"schema":1,"binding":{"installation":after.installation,
        "user":user,"namespace":after.namespace},"ca_sha256":CA,
        "store":"CurrentUser/Root","phase":"prepared"});
    private(&root, "user-trust-1.json", &serde_json::to_vec(&receipt)?)?;
    private(
        &root,
        "ca.pem",
        b"-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----\n",
    )?;
    let residue = legacy_trust::retire_with(&control, Host::Windows, &mut |_, _| {
        anyhow::bail!("an interrupted receipt must not run a trust command")
    })?;
    assert_eq!(residue.certificate, None);
    assert!(
        residue.residue[0].contains("unknown outcome"),
        "{:?}",
        residue.residue
    );
    let names = self::names(&root)?;
    assert!(
        names.iter().any(|name| name == "user-trust-1.json"),
        "{names:?}"
    );
    assert!(names.iter().any(|name| name == "ca.pem"), "{names:?}");
    // The listener is unaffected.
    ensure!(get_tools(port)?.starts_with("HTTP/1.1 200"));
    service.shutdown_reviewed()?;
    Ok(())
}

/// The current record on disk (read while the service is stopped).
fn endpoint_on_disk(root: &Path) -> Result<EndpointDescriptor> {
    let bytes = airk_cli::http_endpoint::read_private(root, "endpoint.json", 4096)?
        .context("endpoint record missing")?;
    EndpointDescriptor::decode(&bytes)
}
