// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::json;

#[path = "runtime_legacy_cache_reprepare_tests.rs"]
mod reprepare_tests;

const ORIGINAL_IMAGE: &[u8] = b"Inert historical helper fixture; never execute these bytes.";
const SOURCE_BYTES: &[u8] = b"Original user source and saved index bytes must be preserved.";

struct Fixture {
    owner: tempfile::TempDir,
    root: PathBuf,
    selected: String,
    global: PathBuf,
    image: PathBuf,
    source: PathBuf,
    current: PathBuf,
    current_bytes: Vec<u8>,
    selection_bytes: Vec<u8>,
}

fn image_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn cache_at(global: &Path, version: &str, bytes: &[u8]) -> Result<PathBuf> {
    let platform = if cfg!(windows) {
        "win32-x64"
    } else if cfg!(target_os = "macos") {
        "darwin-x64"
    } else {
        "linux-x64"
    };
    let leaf = global
        .join("iokaio.airk/helpers")
        .join(format!("{version}-{platform}-{}", &image_hash(bytes)[..16]));
    fs::create_dir_all(&leaf)?;
    let image = leaf.join(if cfg!(windows) { "airk.exe" } else { "airk" });
    fs::write(&image, bytes)?;
    Ok(image)
}

impl Fixture {
    fn new() -> Result<Self> {
        let (owner, root, selected) = super::super::tests::unselected_fixture()?;
        // Real staging initializes this owned installer lock. The inert
        // payload fixture intentionally skips staging and must do so explicitly.
        drop(install_lock(&root)?);
        replace_json(
            &root,
            "selected.json",
            &Selection {
                schema: 1,
                active: selected.clone(),
                previous: None,
            },
        )?;
        let global = owner.path().canonicalize()?.join("Code/User/globalStorage");
        let image = cache_at(&global, "1.3.2", ORIGINAL_IMAGE)?;
        let source = owner.path().join("original-source-and-index.bin");
        fs::write(&source, SOURCE_BYTES)?;
        let (current, _) = verify_runtime(&root, &selected)?;
        let current_bytes = fs::read(&current)?;
        let selection_bytes = fs::read(root.join("selected.json"))?;
        Ok(Self {
            owner,
            root,
            selected,
            global,
            image,
            source,
            current,
            current_bytes,
            selection_bytes,
        })
    }

    fn inspect(&self, inputs: Option<&Path>, prepare: bool) -> Result<serde_json::Value> {
        inspect(&self.root, &self.selected, &self.global, inputs, prepare)
    }

    fn original_config(&self) -> Result<(PathBuf, PathBuf, Vec<u8>)> {
        let config = self.owner.path().join("original-mcp.json");
        let bytes = serde_json::to_vec(&json!({"mcpServers":{"airk-old":{
            "command":self.image,"args":["--root",self.owner.path().join("source"),
            "--data-dir",self.owner.path().join("data"),"--collection","original","mcp"],
            "env":{"PRIVATE_FIXTURE":"must not enter diagnostics"}
        }},"unrelated":{"preserve":true}}))?;
        fs::write(&config, &bytes)?;
        let inputs = self.root.join("cache-locators.json");
        replace_json(
            &self.root,
            "cache-locators.json",
            &vec![ConfigLocator {
                path: config.clone(),
                format: origin::ConfigFormat::McpServersJson,
                server: "airk-old".into(),
                container_path: None,
            }],
        )?;
        Ok((config, inputs, bytes))
    }

    fn assert_preserved(&self) -> Result<()> {
        assert_eq!(fs::read(&self.source)?, SOURCE_BYTES);
        assert_eq!(fs::read(&self.current)?, self.current_bytes);
        assert_eq!(
            fs::read(self.root.join("selected.json"))?,
            self.selection_bytes
        );
        Ok(())
    }
}

