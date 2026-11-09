// SPDX-License-Identifier: Apache-2.0
//! Actual modern worker/provider checkpoint preflight; explicitly supplied image.
use super::*;
use airk_core::service::{JobKind, JobOutcome};
use std::sync::atomic::{AtomicUsize, Ordering};

fn multiple_batch_source() -> String {
    (0..2800)
        .map(|line| format!("cobalt harbor original checkpoint passage {line}\n"))
        .collect()
}

pub(super) fn verify_modern_fixture_inputs(
    main: &Path,
    runtime: &Path,
    expected_source: &str,
    expected_manifest: &str,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Source {
        sha256: airk_core::identity::Sha256Digest,
        files: usize,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Build {
        schema: u32,
        source: Source,
        profile: String,
        runtime: crate::runtime_pair::RuntimePair,
        rustc: String,
        static_ort: serde_json::Map<String, serde_json::Value>,
        static_imports: serde_json::Map<String, serde_json::Value>,
        embedded_model_smoke: serde_json::Map<String, serde_json::Value>,
    }
    fn metadata_bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
        use std::io::Read;
        crate::install_files::safe_file(path)?;
        ensure!(
            std::fs::metadata(path)?.len() <= limit,
            "fixture metadata exceeds writer bound"
        );
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(limit + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= limit,
            "fixture metadata grew beyond writer bound"
        );
        Ok(bytes)
    }
    let expected_source: airk_core::identity::Sha256Digest = expected_source.parse()?;
    let expected_manifest: airk_core::identity::Sha256Digest = expected_manifest.parse()?;
    ensure!(
        crate::runtime_pair::payload_root(main)?.canonicalize()? == runtime.canonicalize()?,
        "crossed fixture payload root"
    );
    let bytes = metadata_bytes(&runtime.join("airk-install.json"), 512 * 1024)?;
    use sha2::{Digest, Sha256};
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == expected_manifest.to_string(),
        "crossed independently pinned install manifest"
    );
    let install: crate::runtime_install::InstallManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        install.schema == 1
            && install.version == env!("CARGO_PKG_VERSION")
            && install.os == std::env::consts::OS
            && install.arch == std::env::consts::ARCH,
        "crossed install manifest schema/version/host"
    );
    ensure!(
        (5..=2048).contains(&install.files.len()),
        "fixture inventory count differs from bounded writer contract"
    );
    for required in [
        crate::install_files::main_name(),
        "airk-runtime.json",
        "airk-build.json",
        "LICENSE",
    ] {
        ensure!(
            install.files.contains_key(required),
            "fixture inventory missing {required}"
        );
    }
    ensure!(
        install
            .files
            .keys()
            .any(|name| name.starts_with("notices/")),
        "fixture dependency notices missing"
    );
    let mut topology = std::collections::BTreeSet::from(["airk-install.json".to_owned()]);
    let mut portable = std::collections::BTreeSet::new();
    let mut total = 0u64;
    for (name, expected) in &install.files {
        ensure!(
            !name.is_empty()
                && !name.contains(['\\', ':'])
                && Path::new(name)
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_)))
                && name != "airk-install.json",
            "invalid fixture inventory path"
        );
        ensure!(
            !matches!(
                Path::new(name).file_name().and_then(|leaf| leaf.to_str()),
                Some(
                    "airk-embed"
                        | "airk-embed.exe"
                        | "airk-launcher"
                        | "airk-launcher.exe"
                        | "airk-launcher-image"
                        | "airk-service"
                        | "airk-service.exe"
                        | "airk-index"
                        | "airk-index.exe"
                        | "airk-source"
                        | "airk-source.exe"
                        | "onnxruntime.dll"
                        | "libonnxruntime.so"
                        | "libonnxruntime.dylib"
                )
            ),
            "modern fixture contains legacy process or dynamic inference image"
        );
        ensure!(
            portable.insert(name.to_ascii_lowercase()),
            "case-aliased fixture inventory"
        );
        let _: airk_core::identity::Sha256Digest = expected.parse()?;
        let file = runtime.join(name);
        crate::install_files::safe_file(&file)?;
        total = total
            .checked_add(std::fs::metadata(&file)?.len())
            .context("fixture inventory byte overflow")?;
        ensure!(
            total <= 4 * 1024 * 1024 * 1024,
            "fixture payload exceeds installer byte budget"
        );
        ensure!(
            crate::runtime_pair::digest_file(&file)? == *expected,
            "fixture seal mismatch: {name}"
        );
        topology.insert(name.clone());
        let mut ancestor = Path::new(name).parent();
        while let Some(directory) = ancestor.filter(|path| !path.as_os_str().is_empty()) {
            topology.insert(
                directory
                    .to_str()
                    .context("non-UTF-8 fixture path")?
                    .replace('\\', "/"),
            );
            ancestor = directory.parent();
        }
    }
    ensure!(
        topology
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == topology.len(),
        "case-aliased fixture topology"
    );
    let mut pending = vec![runtime.to_path_buf()];
    let mut observed = std::collections::BTreeSet::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            let relative = path
                .strip_prefix(runtime)?
                .to_str()
                .context("non-UTF-8 fixture entry")?
                .replace('\\', "/");
            ensure!(
                topology.contains(&relative) && observed.insert(relative),
                "unlisted fixture inventory entry"
            );
            ensure!(
                observed.len() <= 4096,
                "fixture topology exceeds installer bound"
            );
            let metadata = std::fs::symlink_metadata(&path)?;
            ensure!(
                !metadata.file_type().is_symlink(),
                "linked fixture inventory entry"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "redirected fixture inventory entry"
                );
            }
            if metadata.is_dir() {
                pending.push(path);
            } else {
                crate::install_files::safe_file(&path)?;
            }
        }
    }
    ensure!(
        observed == topology,
        "incomplete fixture inventory topology"
    );
    verify_fixture_image(main, true)?;
    let (_, _, pair) = crate::runtime_pair::read_installed_pair(main, env!("CARGO_PKG_VERSION"))?;
    let build_bytes = metadata_bytes(&runtime.join("airk-build.json"), 16_384)?;
    ensure!(
        format!("{:x}", Sha256::digest(&build_bytes)) == install.files["airk-build.json"],
        "build receipt changed after inventory verification"
    );
    let build: Build = serde_json::from_slice(&build_bytes)?;
    ensure!(
        build.schema == 1
            && build.source.sha256 == expected_source
            && (1..=1_000_000).contains(&build.source.files)
            && matches!(build.profile.as_str(), "debug" | "release")
            && !build.rustc.is_empty()
            && build.rustc.len() <= 4096,
        "crossed build receipt schema/source/profile/compiler"
    );
    ensure!(
        serde_json::to_value(&build.runtime)? == serde_json::to_value(&pair)?,
        "build receipt runtime differs from verified runtime pair"
    );
    // Complete writer-owned observations remain sealed evidence; qualification
    // of their contents belongs to the runtime builder rather than this reader.
    let _ = (
        build.static_ort,
        build.static_imports,
        build.embedded_model_smoke,
    );
    Ok(())
}

