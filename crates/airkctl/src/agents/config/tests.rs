// SPDX-License-Identifier: Apache-2.0
//! The shared cases (`fixtures/split/agents/config_cases.json`, hand-reviewed
//! expectations) and focused checks of the parsers, the U92 helper and the
//! file layer.
use super::*;
use serde_json::{Value, json};
use std::collections::HashMap;

const CASES: &str = include_str!("../../../../../fixtures/split/agents/config_cases.json");
const A: &str = "0123456789abcdef0123456789abcdef";

const LATE_ORIGINAL: &str = "{\"servers\":{\"airk\":{}},\"unrelated\":\"old\"}\n";
const LATE_PREPARED: &str = "{\"servers\":{},\"unrelated\":\"old\"}\n";
const LATE_USER: &str = "{\"servers\":{\"airk\":{}},\"unrelated\":\"user edit\"}\n";

// Distinct approved Option A behavior step. The original failed observations
// remain in their retained diagnostic receipts: native retained publication now
// keeps BOTH exact proposal and displaced user bytes, with the original typed
// private receipt. Deletion and absent-file no-clobber still refuse ordinarily.
fn late_record(
    parent: &std::path::Path,
    state: edit_record::State,
) -> (PathBuf, edit_record::Receipt) {
    use airk_process::directory::{ObjectIdentity, OpenedDirectory};
    use std::fs::File;
    fn identity(record: &edit_record::NativeIdentity, actual: ObjectIdentity) {
        match (record, actual) {
            (
                edit_record::NativeIdentity::Unix { device, inode },
                ObjectIdentity::Unix {
                    device: actual_device,
                    inode: actual_inode,
                },
            ) => assert_eq!((*device, *inode), (actual_device, actual_inode)),
            (
                edit_record::NativeIdentity::Windows {
                    volume_serial,
                    file_id,
                },
                ObjectIdentity::Windows {
                    volume_serial: actual_volume,
                    file_id: actual_id,
                },
            ) => assert_eq!(
                (*volume_serial, *file_id),
                (actual_volume, Incarnation::from_bytes(actual_id))
            ),
            _ => panic!("native platform identity changed"),
        }
    }
    for suffix in [".airk", ".airk/agent-edits", ".airk/agent-edits/v1"] {
        let directory = OpenedDirectory::open(&parent.join(suffix)).unwrap();
        airk_ipc::endpoint::require_private_directory_handle(directory.as_file()).unwrap();
    }
    let root = parent.join(".airk/agent-edits/v1");
    let marker_path = root.join("owner.json");
    let marker = edit_record::RootMarker::decode(&std::fs::read(&marker_path).unwrap()).unwrap();
    let user = airk_ipc::endpoint::current_user().unwrap();
    match &marker.owner {
        edit_record::Owner::Unix { uid } => assert_eq!(uid.to_string(), user),
        edit_record::Owner::Windows { sid } => assert_eq!(*sid, user),
    }
    airk_ipc::endpoint::require_private_metadata_handle(&File::open(marker_path).unwrap()).unwrap();
    identity(
        &marker.root,
        OpenedDirectory::open(&root).unwrap().identity().unwrap(),
    );
    identity(
        &marker.parent,
        OpenedDirectory::open(parent).unwrap().identity().unwrap(),
    );
    let directories: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(
        directories.len(),
        1,
        "exactly the original matching transaction is retained"
    );
    let directory = directories.into_iter().next().unwrap();
    let held = OpenedDirectory::open(&directory).unwrap();
    airk_ipc::endpoint::require_private_directory_handle(held.as_file()).unwrap();
    let path = directory.join("receipt.json");
    airk_ipc::endpoint::require_private_metadata_handle(&File::open(&path).unwrap()).unwrap();
    let receipt = edit_record::Receipt::decode(&std::fs::read(&path).unwrap()).unwrap();
    let transaction = directory
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    receipt
        .validate_binding(&marker, transaction, "mcp.json")
        .unwrap();
    assert_eq!(receipt.operation, EditOperation::Repair);
    assert_eq!(receipt.state, state);
    assert_eq!(receipt.sequence, 2);
    for (snapshot, bytes) in [
        (&receipt.original, LATE_ORIGINAL),
        (&receipt.prepared, LATE_PREPARED),
    ] {
        use sha2::Digest;
        assert_eq!(snapshot.length, bytes.len() as u64);
        assert_eq!(
            snapshot.sha256,
            airk_core::identity::Sha256Digest::from_bytes(
                sha2::Sha256::digest(bytes.as_bytes()).into()
            )
        );
    }
    let after = receipt.after.as_ref().unwrap();
    for (observation, path) in [
        (&after.target, parent.join("mcp.json")),
        (&after.prepared, directory.join("prepared")),
        (&after.before, directory.join("before")),
    ] {
        match observation {
            edit_record::Observation::Absent => assert!(!path.exists(), "{}", path.display()),
            edit_record::Observation::Complete { file } => {
                use sha2::Digest;
                let bytes = std::fs::read(&path).unwrap();
                identity(
                    &file.identity,
                    airk_process::directory::identity(&File::open(&path).unwrap()).unwrap(),
                );
                assert_eq!(file.length, bytes.len() as u64);
                assert_eq!(
                    file.sha256,
                    airk_core::identity::Sha256Digest::from_bytes(
                        sha2::Sha256::digest(&bytes).into()
                    )
                );
            }
            edit_record::Observation::Incomplete { .. } => {
                panic!("stable bounded fixture must retain complete evidence")
            }
        }
    }
    (directory, receipt)
}

