// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn cache_image_bound_includes_authentic_pre_service_release_images() -> Result<()> {
    require_cache_image_size(281_591_032)?;
    require_cache_image_size(270_039_760)?;
    require_cache_image_size(MAX_CACHE_IMAGE_BYTES)?;
    assert!(require_cache_image_size(MAX_CACHE_IMAGE_BYTES + 1).is_err());
    Ok(())
}
use serde_json::json;

#[test]
fn cache_only_inventory_captures_unreferenced_versions_without_approving_execution_or_scope() {
    let f = Fixture::new("0.2.1-build.4");
    let original = fs::read(&f.executable).unwrap();
    let other = f.executable.parent().unwrap().parent().unwrap().join(
        f.executable
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replacen("0.2.1-build.4", "1.3.9", 1),
    );
    fs::create_dir(&other).unwrap();
    fs::write(other.join(f.executable.file_name().unwrap()), &original).unwrap();
    fs::remove_file(&f.config).unwrap();
    let caches = inspect_caches_at_global_storage(&f.global).unwrap();
    assert_eq!(caches.len(), 2);
    for observed in caches {
        let mut cache = resume_cache(&observed).unwrap();
        assert!(!cache.observation().artifact_approved);
        assert_eq!(cache.observation().executable_sha256, hex(&original));
        assert_eq!(
            cache.observation().user,
            crate::local_ipc::current_user().unwrap()
        );
        assert!(cache.exists());
        cache.revalidate().unwrap();
    }
}

#[test]
fn cache_retirement_resume_requires_original_directory_and_image_identities_even_after_unlink() {
    let f = Fixture::new("1.3.2");
    let observed = inspect_caches_at_global_storage(&f.global)
        .unwrap()
        .pop()
        .unwrap();
    fs::remove_file(&f.executable).unwrap();
    assert!(
        inspect_caches_at_global_storage(&f.global).is_err(),
        "empty directory invented original image authority"
    );
    let mut resumed = resume_cache(&observed).unwrap();
    assert!(!resumed.exists());
    resumed.revalidate().unwrap();
    drop(resumed);
    fs::remove_dir(&observed.cache_directory).unwrap();
    let mut resumed = resume_cache(&observed).unwrap();
    resumed.revalidate().unwrap();
    drop(resumed);
    let mut crossed = observed.clone();
    crossed.cache_directory = f.global.join("foreign");
    assert!(resume_cache(&crossed).is_err());
    let mut crossed = observed;
    crossed.helpers_identity.push_str("changed");
    assert!(resume_cache(&crossed).is_err());
}

#[test]
fn cache_only_inventory_preserves_unknown_entries_links_and_changed_images() {
    let f = Fixture::new("1.0.0");
    let note = f.executable.parent().unwrap().join("unknown-user-note");
    fs::write(&note, b"preserve user bytes").unwrap();
    assert!(inspect_caches_at_global_storage(&f.global).is_err());
    assert_eq!(fs::read(&note).unwrap(), b"preserve user bytes");
    fs::remove_file(note).unwrap();
    let observed = inspect_caches_at_global_storage(&f.global)
        .unwrap()
        .pop()
        .unwrap();
    let mut cache = resume_cache(&observed).unwrap();
    let old = fs::read(&f.executable).unwrap();
    fs::write(&f.executable, b"changed").unwrap();
    assert!(cache.revalidate().is_err());
    assert!(inspect_caches_at_global_storage(&f.global).is_err());
    drop(cache);
    fs::write(&f.executable, old).unwrap();
    let alias = f.executable.parent().unwrap().join("alias");
    fs::hard_link(&f.executable, &alias).unwrap();
    assert!(inspect_caches_at_global_storage(&f.global).is_err());
    assert!(f.executable.exists() && alias.exists());
}

#[test]
fn cleanup_refuses_current_future_and_unknown_cache_versions() {
    for version in [env!("CARGO_PKG_VERSION"), "99.0.0", "0.1.0"] {
        let f = Fixture::new(version);
        let original = fs::read(&f.executable).unwrap();
        assert!(inspect_caches_at_global_storage(&f.global).is_err());
        assert_eq!(fs::read(&f.executable).unwrap(), original);
    }
}