#[test]
#[ignore = "requires pinned modern AIRK_TEST_BINARY; read-only payload verification, no process"]
fn modern_candidate_receipts_match_pinned_build_without_starting_process() -> Result<()> {
    let main = PathBuf::from(
        std::env::var_os("AIRK_TEST_BINARY")
            .context("pin AIRK_TEST_BINARY to the sealed modern candidate")?,
    )
    .canonicalize()?;
    let runtime = crate::runtime_pair::payload_root(&main)?;
    let source = std::env::var("AIRK_TEST_NATIVE_SOURCE_SHA256")
        .context("pin the candidate original compiled-source SHA256")?;
    let manifest = std::env::var("AIRK_TEST_INSTALL_MANIFEST_SHA256")
        .context("pin the independently verified install-manifest SHA256")?;
    verify_modern_fixture_inputs(&main, runtime, &source, &manifest)
}

#[test]
fn modern_checkpoint_fixture_accepts_independent_install_runtime_and_build_schemas() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let runtime = owner.path().canonicalize()?;
    let main = runtime.join(crate::install_files::main_name());
    std::fs::create_dir_all(main.parent().unwrap())?;
    std::fs::write(&main, b"modern fixture image; never executed")?;
    std::fs::write(runtime.join("LICENSE"), b"original fixture license")?;
    std::fs::create_dir(runtime.join("notices"))?;
    std::fs::write(
        runtime.join("notices/fixture.txt"),
        b"original fixture notice",
    )?;
    let pair = crate::runtime_pair::RuntimePair {
        schema: 2,
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        airk_sha256: crate::runtime_pair::digest_file(&main)?,
        embed_sha256: String::new(),
    };
    let pair_bytes = serde_json::to_vec(&pair)?;
    std::fs::write(runtime.join("airk-runtime.json"), &pair_bytes)?;
    let expected_source = "a".repeat(64);
    let build = serde_json::json!({"schema":1,"source":{"sha256":expected_source,"files":1116},
        "profile":"debug","runtime":pair,"rustc":"original fixture compiler",
        "static_ort":{},"static_imports":{},"embedded_model_smoke":{}});
    let build_bytes = serde_json::to_vec(&build)?;
    std::fs::write(runtime.join("airk-build.json"), &build_bytes)?;
    let mut files = BTreeMap::new();
    for name in [
        crate::install_files::main_name(),
        "airk-runtime.json",
        "airk-build.json",
        "LICENSE",
        "notices/fixture.txt",
    ] {
        files.insert(
            name.to_owned(),
            crate::runtime_pair::digest_file(&runtime.join(name))?,
        );
    }
    let install = crate::runtime_install::InstallManifest {
        schema: 1,
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        files,
    };
    std::fs::write(
        runtime.join("airk-install.json"),
        serde_json::to_vec(&install)?,
    )?;
    let seal = crate::runtime_pair::digest_file(&runtime.join("airk-install.json"))?;
    verify_modern_fixture_inputs(&main, &runtime, &expected_source, &seal)?;
    assert!(verify_modern_fixture_inputs(&main, &runtime, &"b".repeat(64), &seal).is_err());
    assert!(verify_modern_fixture_inputs(&main, &runtime, "invalid", &seal).is_err());
    assert!(
        verify_modern_fixture_inputs(&main, &runtime, &expected_source, &"b".repeat(64)).is_err()
    );
    let original_install = serde_json::to_value(&install)?;
    let reseal = |manifest: &serde_json::Value| -> Result<String> {
        std::fs::write(
            runtime.join("airk-install.json"),
            serde_json::to_vec(manifest)?,
        )?;
        crate::runtime_pair::digest_file(&runtime.join("airk-install.json"))
    };
    for (field, changed) in [
        ("/schema", serde_json::json!(2)),
        ("/source/sha256", serde_json::json!("b".repeat(64))),
        ("/source/sha256", serde_json::json!("invalid")),
        ("/source/files", serde_json::json!(0)),
        ("/profile", serde_json::json!("unsupported")),
        ("/rustc", serde_json::json!("")),
        ("/runtime/airk_sha256", serde_json::json!("b".repeat(64))),
        ("/runtime/version", serde_json::json!("0.0.0")),
        ("/static_ort", serde_json::Value::Null),
    ] {
        let mut altered = build.clone();
        *altered
            .pointer_mut(field)
            .context("missing fixture build field")? = changed;
        std::fs::write(
            runtime.join("airk-build.json"),
            serde_json::to_vec(&altered)?,
        )?;
        let mut manifest = original_install.clone();
        manifest["files"]["airk-build.json"] =
            crate::runtime_pair::digest_file(&runtime.join("airk-build.json"))?.into();
        let changed_seal = reseal(&manifest)?;
        assert!(
            verify_modern_fixture_inputs(&main, &runtime, &expected_source, &changed_seal).is_err(),
            "accepted crossed build field {field}"
        );
    }
    std::fs::write(runtime.join("airk-build.json"), &build_bytes)?;
    for (field, changed) in [
        ("/schema", serde_json::json!(2)),
        ("/version", serde_json::json!("0.0.0")),
        ("/os", serde_json::json!("foreign")),
        ("/arch", serde_json::json!("foreign")),
        ("/files/airk-build.json", serde_json::json!("b".repeat(64))),
    ] {
        let mut manifest = original_install.clone();
        *manifest
            .pointer_mut(field)
            .context("missing fixture install field")? = changed;
        let changed_seal = reseal(&manifest)?;
        assert!(
            verify_modern_fixture_inputs(&main, &runtime, &expected_source, &changed_seal).is_err(),
            "accepted crossed install field {field}"
        );
    }
    let original_pair: serde_json::Value = serde_json::from_slice(&pair_bytes)?;
    for (field, changed) in [
        ("/schema", serde_json::json!(1)),
        ("/version", serde_json::json!("0.0.0")),
        ("/os", serde_json::json!("foreign")),
        ("/arch", serde_json::json!("foreign")),
        ("/airk_sha256", serde_json::json!("b".repeat(64))),
    ] {
        let mut altered = original_pair.clone();
        *altered
            .pointer_mut(field)
            .context("missing fixture runtime field")? = changed;
        std::fs::write(
            runtime.join("airk-runtime.json"),
            serde_json::to_vec(&altered)?,
        )?;
        let mut manifest = original_install.clone();
        manifest["files"]["airk-runtime.json"] =
            crate::runtime_pair::digest_file(&runtime.join("airk-runtime.json"))?.into();
        let changed_seal = reseal(&manifest)?;
        assert!(
            verify_modern_fixture_inputs(&main, &runtime, &expected_source, &changed_seal).is_err(),
            "accepted crossed runtime field {field}"
        );
    }
    std::fs::write(runtime.join("airk-runtime.json"), &pair_bytes)?;
    for name in ["../escape.txt", "airk-embed.exe", "missing.txt"] {
        let mut manifest = original_install.clone();
        manifest["files"][name] = "b".repeat(64).into();
        let changed_seal = reseal(&manifest)?;
        assert!(
            verify_modern_fixture_inputs(&main, &runtime, &expected_source, &changed_seal).is_err(),
            "accepted invalid inventory path {name}"
        );
    }
    let mut missing = original_install.clone();
    missing["files"]
        .as_object_mut()
        .unwrap()
        .remove("airk-build.json");
    let changed_seal = reseal(&missing)?;
    assert!(
        verify_modern_fixture_inputs(&main, &runtime, &expected_source, &changed_seal).is_err()
    );
    let original_seal = reseal(&original_install)?;
    std::fs::write(runtime.join("unlisted.txt"), b"unlisted fixture input")?;
    assert!(
        verify_modern_fixture_inputs(&main, &runtime, &expected_source, &original_seal).is_err()
    );
    std::fs::remove_file(runtime.join("unlisted.txt"))?;
    std::fs::write(&main, b"tampered candidate image")?;
    assert!(
        verify_modern_fixture_inputs(&main, &runtime, &expected_source, &original_seal).is_err()
    );
    Ok(())
}