fn displaced_path(directory: &std::path::Path, receipt: &edit_record::Receipt) -> PathBuf {
    directory.join(match receipt.primitive {
        edit_record::Primitive::ReplaceFile => "before",
        edit_record::Primitive::RenameExchange | edit_record::Primitive::RenameSwap => "prepared",
    })
}

fn late_native_identity(path: &std::path::Path) -> airk_process::directory::ObjectIdentity {
    airk_process::directory::identity(&std::fs::File::open(path).unwrap()).unwrap()
}

fn late_record_identity(
    record: &edit_record::NativeIdentity,
) -> airk_process::directory::ObjectIdentity {
    match record {
        edit_record::NativeIdentity::Unix { device, inode } => {
            airk_process::directory::ObjectIdentity::Unix {
                device: *device,
                inode: *inode,
            }
        }
        edit_record::NativeIdentity::Windows {
            volume_serial,
            file_id,
        } => airk_process::directory::ObjectIdentity::Windows {
            volume_serial: *volume_serial,
            file_id: *file_id.as_bytes(),
        },
    }
}
#[test]
fn late_config_publication_preserves_an_in_place_user_edit() {
    let directory = writer_test_support::Editable::new();
    let file = directory.path().join("mcp.json");
    std::fs::write(&file, LATE_ORIGINAL).unwrap();
    let original_identity = late_native_identity(&file);
    let result = io::with_before_publish(
        |path| std::fs::write(path, LATE_USER).unwrap(),
        || {
            edit_file_for(
                EditOperation::Repair,
                &file,
                Some(directory.path()),
                false,
                |_| Ok(LATE_PREPARED.into()),
            )
        },
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        LATE_PREPARED,
        "{result:?}"
    );
    assert!(
        result.is_err(),
        "a late user edit must be an explicit conflict"
    );
    let error = result.unwrap_err();
    assert_eq!(error.kind, EditFailureKind::ConflictRetained);
    let (transaction, receipt) =
        late_record(directory.path(), edit_record::State::ConflictRetained);
    assert_eq!(
        late_record_identity(&receipt.original.identity),
        original_identity
    );
    assert_eq!(
        std::fs::read(displaced_path(&transaction, &receipt)).unwrap(),
        LATE_USER.as_bytes()
    );
    assert_eq!(
        late_native_identity(&displaced_path(&transaction, &receipt)),
        original_identity,
        "the edited original object is retained, not copied"
    );
    assert!(
        error
            .to_string()
            .contains(&transaction.join("receipt.json").display().to_string())
    );
}