#[test]
fn native_classification_preserves_mixed_originals_and_refuses_unobserved_scope() -> Result<()> {
    let f = Fixture::new()?;
    let managed = cache_at(
        &f.global,
        "1.8.13",
        b"Inert managed cached entry; never execute",
    )?;
    let config = f.owner.path().join("mixed-original.json");
    fs::write(
        &config,
        serde_json::to_vec(&json!({"mcpServers":{
            "standalone":{"command":f.image,"args":["--root",f.owner.path().join("source"),"--data-dir",f.owner.path().join("old-data"),"--collection","saved","mcp"]},
            "managed":{"command":managed,"args":["launch","--service-install-dir",f.root,"--service-project","1".repeat(64)]},
            "unknown":{"command":f.image,"args":["mcp"]}
        }}))?,
    )?;
    let values = ["standalone", "managed", "unknown"].map(|server| ConfigLocator {
        path: config.clone(),
        format: origin::ConfigFormat::McpServersJson,
        server: server.into(),
        container_path: None,
    });
    replace_json(&f.root, "mixed-locators.json", &values)?;
    let plan = f.inspect(Some(&f.root.join("mixed-locators.json")), true)?;
    assert_eq!(plan["origin_locators"], serde_json::to_value(&values)?);
    assert_eq!(plan["origin_proofs"].as_array().unwrap().len(), 3);
    assert_eq!(plan["adoption_locators"], json!([values[0]]));
    assert_eq!(plan["managed_locators"], json!([values[1]]));
    let observations = plan["origin_classifications"].as_array().unwrap();
    assert_eq!(
        observations
            .iter()
            .map(|value| value["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["standalone", "managed", "unsupported"]
    );
    assert!(
        observations
            .iter()
            .all(|value| value["authority"] == "not_observed")
    );
    assert_eq!(
        observations[2]["code"],
        "legacy_origin_declaration_unsupported"
    );
    assert!(
        retire(
            &f.root,
            &f.selected,
            plan["inspection_sha256"].as_str().unwrap()
        )
        .is_err()
    );
    assert!(f.image.exists() && managed.exists());
    f.assert_preserved()
}

#[test]
fn native_locator_classification_distinguishes_actual_containers_in_one_file() -> Result<()> {
    let f = Fixture::new()?;
    let config = f.owner.path().join("multiple-containers.json");
    let args = json!([
        "--root",
        f.owner.path().join("source"),
        "--data-dir",
        f.owner.path().join("old-data"),
        "--collection",
        "saved",
        "mcp"
    ]);
    fs::write(
        &config,
        serde_json::to_vec(
            &json!({"mcpServers":{"airk":{"command":f.image,"args":args}},"servers":{"airk":{"command":f.image,"args":args}}}),
        )?,
    )?;
    let values = vec![
        ConfigLocator {
            path: config.clone(),
            format: origin::ConfigFormat::McpServersJson,
            server: "airk".into(),
            container_path: None,
        },
        ConfigLocator {
            path: config,
            format: origin::ConfigFormat::VscodeServersJson,
            server: "airk".into(),
            container_path: None,
        },
    ];
    replace_json(&f.root, "container-locators.json", &values)?;
    let plan = f.inspect(Some(&f.root.join("container-locators.json")), true)?;
    assert_eq!(plan["adoption_locators"].as_array().unwrap().len(), 2);
    let accepted = read(&f.root, &f.selected)?.unwrap();
    for locator in &values {
        assert_eq!(
            accepted
                .inspection
                .origin_proofs
                .iter()
                .filter(|proof| origin::matches_locator(proof, locator).unwrap())
                .count(),
            1
        );
    }
    assert_ne!(
        plan["origin_classifications"][0]["origin_sha256"],
        plan["origin_classifications"][1]["origin_sha256"]
    );
    f.assert_preserved()
}

#[test]
fn cache_inspection_is_context_only_read_only_and_keeps_unreferenced_profiles_outside_scope()
-> Result<()> {
    let f = Fixture::new()?;
    let other_global = f
        .owner
        .path()
        .join("OtherEditor/User/profiles/other/globalStorage");
    let other = cache_at(&other_global, "1.0.4", b"Other inert helper bytes")?;
    let (config, _, config_bytes) = f.original_config()?;
    let observation = f.inspect(None, false)?;
    assert_eq!(observation["scope"], "context-only");
    assert_eq!(observation["active"], f.selected);
    assert_eq!(observation["global_storage_roots"], json!([f.global]));
    assert_eq!(observation["caches"].as_array().unwrap().len(), 1);
    assert!(observation["origin_proofs"].as_array().unwrap().is_empty());
    assert_eq!(observation["caches"][0]["artifact_approved"], false);
    assert_eq!(
        observation["caches"][0]["executable_sha256"],
        image_hash(ORIGINAL_IMAGE)
    );
    assert!(!f.root.join(JOURNAL).exists());
    require_settled(&f.root)?;
    assert_eq!(fs::read(config)?, config_bytes);
    assert_eq!(fs::read(&f.image)?, ORIGINAL_IMAGE);
    assert_eq!(fs::read(other)?, b"Other inert helper bytes");
    f.assert_preserved()
}

#[test]
fn prepared_original_proof_survives_configuration_migration_without_recapturing_new_authority()
-> Result<()> {
    let f = Fixture::new()?;
    let (config, inputs, original) = f.original_config()?;
    let plan = f.inspect(Some(&inputs), true)?;
    assert_eq!(
        plan["origin_proofs"][0]["config_sha256"],
        image_hash(&original)
    );
    assert!(!serde_json::to_string(&plan)?.contains("must not enter diagnostics"));
    let frozen = fs::read(f.root.join(JOURNAL))?;
    assert!(require_settled(&f.root).is_err());
    assert!(
        retire(
            &f.root,
            &f.selected,
            plan["inspection_sha256"].as_str().unwrap()
        )
        .is_err(),
        "still-referenced original executable was deleted before actual configuration migration"
    );
    assert!(f.image.exists());
    let migrated = serde_json::to_vec(&json!({"mcpServers":{"airk-modern":{
        "type":"http","url":"https://127.0.0.1:57421/mcp","headers":{"Authorization":"Bearer ${AIRK_MCP_TOKEN_FIXTURE}"}
    }},"unrelated":{"preserve":true}}))?;
    fs::write(&config, &migrated)?;
    assert_eq!(f.inspect(Some(&inputs), true)?, plan);
    assert_eq!(fs::read(f.root.join(JOURNAL))?, frozen);
    let outcome = retire(
        &f.root,
        &f.selected,
        plan["inspection_sha256"].as_str().unwrap(),
    )?;
    assert_eq!(outcome["caches_retired"], true);
    assert_eq!(outcome["remaining"], 0);
    assert!(!f.image.exists() && !f.image.parent().unwrap().exists());
    assert!(!f.root.join(JOURNAL).exists());
    assert_eq!(fs::read(config)?, migrated);
    require_settled(&f.root)?;
    f.assert_preserved()
}

#[test]
fn cache_retirement_resumes_missing_image_and_leaf_from_the_exact_frozen_journal() -> Result<()> {
    let f = Fixture::new()?;
    let plan = f.inspect(None, true)?;
    let mut journal = read(&f.root, &f.selected)?.unwrap();
    // Model a crash after the native image exclusion was durably saved and the
    // image/leaf were unlinked. No fixture executable is ever started.
    let held = image_exclusion(&images(&journal.inspection))?;
    journal.image_exclusions = held.images.clone();
    save(&f.root, &journal)?;
    drop(held);
    fs::remove_file(&f.image)?;
    fs::remove_dir(f.image.parent().unwrap())?;
    let frozen = fs::read(f.root.join(JOURNAL))?;
    assert_eq!(f.inspect(None, true)?, plan);
    assert_eq!(fs::read(f.root.join(JOURNAL))?, frozen);
    let outcome = retire(
        &f.root,
        &f.selected,
        plan["inspection_sha256"].as_str().unwrap(),
    )?;
    assert_eq!(outcome["caches_retired"], true);
    assert_eq!(outcome["retired_caches"].as_array().unwrap().len(), 1);
    assert!(!f.root.join(JOURNAL).exists());
    f.assert_preserved()
}

#[test]
fn crossed_cache_hash_path_and_inspection_digest_never_authorize_deletion() -> Result<()> {
    let f = Fixture::new()?;
    let plan = f.inspect(None, true)?;
    let original_journal = fs::read(f.root.join(JOURNAL))?;
    let foreign = f.owner.path().join("foreign-image");
    fs::write(&foreign, b"Unrelated user file")?;
    assert!(retire(&f.root, &f.selected, &"f".repeat(64)).is_err());
    assert_eq!(fs::read(f.root.join(JOURNAL))?, original_journal);
    for change_hash in [true, false] {
        let mut journal: Journal = serde_json::from_slice(&original_journal)?;
        if change_hash {
            journal.inspection.caches[0].executable_sha256 = "f".repeat(64);
        } else {
            journal.inspection.caches[0].cache_directory = f.owner.path().to_owned();
            journal.inspection.caches[0].executable = foreign.clone();
        }
        // Even a self-consistent edited JSON digest cannot manufacture native
        // file/ancestor authority or move the accepted cache to another path.
        journal.inspection_sha256 = journal.inspection.digest()?;
        save(&f.root, &journal)?;
        assert!(read(&f.root, &f.selected).is_err());
        assert!(retire(&f.root, &f.selected, &journal.inspection_sha256).is_err());
        assert_eq!(fs::read(&f.image)?, ORIGINAL_IMAGE);
        assert_eq!(fs::read(&foreign)?, b"Unrelated user file");
    }
    fs::write(f.root.join(JOURNAL), &original_journal)?;
    assert_eq!(f.inspect(None, true)?, plan);
    f.assert_preserved()
}

#[test]
fn a_new_cache_after_preparation_is_preserved_and_cannot_be_added_to_the_frozen_roster()
-> Result<()> {
    let f = Fixture::new()?;
    let plan = f.inspect(None, true)?;
    let later_bytes = b"New inert helper after accepted inspection";
    let later = cache_at(&f.global, "1.2.5", later_bytes)?;
    let error = retire(
        &f.root,
        &f.selected,
        plan["inspection_sha256"].as_str().unwrap(),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("new legacy cache appeared"),
        "{error:#}"
    );
    assert_eq!(fs::read(&later)?, later_bytes);
    assert!(f.root.join(JOURNAL).exists());
    assert_eq!(
        f.inspect(None, true)?,
        plan,
        "retry must not silently widen its accepted roster"
    );
    f.assert_preserved()
}