#[test]
fn nested_claude_entry_keeps_whole_original_file_hash_and_exact_container_identity() {
    let f = Fixture::new("1.2.2");
    let entry = json!({"command":f.executable,"args":f.argv,"env":{"SECRET":"never-export"}});
    let project = "C:\\original project.with.dots";
    let bytes = serde_json::to_vec(&json!({"mcpServers":{"airk-old":entry},"projects":{project:{"mcpServers":{"airk-old":entry}}}})).unwrap();
    fs::write(&f.config, &bytes).unwrap();
    let root = f.inspect().unwrap();
    let mut locator = f.locator();
    locator.container_path = Some(vec!["projects".into(), project.into(), "mcpServers".into()]);
    let nested = inspect_in(&locator, std::slice::from_ref(&f.global)).unwrap();
    assert_eq!(nested.observation().config_sha256, hex(&bytes));
    assert_eq!(
        nested.observation().container_path,
        locator.container_path.unwrap()
    );
    assert_eq!(
        nested.observation().selected_entry_sha256,
        root.observation().selected_entry_sha256
    );
    assert_ne!(
        nested.observation().identity().unwrap(),
        root.observation().identity().unwrap()
    );
    assert!(
        !serde_json::to_string(nested.observation())
            .unwrap()
            .contains("never-export")
    );
    assert!(
        container_path(
            ConfigFormat::VscodeServersJson,
            Some(&["projects".into(), project.into(), "mcpServers".into()])
        )
        .is_err()
    );
}