#[test]
fn late_config_publication_preserves_an_editor_atomic_replacement() {
    let directory = writer_test_support::Editable::new();
    let file = directory.path().join("mcp.json");
    std::fs::write(&file, LATE_ORIGINAL).unwrap();
    let original_identity = late_native_identity(&file);
    let editor_identity = std::rc::Rc::new(std::cell::Cell::new(None));
    let observed = editor_identity.clone();
    let result = io::with_before_publish(
        move |path| {
            use std::io::Write;
            let mut replacement = tempfile::NamedTempFile::new_in(path.parent().unwrap()).unwrap();
            replacement.write_all(LATE_USER.as_bytes()).unwrap();
            replacement.as_file().sync_all().unwrap();
            replacement.persist(path).unwrap();
            observed.set(Some(late_native_identity(path)));
        },
        || {
            edit_file_for(
                EditOperation::Repair,
                &file,
                Some(directory.path()),
                false,
                |_| Ok(LATE_PREPARED.into()),
            )
        },
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        LATE_PREPARED,
        "{result:?}"
    );
    assert!(
        result.is_err(),
        "an editor save must be an explicit conflict"
    );
    let error = result.unwrap_err();
    assert_eq!(error.kind, EditFailureKind::ConflictRetained);
    let (transaction, receipt) =
        late_record(directory.path(), edit_record::State::ConflictRetained);
    assert_eq!(
        late_record_identity(&receipt.original.identity),
        original_identity
    );
    assert_eq!(
        std::fs::read(displaced_path(&transaction, &receipt)).unwrap(),
        LATE_USER.as_bytes()
    );
    assert_ne!(editor_identity.get(), Some(original_identity));
    assert_eq!(
        Some(late_native_identity(&displaced_path(
            &transaction,
            &receipt
        ))),
        editor_identity.get(),
        "the actual editor replacement object is retained"
    );
    assert!(
        error
            .to_string()
            .contains(&transaction.join("receipt.json").display().to_string())
    );
}

#[test]
fn late_config_publication_preserves_a_user_deletion() {
    let directory = writer_test_support::Editable::new();
    let file = directory.path().join("mcp.json");
    std::fs::write(&file, LATE_ORIGINAL).unwrap();
    let original_identity = late_native_identity(&file);
    let result = io::with_before_publish(
        |path| std::fs::remove_file(path).unwrap(),
        || {
            edit_file_for(
                EditOperation::Repair,
                &file,
                Some(directory.path()),
                false,
                |_| Ok(LATE_PREPARED.into()),
            )
        },
    );
    assert!(
        !file.exists(),
        "a user's deletion must not be undone: {result:?}"
    );
    assert!(
        result.is_err(),
        "a late deletion must be an explicit conflict"
    );
    assert_eq!(result.unwrap_err().kind, EditFailureKind::Refused);
    let (transaction, receipt) = late_record(directory.path(), edit_record::State::RefusedRetained);
    assert_eq!(
        late_record_identity(&receipt.original.identity),
        original_identity
    );
    assert_eq!(
        std::fs::read(transaction.join("prepared")).unwrap(),
        LATE_PREPARED.as_bytes()
    );
    assert_eq!(
        receipt.after.unwrap().target,
        edit_record::Observation::Absent
    );
}

#[test]
fn late_config_publication_does_not_clobber_a_concurrently_created_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("mcp.json");
    let result = io::with_before_publish(
        |path| std::fs::write(path, LATE_USER).unwrap(),
        || {
            edit_file(&file, Some(directory.path()), false, |before| {
                assert_eq!(before, None);
                Ok(LATE_PREPARED.into())
            })
        },
    );
    assert!(
        result.is_err(),
        "existing no-clobber publication must refuse"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), LATE_USER);
    assert!(!directory.path().join(".airk/agent-edits").exists());
}