#[test]
fn modern_checkpoint_fixture_verifies_single_main_and_legacy_requires_helper() -> Result<()> {
    use sha2::{Digest, Sha256};
    let owner = tempfile::tempdir()?;
    let main = owner.path().join(crate::install_files::main_name());
    std::fs::create_dir_all(main.parent().context("fixture main has no parent")?)?;
    let helper = main.parent().unwrap().join(if cfg!(windows) {
        "airk-embed.exe"
    } else {
        "airk-embed"
    });
    std::fs::write(&main, b"single main verification fixture")?;
    let mut pair = crate::runtime_pair::RuntimePair {
        schema: 2,
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        airk_sha256: format!("{:x}", Sha256::digest(std::fs::read(&main)?)),
        embed_sha256: String::new(),
    };
    let manifest = crate::runtime_pair::payload_root(&main)?.join("airk-runtime.json");
    std::fs::write(&manifest, serde_json::to_vec(&pair)?)?;
    assert!(!helper.exists());
    verify_fixture_image(&main, true)?;
    assert!(verify_fixture_image(&main, false).is_err());
    std::fs::write(&main, b"changed single main")?;
    assert!(verify_fixture_image(&main, true).is_err());
    pair.schema = 1;
    pair.airk_sha256 = format!("{:x}", Sha256::digest(std::fs::read(&main)?));
    pair.embed_sha256 = format!("{:x}", Sha256::digest(b"legacy helper fixture"));
    std::fs::write(&manifest, serde_json::to_vec(&pair)?)?;
    assert!(verify_fixture_image(&main, false).is_err());
    std::fs::write(&helper, b"legacy helper fixture")?;
    verify_fixture_image(&main, false)?;
    assert!(verify_fixture_image(&main, true).is_err());
    Ok(())
}