#[test]
fn standalone_defaults_follow_verified_layout_families_and_never_current_working_directory() {
    for (version, namespace) in [
        ("0.2.1-build.4", "collections"),
        ("1.0.0-build.7", "collections"),
        ("1.1.4", "collections"),
        ("1.2.0", "collections-1.2"),
        ("1.3.1", "collections-1.2"),
    ] {
        let f = Fixture::new(version);
        let source = f._temp.path().join("missing-original-source");
        fs::write(&f.config,serde_json::to_vec(&json!({"mcpServers":{"airk-old":{"command":f.executable,"args":["--root",source,"mcp"]}}})).unwrap()).unwrap();
        let origin = f.inspect().unwrap();
        let scope = origin.observation().standalone_scope().unwrap();
        assert_eq!(scope.root, source);
        assert_eq!(scope.data_dir, source.join(".airk"));
        assert_eq!(scope.collection, "default");
        assert_eq!(scope.namespace, namespace);
        assert!(scope.data_dir_defaulted && scope.collection_defaulted);
        assert!(
            !source.exists(),
            "scope resolution created original source directories"
        );
        fs::write(
            &f.config,
            serde_json::to_vec(
                &json!({"mcpServers":{"airk-old":{"command":f.executable,"args":["mcp"]}}}),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            f.inspect()
                .unwrap()
                .observation()
                .standalone_scope()
                .is_err(),
            "current process CWD substituted for historical CWD"
        );
    }
}

fn triplet(root: &Path, data: &Path) -> Vec<String> {
    vec![
        "--root".into(),
        root.to_string_lossy().into_owned(),
        "--data-dir".into(),
        data.to_string_lossy().into_owned(),
        "--collection".into(),
        "old-original".into(),
        "mcp".into(),
    ]
}
struct Fixture {
    _temp: tempfile::TempDir,
    global: PathBuf,
    executable: PathBuf,
    config: PathBuf,
    argv: Vec<String>,
}
impl Fixture {
    fn new(version: &str) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let global = temp.path().join("Code/User/globalStorage");
        let platform = if cfg!(windows) {
            "win32-x64"
        } else if cfg!(target_os = "macos") {
            "darwin-x64"
        } else {
            "linux-x64"
        };
        let bytes = b"non-executable historical fixture; never run";
        let cache = global
            .join("iokaio.airk/helpers")
            .join(format!("{version}-{platform}-{}", &hex(bytes)[..16]));
        fs::create_dir_all(&cache).unwrap();
        let executable = cache.join(if cfg!(windows) { "airk.exe" } else { "airk" });
        fs::write(&executable, bytes).unwrap();
        let argv = triplet(&temp.path().join("source"), &temp.path().join("saved"));
        let config = temp.path().join("original-config.json");
        fs::write(&config,serde_json::to_vec(&json!({"mcpServers":{"airk-old":{"command":executable,"args":argv,"env":{"UNRELATED_SECRET":"never-copy-this"},"disabledTools":["write"]}}})).unwrap()).unwrap();
        Self {
            _temp: temp,
            global,
            executable,
            config,
            argv,
        }
    }
    fn locator(&self) -> ConfigLocator {
        ConfigLocator {
            path: self.config.clone(),
            format: ConfigFormat::McpServersJson,
            server: "airk-old".into(),
            container_path: None,
        }
    }
    fn inspect(&self) -> Result<NativeLegacyOrigin> {
        inspect_in(&self.locator(), std::slice::from_ref(&self.global))
    }
}
#[test]
fn captures_actual_original_bytes_without_approving_or_running_image() {
    for version in ["0.2.1-build.4", "1.0.0-build.7", "9.17.42"] {
        let f = Fixture::new(version);
        let mut proof = f.inspect().unwrap();
        let o = proof.observation();
        assert_eq!(o.provenance, "owned_legacy_helper_cache");
        assert_eq!(o.cache_version, version);
        assert!(!o.artifact_approved);
        assert_eq!(o.config_sha256, hex(&fs::read(&f.config).unwrap()));
        assert_eq!(o.executable_sha256, hex(&fs::read(&f.executable).unwrap()));
        assert_eq!(o.declaration.argv, f.argv);
        assert_eq!(o.declaration.collection.as_deref(), Some("old-original"));
        assert!(
            !serde_json::to_string(o)
                .unwrap()
                .contains("never-copy-this")
        );
        assert_eq!(proof.executable(), f.executable);
        proof.revalidate().unwrap();
    }
}
#[test]
fn keeps_after_role_index_selection_and_aliases_distinct() {
    let f = Fixture::new("1.2.2");
    let mut argv = f.argv.clone();
    argv.extend([
        "--index-collection".into(),
        "other-index".into(),
        "--additional-root".into(),
        format!("docs={}", f.global.display()),
        "--no-default-project".into(),
    ]);
    let d = normalize_argv(&argv).unwrap();
    assert_eq!(d.collection.as_deref(), Some("old-original"));
    assert_eq!(d.index_collection.as_deref(), Some("other-index"));
    assert!(d.no_default_project);
    assert_eq!(d.additional_roots[0].alias, "docs");
    assert_eq!(d.argv, argv);
}
#[test]
fn refuses_unknown_ambiguous_or_relative_authority() {
    let f = Fixture::new("1.1.4");
    for extra in [
        vec!["--root", "relative"],
        vec!["--unknown-scope", "all"],
        vec!["mcp"],
        vec!["--collection", "second"],
        vec!["--additional-root", "no-alias"],
    ] {
        let mut argv = f.argv.clone();
        argv.extend(extra.into_iter().map(str::to_owned));
        assert!(normalize_argv(&argv).is_err());
    }
    assert!(normalize_argv(&["mcp".into(), "--root".into(), "relative".into()]).is_err());
}
#[test]
fn does_not_collapse_unbound_default_or_managed_selection_to_project_triplet() {
    let d = normalize_argv(&["launch".into(), "--no-default-project".into()]).unwrap();
    assert!(d.root.is_none() && d.data_dir.is_none() && d.collection.is_none());
    assert!(d.no_default_project);
}
#[test]
fn supports_original_json_families_and_generated_codex_subset() {
    let f = Fixture::new("1.0.0");
    let entry = json!({"command":f.executable,"args":f.argv});
    for (format, value) in [
        (
            ConfigFormat::McpServersJson,
            json!({"mcpServers":{"airk":entry}}),
        ),
        (
            ConfigFormat::VscodeServersJson,
            json!({"servers":{"airk":{"type":"stdio","command":f.executable,"args":f.argv}}}),
        ),
        (
            ConfigFormat::OpenCodeJson,
            json!({"mcp":{"airk":{"type":"local","command":std::iter::once(f.executable.to_string_lossy().into_owned()).chain(f.argv.clone()).collect::<Vec<_>>()}}}),
        ),
    ] {
        let (command, args, _) =
            declaration_from_config(&serde_json::to_vec(&value).unwrap(), format, "airk").unwrap();
        assert_eq!(PathBuf::from(command), f.executable);
        assert_eq!(args, f.argv);
    }
    let text = format!(
        "# retained original\n[mcp_servers.\"airk old\"]\ncommand = {}\nargs = {}\nstartup_timeout_sec = 60\n",
        serde_json::to_string(&f.executable).unwrap(),
        serde_json::to_string(&f.argv).unwrap()
    );
    let (command, args, _) =
        declaration_from_config(text.as_bytes(), ConfigFormat::CodexToml, "airk old").unwrap();
    assert_eq!(PathBuf::from(command), f.executable);
    assert_eq!(args, f.argv);
}
#[test]
fn refuses_duplicate_json_and_toml_servers_or_fields() {
    assert!(
        declaration_from_config(
            br#"{"mcpServers":{"airk":{"command":"x","command":"y","args":[]}}}"#,
            ConfigFormat::McpServersJson,
            "airk"
        )
        .is_err()
    );
    assert!(codex_entry("[mcp_servers.airk]\ncommand=\"x\"\ncommand=\"y\"", "airk").is_err());
    assert!(
        codex_entry(
            "[mcp_servers.airk]\ncommand=\"x\"\n[mcp_servers.airk]\nargs=[]",
            "airk"
        )
        .is_err()
    );
}
#[test]
fn structural_toml_cannot_invent_servers_from_multiline_strings() {
    for quote in ["\"\"\"", "'''"] {
        let text = format!(
            "note = {quote}\n[mcp_servers.fake]\ncommand = 'airk'\nargs = ['mcp']\n{quote}\n"
        );
        assert!(codex_entry(&text, "fake").is_err());
    }
    let text = "[mcp_servers.'real server'] # trailing table comment\ncommand = 'airk' # literal string\nargs = [\n 'mcp', # inline array comment\n]\n[mcp_servers.'real server'.env]\nTOKEN = '''never-copy-secret'''\n";
    let entry = codex_entry(text, "real server").unwrap();
    assert_eq!(entry["command"], "airk");
    assert_eq!(entry["env"]["TOKEN"], "never-copy-secret");
    let changed = text.replace("never-copy-secret", "changed-secret");
    assert_ne!(
        hex(&serde_json::to_vec(&entry).unwrap()),
        hex(&serde_json::to_vec(&codex_entry(&changed, "real server").unwrap()).unwrap())
    );
}
#[test]
fn refuses_wrong_cache_prefix_and_noncanonical_global_storage() {
    let f = Fixture::new("1.0.0");
    assert!(inspect_in(&f.locator(), &[f.global.join("different")]).is_err());
    fs::write(&f.executable, b"different executable image").unwrap();
    assert!(f.inspect().is_err());
}
#[test]
fn detects_config_and_image_tampering_after_capture() {
    let f = Fixture::new("1.0.0");
    let mut proof = f.inspect().unwrap();
    fs::write(&f.config, b"{}").unwrap();
    assert!(proof.revalidate().is_err());
    let f = Fixture::new("1.0.0");
    let mut proof = f.inspect().unwrap();
    fs::write(&f.executable, b"replaced").unwrap();
    assert!(proof.revalidate().is_err());
}
#[test]
fn rejects_hardlinked_original_image() {
    let f = Fixture::new("1.0.0");
    fs::hard_link(&f.executable, f.global.join("aliased-airk")).unwrap();
    assert!(f.inspect().is_err());
}
#[test]
fn authenticates_existing_custom_editor_storage_without_trusting_a_location_claim() {
    let f = Fixture::new("0.2.1-build.4");
    let mut proof = inspect_at_global_storage(&f.locator(), &f.global).unwrap();
    assert_eq!(proof.observation().global_storage, f.global);
    assert_eq!(
        proof.observation().location_basis,
        "explicit_owned_editor_storage"
    );
    assert!(!proof.observation().artifact_approved);
    proof.revalidate().unwrap();
    assert!(inspect_at_global_storage(&f.locator(), &f.global.join("not-user-storage")).is_err());
    assert!(
        inspect_at_global_storage(
            &f.locator(),
            &f._temp.path().join("missing/User/globalStorage")
        )
        .is_err()
    );
}
#[test]
fn accepts_exact_owned_named_profile_storage() {
    let mut f = Fixture::new("1.0.0");
    let new = f
        .global
        .parent()
        .unwrap()
        .join("profiles/abc-123/globalStorage");
    fs::create_dir_all(new.parent().unwrap()).unwrap();
    fs::rename(&f.global, &new).unwrap();
    f.executable = new.join(f.executable.strip_prefix(&f.global).unwrap());
    f.global = new;
    fs::write(
        &f.config,
        serde_json::to_vec(
            &json!({"mcpServers":{"airk-old":{"command":f.executable,"args":f.argv}}}),
        )
        .unwrap(),
    )
    .unwrap();
    let proof = inspect_at_global_storage(&f.locator(), &f.global).unwrap();
    assert_eq!(proof.observation().global_storage, f.global);
    let default = f
        .global
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("globalStorage");
    assert!(inspect_in(&f.locator(), &[default]).is_ok());
}
#[cfg(unix)]
#[test]
fn rejects_symbolic_images_and_world_writable_files() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let f = Fixture::new("1.0.0");
    fs::set_permissions(&f.executable, fs::Permissions::from_mode(0o666)).unwrap();
    assert!(f.inspect().is_err());
    let f = Fixture::new("1.0.0");
    let real = f.global.join("moved");
    fs::rename(&f.executable, &real).unwrap();
    symlink(real, &f.executable).unwrap();
    assert!(f.inspect().is_err());
}
