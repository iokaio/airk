// SPDX-License-Identifier: Apache-2.0
//! Native original-capability/physical partition regressions. These do not
//! substitute for the actual selected-runtime batch/process/Current campaign.
use super::*;
use origin::ConfigFormat;
use serde_json::json;

struct Fixture {
    _owner: tempfile::TempDir,
    base: PathBuf,
    global: PathBuf,
    data: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Result<Self> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let global = base.join("editor/User/globalStorage");
        let data = base.join("original-data");
        let root = base.join("missing-original-source");
        Ok(Self {
            _owner: owner,
            base,
            global,
            data,
            root,
        })
    }
    fn executable(&self, version: &str) -> Result<PathBuf> {
        let bytes = b"Inert original fixture image; never execute";
        let digest = format!("{:x}", Sha256::digest(bytes));
        let platform = if cfg!(windows) {
            "win32-x64"
        } else if cfg!(target_os = "macos") {
            "darwin-arm64"
        } else {
            "linux-x64"
        };
        let directory = self
            .global
            .join("iokaio.airk/helpers")
            .join(format!("{version}-{platform}-{}", &digest[..16]));
        fs::create_dir_all(&directory)?;
        let executable = directory.join(if cfg!(windows) { "airk.exe" } else { "airk" });
        fs::write(&executable, bytes)?;
        Ok(executable)
    }
    fn entry(&self, version: &str, data: &Path) -> Result<serde_json::Value> {
        Ok(
            json!({"command":self.executable(version)?,"args":["--root",self.root,"--data-dir",data,"--collection","saved","mcp"]}),
        )
    }
    fn proof(&self, path: &Path, server: &str) -> Result<Captured> {
        let locator = ConfigLocator {
            path: path.into(),
            format: ConfigFormat::McpServersJson,
            server: server.into(),
            container_path: None,
        };
        let proof = origin::inspect_at_global_storage(&locator, &self.global)?;
        let scope = proof.standalone_scope()?;
        let collection = scope.collection_path();
        fs::create_dir_all(&collection)?;
        let locks = collection.parent().unwrap().join(".writer-locks");
        fs::create_dir_all(&locks)?;
        let advisory = locks.join(&scope.collection);
        if !advisory.exists() {
            fs::write(advisory, b"")?;
        }
        fs::write(
            collection.join("saved-publication"),
            b"Identical fictional saved publication bytes",
        )?;
        Ok(Captured {
            locator,
            identity: proof.identity()?,
            scope,
            proof: Some(proof),
        })
    }
}
fn order(mut captured: Vec<Captured>) -> Vec<Captured> {
    captured.sort_by_key(|value| value.identity);
    captured
}

#[test]
fn codex_and_claude_originals_share_one_actual_native_backing_and_keep_independent_claims()
-> Result<()> {
    let f = Fixture::new()?;
    let first = f.base.join("codex-fixture.json");
    let second = f.base.join("claude-fixture.json");
    fs::write(
        &first,
        serde_json::to_vec(&json!({"mcpServers":{"airk":f.entry("1.3.2",&f.data)?}}))?,
    )?;
    fs::write(
        &second,
        serde_json::to_vec(
            &json!({"mcpServers":{"airk-legacy":f.entry("1.3.2",&f.data)?},"unrelated":"preserve"}),
        )?,
    )?;
    let before = (fs::read(&first)?, fs::read(&second)?);
    let captured = order(vec![
        f.proof(&first, "airk")?,
        f.proof(&second, "airk-legacy")?,
    ]);
    let groups = partition(
        Incarnation::from_bytes([1; 16]),
        &captured,
        None,
        &f.base.join("unused-control"),
    )?;
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].members.len(), 2);
    let claims = captured
        .iter()
        .map(|value| claim(value.proof.as_ref().unwrap()))
        .collect::<Result<Vec<_>>>()?;
    assert_ne!(claims[0].origin_sha256, claims[1].origin_sha256);
    assert_ne!(claims[0].config_sha256, claims[1].config_sha256);
    assert_ne!(claims[0].original_server, claims[1].original_server);
    assert_eq!(
        groups[0].writer.as_ref().unwrap().collection_identity()?,
        groups[0].physical
    );
    assert!(
        LegacyWriterExclusion::acquire_existing(&groups[0].collection).is_err(),
        "native batch retains its actual writer exclusion"
    );
    assert_eq!((fs::read(&first)?, fs::read(&second)?), before);
    assert!(
        !f.root.exists(),
        "partitioning must not open/create original source directories"
    );
    Ok(())
}

#[test]
fn same_file_same_labels_but_different_legacy_namespaces_remain_two_physical_groups() -> Result<()>
{
    let f = Fixture::new()?;
    let config = f.base.join("same-file-aliases.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"mcpServers":{
        "before-1-2":f.entry("1.0.0",&f.data)?,"from-1-2":f.entry("1.3.2",&f.data)?}}))?,
    )?;
    let captured = order(vec![
        f.proof(&config, "before-1-2")?,
        f.proof(&config, "from-1-2")?,
    ]);
    let groups = partition(
        Incarnation::from_bytes([2; 16]),
        &captured,
        None,
        &f.base.join("unused-control"),
    )?;
    assert_eq!(groups.len(), 2);
    assert_ne!(groups[0].physical, groups[1].physical);
    assert_ne!(groups[0].namespace, groups[1].namespace);
    assert_ne!(groups[0].id, groups[1].id);
    let claims = captured
        .iter()
        .map(|value| claim(value.proof.as_ref().unwrap()))
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(claims[0].config_sha256, claims[1].config_sha256);
    assert_ne!(claims[0].origin_sha256, claims[1].origin_sha256);
    assert_ne!(
        claims[0].selected_entry_sha256,
        claims[1].selected_entry_sha256
    );
    Ok(())
}