#[test]
fn late_config_publication_noop_observer_keeps_exact_prepared_bytes() {
    let directory = writer_test_support::Editable::new();
    let file = directory.path().join("mcp.json");
    std::fs::write(&file, LATE_ORIGINAL).unwrap();
    let original_identity = late_native_identity(&file);
    let observed = std::rc::Rc::new(std::cell::Cell::new(0));
    let count = observed.clone();
    let result = io::with_before_publish(
        move |path| {
            assert_eq!(std::fs::read_to_string(path).unwrap(), LATE_ORIGINAL);
            count.set(count.get() + 1);
        },
        || {
            edit_file(&file, Some(directory.path()), false, |_| {
                Ok(LATE_PREPARED.into())
            })
        },
    );
    assert_eq!(result, Ok(true));
    assert_eq!(observed.get(), 1);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), LATE_PREPARED);
    let (transaction, receipt) =
        late_record(directory.path(), edit_record::State::PublishedRetained);
    assert_eq!(
        late_record_identity(&receipt.original.identity),
        original_identity
    );
    assert_eq!(
        std::fs::read(displaced_path(&transaction, &receipt)).unwrap(),
        LATE_ORIGINAL.as_bytes()
    );
    assert_eq!(
        late_native_identity(&displaced_path(&transaction, &receipt)),
        original_identity
    );
}

#[test]
fn exact_entry_revalidation_refuses_live_changes_and_keeps_concurrent_source_bytes() {
    let ownership = |_: Incarnation| Ownership::Unavailable;
    let exists = |_: &str| Some(true);
    let observations = Observations {
        endpoint_url: None,
        trust_installed: None,
        ownership: &ownership,
        path_exists: &exists,
        verified_launchers: &[],
        windows: false,
    };
    let directory = writer_test_support::Editable::new();
    let file = directory.path().join(".mcp.json");
    let original = r#"{"mcpServers":{"airk":{"command":"airk"}}}"#;
    let changed = r#"{"mcpServers":{"airk":{"command":"changed"}},"keep":"concurrent bytes"}"#;
    let entries = Classifier::new(&observations)
        .file(
            Agent::Claude,
            Scope::Project,
            &file.to_string_lossy(),
            original,
        )
        .entries;
    assert_eq!(entries.len(), 1);
    let expected = &entries[0];
    let target = Target {
        container_path: vec!["mcpServers".into()],
        server_key: "airk".into(),
    };
    std::fs::write(&file, changed).unwrap();
    let result = edit_file(&file, Some(directory.path()), false, |source| {
        let source = source.unwrap();
        revalidate_entry(source, expected, &observations)?;
        remove(Agent::Claude, Scope::Project, source, &target)
    });
    assert!(result.unwrap_err().contains("changed before its edit"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);

    std::fs::write(&file, original).unwrap();
    let result = edit_file(&file, Some(directory.path()), false, |source| {
        let source = source.unwrap();
        revalidate_entry(source, expected, &observations)?;
        let prepared = remove(Agent::Claude, Scope::Project, source, &target)?;
        std::fs::write(&file, changed).unwrap();
        Ok(prepared)
    });
    assert!(result.unwrap_err().contains("changed while AIRK prepared"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), changed);
}

#[test]
fn exact_entry_revalidation_uses_fresh_connection_observations() {
    let source = r#"{"servers":{"airk":{"type":"http","url":"https://127.0.0.1:62133/mcp","headers":{"Authorization":"Bearer ${input:airkToken_11111111111111111111111111111111}"}}}}"#;
    let exists = |_: &str| Some(true);
    let missing = |_: Incarnation| Ownership::NotOwned;
    let owned = |_: Incarnation| Ownership::Owned {
        url: "https://127.0.0.1:62133/mcp".into(),
    };
    let before = Observations {
        endpoint_url: None,
        trust_installed: None,
        ownership: &missing,
        path_exists: &exists,
        verified_launchers: &[],
        windows: false,
    };
    let after = Observations {
        ownership: &owned,
        ..before
    };
    let entries = Classifier::new(&before)
        .file(
            Agent::Copilot,
            Scope::Project,
            "/fixture/.vscode/mcp.json",
            source,
        )
        .entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].status, Some(Class::HttpsUnknownConnection));
    assert!(revalidate_entry(source, &entries[0], &before).is_ok());
    assert!(
        revalidate_entry(source, &entries[0], &after)
            .unwrap_err()
            .contains("changed before its edit")
    );
}