#[test]
fn checkpoint_fixture_requires_several_distinct_embedding_batches() -> Result<()> {
    let text = multiple_batch_source();
    let chunks = airk_ingest::chunk(&text);
    assert!(chunks.len() > 2 * airk_retrieval::embedding_batch::MAX_BATCH_CHUNKS);
    let distinct = chunks
        .iter()
        .map(|chunk| &chunk.text)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(distinct.len(), chunks.len());
    // Citation chunks alone do not prove several inference batches: the native
    // semantic planner may merge or split them. Exercise that actual planner
    // with the same fixed model used by the worker/provider preflight.
    let source = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    std::fs::write(source.path().join("many.txt"), &text)?;
    let mut payload = None;
    airk_ingest::scan_payloads_with_budgets(
        source.path(),
        data.path(),
        &airk_core::Snapshot::empty(source.path().canonicalize()?.to_string_lossy().into_owned()),
        &BTreeMap::new(),
        airk_ingest::ScanLimits::default(),
        &airk_ingest::adapters::profile::ExtractionPolicy {
            retained_profile: None,
            legacy_fallback_detail: None,
            detail: None,
            version: 1,
            rules: vec![],
        },
        false,
        |_| Ok(None),
        |_, artifact, _, extracted| {
            payload = extracted.cloned();
            Ok(artifact.clone())
        },
    )?;
    let vocabulary = br#"{"cobalt":[1.0,0.0],"harbor":[0.0,1.0]}"#;
    use sha2::{Digest, Sha256};
    std::fs::write(data.path().join("vocabulary.json"), vocabulary)?;
    std::fs::write(
        data.path().join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema": 1, "name": "guarded native workers", "license": "Apache-2.0",
            "sha256": format!("{:x}", Sha256::digest(vocabulary)), "dimensions": 2,
        }))?,
    )?;
    let model = airk_retrieval::model::EmbeddingModel::open(data.path())?;
    let mut inputs = std::collections::BTreeSet::new();
    let stats = airk_retrieval::vector_units::visit_vector_units(
        &payload.context("native text extraction omitted payload")?,
        &model,
        |unit| {
            inputs.insert(unit.input);
            Ok(())
        },
    )?;
    assert!(inputs.len() > 2 * airk_retrieval::embedding_batch::MAX_BATCH_CHUNKS);
    assert_eq!(stats.units, inputs.len() as u64);
    Ok(())
}

