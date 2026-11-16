// SPDX-License-Identifier: Apache-2.0
//! Same-weight, current-policy metadata staging. Never a product catalog emitter.
use super::*;
use airk_core::arbiter::{
    context_action::calibration::{CALIBRATION_SCHEMA_V2, ClassDisposition, ClassSupport},
    resource_policy::{ResourceFilePin, ResourcePolicy, ResourceRunInput},
};
use serde::Deserialize;
use serde_json::{Value, json};

mod compact_graph;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema: String,
    source_catalog: ResourceFilePin,
    native_build_receipt: ResourceFilePin,
    repository_root: PathBuf,
    acquisition_receipt: ResourceFilePin,
    asset_census: ResourceFilePin,
    asset_root: PathBuf,
    output: PathBuf,
    source_commit: String,
    source_files: Vec<ResourceFilePin>,
    #[serde(default)]
    compact_graph: Option<compact_graph::Input>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalCatalog {
    schema: String,
    profiles: Vec<Profile>,
}
fn pinned_metadata(pin: &ResourceFilePin, maximum: u64) -> Result<Vec<u8>> {
    pin.validate()?;
    let path = Path::new(&pin.path);
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        path.is_absolute()
            && meta.is_file()
            && !meta.file_type().is_symlink()
            && meta.len() <= maximum,
        "bounded absolute regular metadata source required"
    );
    let mut raw = Vec::new();
    fs::File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut raw)?;
    ensure!(
        raw.len() as u64 == meta.len() && sha(&raw) == pin.sha256,
        "frozen metadata source changed"
    );
    Ok(raw)
}
fn verify_sources(input: &Input) -> Result<()> {
    ensure!(
        !input.source_files.is_empty()
            && input.source_files.len() <= 128
            && input.source_commit.len() == 40
            && input
                .source_commit
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()),
        "native source identity required"
    );
    let mut paths = BTreeSet::new();
    for pin in &input.source_files {
        ensure!(paths.insert(&pin.path), "duplicate frozen source path");
        pinned_metadata(pin, 32 << 20)?;
    }
    Ok(())
}
fn verify_build_sources(input: &Input) -> Result<Sha256Digest> {
    ensure!(
        input.repository_root.is_absolute(),
        "absolute original repository root required"
    );
    let build: Value =
        serde_json::from_slice(&pinned_metadata(&input.native_build_receipt, 32 << 20)?)?;
    ensure!(
        build["schema"] == "airk-readiness-command/1"
            && build["exit_code"] == 0
            && build["inputs_unchanged"] == true
            && build["changed_groups"] == json!([])
            && build["runner_unchanged"] == true
            && build["runtime_unchanged"] == true
            && build["command_inputs_unchanged"] == true,
        "original native build receipt incomplete/drifted"
    );
    let group: Sha256Digest = serde_json::from_value(build["groups"]["native"]["sha256"].clone())?;
    let files = build["groups"]["native"]["files"]
        .as_object()
        .context("native build source files absent")?;
    for pin in &input.source_files {
        let relative = Path::new(&pin.path)
            .strip_prefix(&input.repository_root)
            .context("source file outside original repository")?;
        let key = relative
            .to_str()
            .context("native source path UTF-8")?
            .replace('\\', "/");
        ensure!(
            files.get(&key) == Some(&json!(pin.sha256)),
            "declared source differs from native build input seal"
        );
    }
    Ok(group)
}
fn pin(path: &Path) -> Result<ResourceFilePin> {
    let raw = fs::read(path)?;
    Ok(ResourceFilePin {
        path: path.to_str().context("native UTF-8 output path")?.into(),
        sha256: sha(&raw),
    })
}
fn context_bootstrap_v2(
    prepared: &mut PreparedProfile,
    directory: &Path,
    source_catalog: Sha256Digest,
    original: Sha256Digest,
    policy: &ResourcePolicy,
) -> Result<()> {
    let old = prepared
        .profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("raw calibration absent")?
        .clone();
    let old_file = prepared
        .files
        .get(&old.name)
        .context("raw calibration file absent")?;
    let raw = pinned_metadata(
        &ResourceFilePin {
            path: old_file.to_str().context("calibration path")?.into(),
            sha256: old.sha256,
        },
        64 << 10,
    )?;
    let task = prepared.profile.task_policies[&Task::ContextAction];
    let mut value = JointCalibration::parse_and_validate(&raw, &prepared.profile, task)?;
    ensure!(
        value.rules.len() == 1
            && value.rules[0].action == ContextActionClass::Read
            && value.rules[0].allowed_gaps == [Gap::Unsupported]
            && value.rules[0].allowed_reasons == [ContextReason::AdditionalEvidenceMayHelp]
            && value.rules[0].allowed_roles == [EvidenceRole::Unknown]
            && value.rules[0].allowed_relevance == [RelevanceHypothesis::Uncertain]
            && value.rules[0].minimum_raw_margin == f32::MAX,
        "only the native unqualified raw-score bootstrap may regenerate without labels"
    );
    value.schema = CALIBRATION_SCHEMA_V2.into();
    value.class_support = Some(vec![
        ClassDisposition {
            action: ContextActionClass::Read,
            support: ClassSupport::Supported {
                qualification: None,
            },
        },
        ClassDisposition {
            action: ContextActionClass::Sufficient,
            support: ClassSupport::Unsupported {
                reason:
                    "No suitable human-backed terminal qualification; raw-score evaluation only"
                        .into(),
            },
        },
        ClassDisposition {
            action: ContextActionClass::Clarify,
            support: ClassSupport::Unsupported {
                reason:
                    "No suitable human-backed terminal qualification; raw-score evaluation only"
                        .into(),
            },
        },
        ClassDisposition {
            action: ContextActionClass::NoJustifiedRead,
            support: ClassSupport::Unsupported {
                reason:
                    "No suitable human-backed terminal qualification; raw-score evaluation only"
                        .into(),
            },
        },
    ]);
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    let name = "context-action-calibration.json";
    fs::write(
        directory.join("source-prepared-profile.json"),
        serde_json::to_vec_pretty(&prepared.profile)?,
    )?;
    let asset = prepared
        .profile
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("raw calibration asset")?;
    asset.name = name.into();
    asset.bytes = bytes.len() as u64;
    asset.sha256 = sha(&bytes);
    prepared.files.remove(&old.name);
    prepared.files.insert(name.into(), directory.join(name));
    prepared.profile.calibration = sha(&bytes);
    prepared
        .profile
        .assets
        .sort_by(|left, right| left.name.cmp(&right.name));
    prepared.profile.manifest = Sha256Digest::from_bytes([0; 32]);
    prepared.profile.manifest = framed_digest(
        b"airk.private.context-metadata-regeneration-profile/2",
        &[
            &policy.canonical_bytes()?,
            source_catalog.as_bytes(),
            original.as_bytes(),
            &serde_json::to_vec(&prepared.profile)?,
        ],
    );
    prepared.profile.validate()?;
    JointCalibration::parse_and_validate(&bytes, &prepared.profile, task)?;
    fs::write(directory.join(name), bytes)?;
    fs::write(
        directory.join("experimental-profile.json"),
        serde_json::to_vec_pretty(&prepared.profile)?,
    )?;
    fs::write(
        directory.join("asset-files.private.json"),
        serde_json::to_vec_pretty(&prepared.files)?,
    )?;
    Ok(())
}