fn id(value: &str) -> Incarnation {
    value.parse().expect("connection id")
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("string array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}

fn expect(
    name: &str,
    operation: &str,
    source: Option<&str>,
    result: Result<String, String>,
    expected: &Value,
) {
    let label = format!("{name}: {operation}");
    if expected.get("refused").is_some() {
        assert!(
            result.is_err(),
            "{label}: expected a refusal, got {result:?}"
        );
        return;
    }
    let text = result.unwrap_or_else(|error| panic!("{label}: {error}"));
    if expected.get("unchanged").is_some() {
        assert_eq!(Some(text.as_str()), source, "{label}: expected no change");
    } else {
        let wanted = expected["text"].as_str().expect("expected text");
        assert_eq!(text, wanted, "{label}");
    }
}

fn connection(spec: &Value) -> Connection<'_> {
    Connection {
        id: id(spec["connectionId"].as_str().expect("connectionId")),
        url: spec["url"].as_str().expect("url"),
        airkctl: spec["airkctl"].as_str().expect("airkctl"),
    }
}

fn run(case: &Value) {
    let name = case["name"].as_str().expect("name");
    let agent: Agent = serde_json::from_value(case["agent"].clone()).expect("agent");
    let scope: Scope = serde_json::from_value(case["scope"].clone()).expect("scope");
    let file = case["file"].as_str().expect("file");
    let source = case["source"].as_str();
    let facts = &case["observations"];
    let connections: HashMap<String, Ownership> = facts["connections"]
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| {
                    let ownership = match value.get("owned").and_then(Value::as_str) {
                        Some(url) => Ownership::Owned {
                            url: url.to_string(),
                        },
                        None if value == "not_owned" => Ownership::NotOwned,
                        None => panic!("{name}: unknown ownership {value}"),
                    };
                    (key.clone(), ownership)
                })
                .collect()
        })
        .unwrap_or_default();
    let paths: HashMap<String, Option<bool>> = facts["paths"]
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(key, value)| (key.clone(), value.as_bool()))
                .collect()
        })
        .unwrap_or_default();
    let verified = facts
        .get("verifiedLaunchers")
        .map(strings)
        .unwrap_or_default();
    let ownership = |connection: Incarnation| {
        connections
            .get(&connection.to_string())
            .cloned()
            .unwrap_or(Ownership::Unavailable)
    };
    let exists = |path: &str| paths.get(path).copied().unwrap_or(Some(true));
    let observations = Observations {
        endpoint_url: facts["endpointUrl"].as_str(),
        trust_installed: facts["trustInstalled"].as_bool(),
        ownership: &ownership,
        path_exists: &exists,
        verified_launchers: &verified,
        windows: false,
    };
    if let Some(source) = source {
        let report = Classifier::new(&observations).file(agent, scope, file, source);
        let mut findings = serde_json::to_value(&report.findings).expect("findings");
        for finding in findings.as_array_mut().expect("array") {
            finding.as_object_mut().expect("object").remove("detail");
        }
        assert_eq!(findings, case["findings"], "{name}: findings");
        let entries: Vec<Value> = report
            .entries
            .iter()
            .map(|entry| json!({"containerPath": entry.container_path, "serverKey": entry.server_key, "status": entry.status}))
            .collect();
        assert_eq!(Value::Array(entries), case["entries"], "{name}: entries");
    }
    let target = |spec: &Value| Target {
        container_path: strings(&spec["containerPath"]),
        server_key: spec["serverKey"].as_str().expect("serverKey").to_string(),
    };
    if let Some(spec) = case.get("connect") {
        let key = spec["serverKey"].as_str().expect("serverKey");
        let result = connect(agent, scope, source, key, &connection(spec));
        expect(name, "connect", source, result, &spec["expect"]);
    }
    for operation in ["repair", "remove"] {
        let Some(spec) = case.get(operation) else {
            continue;
        };
        let text = source.unwrap_or_else(|| panic!("{name}: {operation} needs a source"));
        let result = if operation == "repair" {
            repair(agent, scope, text, &target(spec), &connection(spec))
        } else {
            remove(agent, scope, text, &target(spec))
        };
        expect(name, operation, source, result, &spec["expect"]);
    }
}