#[test]
fn qualification_fixtures_require_native_source_preparation() {
    fn direct_source_preparations(source: &str) -> Vec<&str> {
        let mut found = Vec::new();
        for draft in source.split("Draft::").skip(1) {
            let Some((variant, rest)) = draft.split_once('{') else {
                continue;
            };
            let variant = variant.trim();
            if !matches!(
                variant,
                "InstallModel"
                    | "ImportUsage"
                    | "ExtractionPolicy"
                    | "ScanLimits"
                    | "QuotaPolicy"
                    | "Calibrate"
                    | "RetainedProfile"
            ) {
                continue;
            }
            let mut depth = 1usize;
            for (offset, character) in rest.char_indices() {
                match character {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    let suffix = rest[offset + 1..].trim_start();
                    if suffix
                        .strip_prefix('.')
                        .map(str::trim_start)
                        .and_then(|suffix| suffix.strip_prefix("prepare"))
                        .is_some_and(|suffix| suffix.trim_start().starts_with('('))
                    {
                        found.push(variant);
                    }
                    break;
                }
            }
        }
        found
    }
    for source in [
        include_str!("project_supervisor_guarded_qualification_tests.rs"),
        include_str!("project_supervisor_modern_checkpoint_tests.rs"),
    ] {
        assert_eq!(direct_source_preparations(source), Vec::<&str>::new());
    }
    for variant in [
        "InstallModel",
        "ImportUsage",
        "ExtractionPolicy",
        "ScanLimits",
        "QuotaPolicy",
        "Calibrate",
        "RetainedProfile",
    ] {
        let stale = format!(
            "{}{} {{ directory: {{ original.clone() }} }}\n . prepare ()?",
            "Draft::", variant
        );
        assert_eq!(direct_source_preparations(&stale), [variant]);
        let canonical = format!(
            "prepare({}, {} {{ directory: original }})",
            "owner", "Draft::InstallModel"
        );
        assert!(direct_source_preparations(&canonical).is_empty());
    }
}