#[test]
fn identical_saved_bytes_in_distinct_native_directories_do_not_merge() -> Result<()> {
    let f = Fixture::new()?;
    let config = f.base.join("different-data.json");
    let other = f.base.join("other-data");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"mcpServers":{
        "first":f.entry("1.3.2",&f.data)?,"second":f.entry("1.3.2",&other)?}}))?,
    )?;
    let captured = order(vec![
        f.proof(&config, "first")?,
        f.proof(&config, "second")?,
    ]);
    let groups = partition(
        Incarnation::from_bytes([3; 16]),
        &captured,
        None,
        &f.base.join("unused-control"),
    )?;
    assert_eq!(groups.len(), 2);
    assert_ne!(groups[0].physical, groups[1].physical);
    assert_eq!(
        fs::read(groups[0].collection.join("saved-publication"))?,
        fs::read(groups[1].collection.join("saved-publication"))?
    );
    Ok(())
}

#[test]
fn changed_same_file_alias_cannot_reuse_an_earlier_held_original_claim() -> Result<()> {
    let f = Fixture::new()?;
    let config = f.base.join("changed-alias.json");
    fs::write(
        &config,
        serde_json::to_vec(
            &json!({"mcpServers":{"first":f.entry("1.3.2",&f.data)?,"second":f.entry("1.3.2",&f.data)?}}),
        )?,
    )?;
    let mut first = f.proof(&config, "first")?;
    fs::write(
        &config,
        serde_json::to_vec(
            &json!({"mcpServers":{"first":f.entry("1.3.2",&f.data)?,"second":{"command":"foreign","args":[]}}}),
        )?,
    )?;
    assert!(first.proof.as_mut().unwrap().revalidate().is_err());
    assert!(f.proof(&config, "second").is_err());
    Ok(())
}

#[test]
fn locator_and_full_pretty_reply_bounds_refuse_without_truncation() -> Result<()> {
    let f = Fixture::new()?;
    let value = ConfigLocator {
        path: f.base.join("original.json"),
        format: ConfigFormat::McpServersJson,
        server: "airk".into(),
        container_path: None,
    };
    assert!(canonical_locators(Vec::new()).is_err());
    assert!(canonical_locators(vec![value.clone(), value.clone()]).is_err());
    assert!(
        canonical_locators(
            (0..33)
                .map(|index| ConfigLocator {
                    server: format!("airk-{index}"),
                    ..value.clone()
                })
                .collect()
        )
        .is_err()
    );
    let response = json!({"groups":[{"complete":"retained"}],"claims":["every original"]});
    assert_eq!(checked_reply(response.clone())?, response);
    assert!(
        checked_reply(json!("x".repeat(MAX_REPLY - 3))).is_ok(),
        "full value plus final LF fits exact wire budget"
    );
    assert!(
        checked_reply(json!("x".repeat(MAX_REPLY - 2))).is_err(),
        "final LF overflow was omitted from native wire budget"
    );
    assert!(checked_reply(json!({"complete":"x".repeat(MAX_REPLY)})).is_err());
    Ok(())
}

#[test]
fn legacy_adoption_data_container_is_separate_owned_and_never_adopts_unknown_marker() -> Result<()>
{
    let temp = tempfile::tempdir()?;
    let parent = temp.path().canonicalize()?;
    let state = parent.join("state");
    let install = parent.join("install");
    let namespace = Incarnation::from_bytes([193; 16]);
    let (data, owner) = adopted_data_location(&install, &state, namespace)?;
    assert_eq!(data.parent(), Some(parent.as_path()));
    disjoint(&data, &state)?;
    disjoint(&data, &install)?;
    #[cfg(unix)]
    let original_mode = {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755))?;
        fs::metadata(&parent)?.permissions().mode()
    };
    let pin = adopted_data_root(&data, &owner)?;
    pin.check()?;
    let marker = fs::read(data.join("owner.json"))?;
    drop(pin);
    let pin = adopted_data_root(&data, &owner)?;
    assert_eq!(fs::read(data.join("owner.json"))?, marker);
    drop(pin);
    let mut crossed = owner.clone();
    crossed["namespace"] = serde_json::json!(Incarnation::from_bytes([194; 16]));
    assert!(adopted_data_root(&data, &crossed).is_err());
    assert_eq!(fs::read(data.join("owner.json"))?, marker);
    let unknown = parent.join("unknown");
    local_ipc::prepare_root(&unknown)?;
    assert!(adopted_data_root(&unknown, &owner).is_err());
    assert!(!unknown.join("owner.json").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&parent)?.permissions().mode(), original_mode);
        assert_eq!(fs::metadata(&data)?.permissions().mode() & 0o777, 0o700);
        let alias = parent.join("alias");
        std::os::unix::fs::symlink(&data, &alias)?;
        assert!(adopted_data_root(&alias, &owner).is_err());
    }
    Ok(())
}