#[test]
fn shared_cases() {
    let document: Value = serde_json::from_str(CASES).expect("config_cases.json");
    assert_eq!(document["schema"], "airk.split.agents.config_cases/1");
    let cases = document["cases"].as_array().expect("cases");
    assert!(!cases.is_empty(), "no shared cases selected");
    for case in cases {
        run(case);
    }
}

#[test]
fn helper_quotes_paths_a_shell_would_split() {
    let windows = r"C:\Program Files\AIRK\airkctl.exe";
    let command = recognize::helper_command(windows, id(A)).expect("helper");
    assert_eq!(
        command,
        format!("\"{windows}\" agents credential --connection-id {A} --format header-json")
    );
    assert_eq!(
        recognize::helper_connection(&command),
        Some((windows.to_string(), id(A)))
    );
    for refused in [
        "airkctl",
        "/opt/airk/airk",
        "/opt/$HOME/airkctl",
        r"\\server\share\airkctl.exe",
    ] {
        assert!(
            recognize::helper_command(refused, id(A)).is_err(),
            "{refused}"
        );
    }
}

#[test]
fn only_the_u92_helper_form_is_recognized() {
    let ok =
        format!("/opt/airk/bin/airkctl agents credential --connection-id {A} --format header-json");
    assert!(recognize::helper_connection(&ok).is_some());
    for other in [
        format!(
            "/opt/airk/airk runtime http-credential --install-dir /opt/airk --connection-id {A} --format header-json"
        ),
        format!("airkctl agents credential --connection-id {A} --format header-json"),
        format!(
            "/opt/airk/bin/airkctl agents credential --connection-id {} --format header-json",
            A.to_uppercase()
        ),
        format!(
            "/opt/airk/bin/airkctl  agents credential --connection-id {A} --format header-json"
        ),
        format!(
            "/opt/airk/bin/airkctl agents credential --connection-id {A} --format header-json --x"
        ),
        format!("/opt/airk/bin/other agents credential --connection-id {A} --format header-json"),
    ] {
        assert_eq!(recognize::helper_connection(&other), None, "{other}");
    }
}

#[test]
fn endpoint_urls_are_loopback_https_mcp_only() {
    for ok in ["https://127.0.0.1:1/mcp", "https://127.0.0.1:65535/mcp"] {
        assert!(recognize::http_url(ok), "{ok}");
    }
    for refused in [
        "https://127.0.0.1:0/mcp",
        "https://127.0.0.1:65536/mcp",
        "https://127.0.0.1:08/mcp",
        "http://127.0.0.1:4433/mcp",
        "https://localhost:4433/mcp",
        "https://127.0.0.1:4433/mcp/",
    ] {
        assert!(!recognize::http_url(refused), "{refused}");
    }
}