#[test]
#[ignore = "exact original native catalog/acquisition/census, same-user verified assets and fresh private output required; no NN or publication"]
fn emit_current_policy_profiles_from_verified_original_assets() -> Result<()> {
    let request_path = PathBuf::from(
        std::env::var_os("AIRK_CONTEXT_METADATA_REGEN_INPUT").context("metadata input missing")?,
    );
    let request_sha: Sha256Digest =
        std::env::var("AIRK_CONTEXT_METADATA_REGEN_INPUT_SHA256")?.parse()?;
    let request_pin = ResourceFilePin {
        path: request_path.to_str().context("native request path")?.into(),
        sha256: request_sha,
    };
    let input: Input = serde_json::from_slice(&pinned_metadata(&request_pin, 1 << 20)?)?;
    ensure!(
        ((input.schema == "airk.private.context-metadata-regeneration/1"
            && input.compact_graph.is_none())
            || (input.schema == "airk.private.context-metadata-regeneration/2"
                && input.compact_graph.is_some()))
            && input.output.is_absolute()
            && !input.output.exists()
            && input.asset_root.is_absolute(),
        "fresh private metadata-regeneration scope required"
    );
    let root_meta = fs::symlink_metadata(&input.asset_root)?;
    ensure!(
        root_meta.is_dir() && !root_meta.file_type().is_symlink(),
        "original asset root must be a retained ordinary directory"
    );
    verify_sources(&input)?;
    let native_source_group = verify_build_sources(&input)?;
    // These exact original receipts stay historical. Constructor authentication
    // below proves actual bytes; no copied Provision pass flag is trusted.
    pinned_metadata(&input.acquisition_receipt, 32 << 20)?;
    pinned_metadata(&input.asset_census, 32 << 20)?;
    let original: OriginalCatalog =
        serde_json::from_slice(&pinned_metadata(&input.source_catalog, 1 << 20)?)?;
    let expected = BTreeSet::from([
        ModelChoice::FocusQwenReranker06B,
        ModelChoice::ContextQwenCompact,
        ModelChoice::ContextQwenLarge,
    ]);
    ensure!(
        original.schema == CATALOG_SCHEMA
            && original.profiles.len() == 3
            && original
                .profiles
                .iter()
                .map(|profile| profile.model)
                .collect::<BTreeSet<_>>()
                == expected,
        "exact original selected native profile roster required"
    );
    for profile in &original.profiles {
        profile.validate()?;
        profile.validate_release_downloads()?;
    }
    let policy = ResourcePolicy::software_v2();
    policy.validate()?;
    let policy_digest = policy.digest()?;
    let started = std::time::Instant::now();
    let started_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let binary = std::env::current_exe()?;
    let mut binary_file = fs::File::open(&binary)?;
    let mut binary_hash = Sha256::new();
    let mut binary_buffer = [0_u8; 65536];
    loop {
        let count = binary_file.read(&mut binary_buffer)?;
        if count == 0 {
            break;
        }
        binary_hash.update(&binary_buffer[..count]);
    }
    let binary_sha256 = Sha256Digest::from_bytes(binary_hash.finalize().into());
    fs::create_dir(&input.output)?;
    let mut rows = Vec::new();
    let report = |complete: bool, rows: &[Value], error: Option<String>| {
        json!({
        "schema":"airk.private.context-metadata-regeneration-report/1","complete":complete,"error":error,
        "input":request_pin,"source_commit":input.source_commit,"source_files":input.source_files,
        "source_catalog":input.source_catalog,"native_build_receipt":input.native_build_receipt,"native_source_group":native_source_group,"original_acquisition_receipt":input.acquisition_receipt,"original_asset_census":input.asset_census,
        "resource_policy":policy,"resource_policy_digest":policy_digest,"native_binary":binary,"native_binary_sha256":binary_sha256,"started_unix_ms":started_unix_ms,"elapsed_us":started.elapsed().as_micros(),"profiles":rows,
        "NN_invocations":0,"task_qualified":false,"release_qualified":false,"public_source_available":false,
        "limits":"Private same-weight metadata staging only. Old2048 numeric fixtures remain historical controls. New metadata has no product download authority. Context terminals remain unsupported; Read uses an extreme raw-score-only bootstrap, not fitted positive evidence."})
    };
    let report_path = input
        .output
        .join("metadata-regeneration-report.private.json");
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report(false, &rows, None))?,
    )?;
    let run = (|| -> Result<()> {
        for old in original.profiles {
            if input.compact_graph.is_some() && old.model != ModelChoice::ContextQwenCompact {
                continue;
            }
            let model_name = serde_json::to_value(old.model)?
                .as_str()
                .context("native model name")?
                .to_owned();
            let directory = input.output.join(&model_name);
            let originals = input.asset_root.join(old.manifest.to_string());
            let mut source_assets: Vec<AssetInput> = old
                .assets
                .iter()
                .filter(|asset| !matches!(asset.role, AssetRole::Compiler | AssetRole::Calibration))
                .map(|asset| AssetInput {
                    asset: asset.clone(),
                    path: originals.join(&asset.name),
                })
                .collect();
            if let Some(graph) = &input.compact_graph {
                compact_graph::apply(graph, &mut source_assets)?;
            }
            let mut prepared = prepare_with_limits(
                ProfileInput {
                    model: old.model,
                    profile_id: if input.compact_graph.is_some() {
                        "private-compact-fp32-gqa-diagnostic-v1".into()
                    } else {
                        format!("private-final-policy-v2-{model_name}")
                    },
                    assets: source_assets,
                    cpu_fit: old.cpu_fit.clone().context("original declared CpuFit")?,
                    memory_reservation_bytes: old.memory_reservation_bytes,
                    peak_disk_bytes: old.peak_disk_bytes,
                    maximum_candidates: old.max_candidates,
                    calibration: CalibrationInput::NumericRawOnly,
                },
                &policy,
                &directory,
            )?;
            ensure!(
                prepared.resource_policy.as_ref() == Some(&policy),
                "native preparation lost exact resource policy"
            );
            if old.declared_stage() == Some(Stage::Focus) {
                ensure!(
                    prepared.profile.max_tokens == 512
                        && prepared.profile.compiler == old.compiler
                        && prepared.profile.calibration == old.calibration
                        && prepared.profile.policy == old.policy
                        && prepared.profile.task_policies == old.task_policies,
                    "Focus source/metadata changed; independent original public availability cannot transfer"
                );
            } else {
                ensure!(
                    prepared.profile.max_tokens == 8192
                        && prepared.profile.compiler != old.compiler
                        && prepared.profile.task_policies != old.task_policies,
                    "Context must have newly emitted width and current task policy; old identities cannot transfer"
                );
            }
            if old.declared_stage() == Some(Stage::Context) {
                context_bootstrap_v2(
                    &mut prepared,
                    &directory,
                    input.source_catalog.sha256,
                    old.manifest,
                    &policy,
                )?;
            }
            if let Some(graph) = &input.compact_graph {
                compact_graph::validate_prepared(graph, &prepared)?;
            }
            ensure!(
                prepared.profile.qualification.is_none()
                    && prepared.profile.qualified_targets.is_empty()
                    && prepared.profile.downloads.is_empty(),
                "metadata regeneration cannot grant release/network authority"
            );
            for asset in old
                .assets
                .iter()
                .filter(|asset| !matches!(asset.role, AssetRole::Compiler | AssetRole::Calibration))
            {
                let current = prepared
                    .profile
                    .assets
                    .iter()
                    .find(|current| current.name == asset.name)
                    .context("original neural/notice/fixture asset absent")?;
                if asset.role == AssetRole::Model && input.compact_graph.is_some() {
                    ensure!(
                        current.sha256
                            == input
                                .compact_graph
                                .as_ref()
                                .context("graph override")?
                                .graph
                                .sha256,
                        "new graph asset identity differs"
                    );
                    continue;
                }
                ensure!(
                    serde_json::to_value(current)? == serde_json::to_value(asset)?,
                    "regeneration changed an original neural/tokenizer/notice/fixture identity"
                );
            }
            let names = prepared
                .profile
                .assets
                .iter()
                .map(|asset| &asset.name)
                .collect::<BTreeSet<_>>();
            ensure!(
                prepared.files.keys().collect::<BTreeSet<_>>() == names,
                "native regenerated asset map differs"
            );
            let profile_pin = pin(&directory.join("experimental-profile.json"))?;
            let files = prepared
                .files
                .iter()
                .map(|(name, path)| {
                    Ok((
                        name.clone(),
                        path.to_str().context("native asset path")?.to_owned(),
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            let request = ResourceRunInput {
                schema: "airk.arbiter.resource-run/2".into(),
                policy: policy.clone(),
                policy_digest: policy.digest()?,
                source_manifest: request_pin.clone(),
                profile: profile_pin.clone(),
                files,
                numeric_fixture: None,
            };
            request.validate()?;
            fs::write(
                directory.join("resource-run-input.private.json"),
                serde_json::to_vec_pretty(&request)?,
            )?;
            let mut row = json!({"model":old.model,"original_manifest":old.manifest,"profile":profile_pin,
                "manifest":prepared.profile.manifest,"compiler":prepared.profile.compiler,"calibration":prepared.profile.calibration,
                "task_policies":prepared.profile.task_policies,"max_tokens":prepared.profile.max_tokens,
                "asset_files":pin(&directory.join("asset-files.private.json"))?,"resource_run_input":pin(&directory.join("resource-run-input.private.json"))?,
                "original_fixture_is_new8192_qualification":false,"new_neural_weights":false,"terminal_classes_supported":false,"task_qualified":false,"release_qualified":false});
            if let Some(graph) = &input.compact_graph {
                row["new_model_graph"] = json!(true);
                row["graph_override"] = serde_json::to_value(graph)?;
            }
            rows.push(row);
            fs::write(
                &report_path,
                serde_json::to_vec_pretty(&report(false, &rows, None))?,
            )?;
        }
        verify_sources(&input)?;
        ensure!(
            verify_build_sources(&input)? == native_source_group,
            "native build source changed during preparation"
        );
        Ok(())
    })();
    fs::write(
        &report_path,
        serde_json::to_vec_pretty(&report(
            run.is_ok(),
            &rows,
            run.as_ref().err().map(ToString::to_string),
        ))?,
    )?;
    run
}