#[test]
#[ignore = "requires matched modern AIRK_TEST_BINARY; actual worker/model setup preflight"]
fn modern_fixture_prepares_model_and_publishes_before_checkpoint() -> Result<()> {
    let fixture = NativeFixture::build_modern().context("worker/model setup phase")?;
    executor()?
        .block_on(async {
            for index in 0..fixture.indices.len() {
                let mut stream = connect_scope(&fixture.control.owner, fixture.selection(index)?)
                    .await
                    .with_context(|| format!("setup verification: connect index {index}"))?;
                require_semantic_ready(&mut stream).await.with_context(|| {
                    format!("setup verification: semantic readiness index {index}")
                })?;
                search_scope(&mut stream, "vector", "cobalt harbor")
                    .await
                    .with_context(|| format!("setup verification: vector query index {index}"))?;
            }
            fixture.source_bytes_unchanged()
        })
        .context("worker/model setup verification phase")
}

#[test]
#[ignore = "requires matched modern AIRK_TEST_BINARY; actual worker/provider preflight"]
fn accepted_vectors_finish_later_batch_before_native_checkpoint_seals() -> Result<()> {
    let fixture = NativeFixture::build_modern().context("worker/model setup phase")?;
    accepted_vectors_checkpoint_phase(fixture)
        .context("held embedding/checkpoint verification phase")
}