#[test]
fn jsonc_accepts_comments_and_trailing_commas_only() {
    let tree = jsonc::parse(
        "\u{feff}{ /* a */ \"a\": [1, 2,], // b\n \"b\": {\"c\": \"\\u00e9\\ud83d\\ude00\"}, }",
    )
    .expect("jsonc");
    assert_eq!(
        tree.value(),
        json!({"a": [1, 2], "b": {"c": "\u{e9}\u{1f600}"}})
    );
    for refused in [
        "[]",
        "{\"a\": 1,,}",
        "{\"a\": 1} x",
        "{\"a\": tru}",
        "{'a': 1}",
        "{\"a\": \"\n\"}",
        "{\"a\": 1 /* x",
    ] {
        assert!(jsonc::parse(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn toml_strings_cannot_manufacture_headers() {
    let source = "[tool]\nnote = \"\"\"\n[mcp_servers.airk]\nurl = \"x\"\n\"\"\"\n[mcp_servers.real]\ncommand = 'a # not a comment'\n";
    let rows = toml::declarations(source).expect("toml");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].server_key, "real");
    assert_eq!(rows[0].fields["command"], "a # not a comment");
    for refused in [
        "[mcp_servers.a]\n[mcp_servers.a]\n",
        "[[mcp_servers.a]]\n",
        "[mcp_servers.a]\nx = 1\nx = 2\n",
        "[mcp_servers]\na = { url = \"u\" }\n[mcp_servers.a]\ncommand = \"c\"\n",
        "x = [\n",
    ] {
        assert!(toml::declarations(refused).is_err(), "{refused:?}");
    }
}

#[test]
fn toml_inline_airk_entries_need_manual_review() {
    let source = format!(
        "[mcp_servers]\nairk = {{ url = \"https://127.0.0.1:4433/mcp\", http_headers_helper = \"/opt/airk/bin/airkctl agents credential --connection-id {A} --format header-json\" }}\n"
    );
    let target = Target {
        container_path: vec!["mcp_servers".into()],
        server_key: "airk".into(),
    };
    assert!(remove(Agent::Codex, Scope::Project, &source, &target).is_err());
}

#[test]
fn user_locations_follow_the_platform() {
    let env = UserEnv {
        home: Some(r"C:\Users\u".into()),
        appdata: Some(r"D:\Roaming".into()),
        codex_home: Some("relative".into()),
        ..UserEnv::default()
    };
    let found = user_locations(Platform::Windows, &env);
    let files: Vec<(Agent, &str)> = found
        .files
        .iter()
        .map(|(agent, path)| (*agent, path.as_str()))
        .collect();
    assert_eq!(
        files,
        [
            (Agent::Codex, r"C:\Users\u\.codex\config.toml"),
            (Agent::Claude, r"C:\Users\u\.claude.json"),
            (Agent::Copilot, r"D:\Roaming\Code\User\mcp.json"),
            (Agent::Copilot, r"D:\Roaming\Code - Insiders\User\mcp.json"),
        ]
    );
    assert_eq!(found.blockers, ["invalid_codex_home"]);
    let linux = user_locations(
        Platform::Linux,
        &UserEnv {
            home: Some("/home/u".into()),
            ..UserEnv::default()
        },
    );
    assert_eq!(
        linux.files[2],
        (
            Agent::Copilot,
            "/home/u/.config/Code/User/mcp.json".to_string()
        )
    );
}

#[test]
fn edit_file_creates_project_folders_and_replaces_atomically() {
    let fixture = writer_test_support::Editable::new();
    let root = fixture.path().to_path_buf();
    let file = root.join(".vscode").join("mcp.json");
    let wrote = edit_file(&file, Some(root.as_path()), true, |before| {
        assert_eq!(before, None);
        Ok("{}\n".into())
    })
    .expect("create");
    assert!(wrote);
    let unchanged = edit_file(&file, Some(root.as_path()), false, |before| {
        Ok(before.unwrap_or_default().to_string())
    })
    .expect("unchanged");
    assert!(!unchanged);
    let replaced = edit_file(&file, Some(root.as_path()), false, |_| {
        Ok("{ \"servers\": {} }\n".into())
    })
    .expect("replace");
    assert!(replaced);
    assert_eq!(
        std::fs::read_to_string(&file).expect("read"),
        "{ \"servers\": {} }\n"
    );
    let user = root.join("missing").join("config.toml");
    assert!(edit_file(&user, None, true, |_| Ok(String::new())).is_err());
}

#[cfg(unix)]
#[test]
fn linked_configuration_files_are_refused() {
    let root = tempfile::tempdir().expect("tempdir");
    let target = root.path().join("real.json");
    std::fs::write(&target, "{}\n").expect("write");
    let link = root.path().join(".mcp.json");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");
    assert!(edit_file(&link, None, false, |_| Ok("{\"x\": 1}\n".into())).is_err());
    assert_eq!(std::fs::read_to_string(&target).expect("read"), "{}\n");
}