fn accepted_vectors_checkpoint_phase(mut fixture: NativeFixture) -> Result<()> {
    let project = fixture.indices[0].registration.binding;
    let source = Path::new(&fixture.indices[0].registration.root).join("many.txt");
    let original = multiple_batch_source();
    std::fs::write(&source, &original)?;
    let key = fixture.key()?;
    let job =
        fixture
            .registry
            .enqueue(project, local_ipc::caller_identity()?, &key, JobKind::Index)?;
    fixture.control.owner.borrow_mut().run_job(job)?;
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let (entered, waiting) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    fixture
        .control
        .owner
        .borrow()
        .pool
        .as_ref()
        .context("native provider absent")?
        .set_compute_hook(Box::new(move |context, request| {
            if matches!(
                request,
                airk_retrieval::embedding_worker::Request::Embed { .. }
            ) {
                let call = observed.fetch_add(1, Ordering::SeqCst);
                if call == 0 {
                    entered.send(context.unwrap().worker).unwrap();
                    // Sender drop releases the actual call on every error path.
                    let _ = released.recv_timeout(Duration::from_secs(30));
                }
            }
        }))?;
    let key = fixture.key()?;
    let job = fixture.registry.enqueue(
        project,
        local_ipc::caller_identity()?,
        &key,
        JobKind::Vectors,
    )?;
    let allowance = {
        let mut owner = fixture.control.owner.borrow_mut();
        owner
            .resources
            .pending(project, job.id, job.kind, Instant::now())?;
        let allowance = owner
            .resources
            .next(Instant::now())?
            .context("native job not admitted")?;
        ensure!(allowance.job == job.id, "scheduler selected different job");
        owner
            .workers
            .get_mut(&project)
            .context("worker absent")?
            .run_job(allowance.clone())?;
        allowance
    };
    ensure!(
        waiting.recv_timeout(Duration::from_secs(10))? == Some(allowance.identity),
        "held compute crossed worker"
    );
    let operation = new_instance_id()?;
    let observe = |fixture: &mut NativeFixture,
                   project,
                   seal|
     -> Result<protocol::WorkerCheckpointObservation> {
        let mut owner = fixture.control.owner.borrow_mut();
        let worker = owner
            .workers
            .get_mut(&project)
            .context("checkpoint worker absent")?;
        let identity = worker
            .identity
            .context("checkpoint worker identity absent")?;
        service_ipc::write_frame(
            worker.input.as_mut().context("worker input closed")?,
            &WorkerCommand::Checkpoint {
                identity,
                operation_id: operation,
                seal,
            },
        )?;
        let Reply::Checkpoint { observation } =
            worker.output.recv_timeout(Duration::from_secs(5))?
        else {
            anyhow::bail!("native worker checkpoint response missing")
        };
        ensure!(
            observation.schema == 1
                && observation.identity == identity
                && observation.operation_id == operation
                && observation.admission_closed
                && observation.engine_sha256
                    == crate::runtime_pair::engine_identity_at(&fixture.main)?,
            "crossed native worker checkpoint response: {}",
            serde_json::to_string(&observation)?
        );
        Ok(observation)
    };
    let pending = observe(&mut fixture, project, false)?;
    ensure!(
        !pending.durable_boundary
            && !pending.sealed
            && pending.active_work_grant == Some(allowance.id),
        "accepted semantic work prematurely checkpointed: {}",
        serde_json::to_string(&pending)?
    );
    let provider = fixture
        .control
        .owner
        .borrow()
        .pool
        .as_ref()
        .unwrap()
        .provider_status()?
        .unwrap();
    ensure!(
        provider.admission_open
            && provider.running_outstanding == 1
            && provider.model_references_outstanding > 0,
        "checkpoint stopped admission needed by accepted later batches: {}",
        serde_json::to_string(&provider)?
    );
    ensure!(
        fixture.registry.job(job.id)?.outcome.is_none(),
        "held job already completed"
    );
    release.send(())?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = fixture
            .control
            .owner
            .borrow_mut()
            .workers
            .get_mut(&project)
            .unwrap()
            .work_status(None, false, None)?;
        ensure!(
            status.failure.is_none(),
            "accepted native vector job failed: {}",
            serde_json::to_string(&status)?
        );
        if status.finished == Some(allowance.id) {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "accepted vector job did not finish after release"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    fixture
        .control
        .owner
        .borrow_mut()
        .resources
        .finished(allowance.identity, allowance.id)?;
    let saved = fixture.registry.job(job.id)?;
    ensure!(
        matches!(saved.outcome, Some(JobOutcome::Succeeded { .. })) && saved.id == job.id,
        "original accepted vector receipt lost: {saved:?}"
    );
    ensure!(
        count.load(Ordering::SeqCst) >= 2,
        "fixture never needed a later embedding batch"
    );
    let sealed = observe(&mut fixture, project, true)?;
    ensure!(
        sealed.durable_boundary && sealed.sealed && sealed.active_work_grant.is_none(),
        "completed worker did not seal: {}",
        serde_json::to_string(&sealed)?
    );
    // Capture and seal every worker, including the untouched sibling, before
    // asking the real provider owner for the completion barrier.
    let identities = fixture
        .control
        .owner
        .borrow()
        .workers
        .values()
        .map(|worker| worker.identity.unwrap())
        .collect::<Vec<_>>();
    for identity in &identities {
        let observed = observe(&mut fixture, identity.project, true)?;
        ensure!(
            observed.durable_boundary && observed.sealed,
            "captured sibling is not sealed: {observed:?}"
        );
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let complete = fixture
            .control
            .owner
            .borrow_mut()
            .pool
            .as_mut()
            .unwrap()
            .checkpoint_drain(operation, identities.clone())?;
        if let Some(complete) = complete {
            ensure!(
                complete.state == airk_core::inference_provider::CompletionState::Complete
                    && complete.captured_workers == identities,
                "native completion crossed worker roster"
            );
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "native provider checkpoint remained pending"
        );
        std::thread::yield_now();
    }
    ensure!(
        std::fs::read(&source)? == original.as_bytes(),
        "checkpoint changed source bytes"
    );
    fixture.source_bytes_unchanged()?;
    eprintln!(
        "accepted Vectors {} completed {} actual embedding calls before native checkpoint",
        job.id,
        count.load(Ordering::SeqCst)
    );
    Ok(())
}
