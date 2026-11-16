// SPDX-License-Identifier: Apache-2.0
//! Ignored native controls for a separately owned public matched-runtime journey.
use super::*;
use airk_core::{
    arbiter::{Enrollment, ManagementRequest, ModelChoice, Stage, StageConfiguration, Task},
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
    service_resources::{ResourceConfiguration, ResourcePreset},
};
use airk_process::directory::OpenedDirectory;
use anyhow::bail;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, io::Write, path::PathBuf};

const PREFIX: &str = "AIRK_PUBLIC_ARBITER_";
fn input(name: &str) -> Result<String> {
    std::env::var(format!("{PREFIX}{name}")).with_context(|| format!("{PREFIX}{name} missing"))
}
fn path(name: &str) -> Result<PathBuf> {
    let original = PathBuf::from(input(name)?);
    ensure!(
        original.is_absolute(),
        "public journey path is not absolute"
    );
    original.canonicalize().map_err(Into::into)
}
fn hash(path: &Path) -> Result<Sha256Digest> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(Sha256Digest::from_bytes(hasher.finalize().into()))
}
fn artifact_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("artifacts")
        .canonicalize()
        .map_err(Into::into)
}
fn write_new(output: &Path, name: &str, value: &impl Serialize) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join(name))?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}
#[derive(Clone, Copy)]
enum ConfigureMode {
    Enable,
    Disable,
    Unenroll,
}
fn focus_request(
    revision: u64,
    selected: Sha256Digest,
    index: IndexId,
    mode: ConfigureMode,
) -> Result<ManagementRequest> {
    ensure!(revision > 0, "public Configure revision must be positive");
    let stage = StageConfiguration {
        enabled: matches!(mode, ConfigureMode::Enable),
        selected_profile: Some(selected),
        indexes: if matches!(mode, ConfigureMode::Unenroll) {
            vec![]
        } else {
            vec![Enrollment {
                index_id: index,
                tasks: BTreeSet::from([Task::FocusRank]),
            }]
        },
    };
    stage.validate(Stage::Focus)?;
    Ok(ManagementRequest::Configure {
        expected_revision: revision,
        stage: Stage::Focus,
        enabled: stage.enabled,
        selected_profile: stage.selected_profile,
        indexes: stage.indexes,
    })
}
fn fixture_resources() -> Result<ResourceConfiguration> {
    let configuration = ResourceConfiguration {
        preset: ResourcePreset::Throughput,
        memory_bytes: Some(64 << 30),
        inference_threads: Some(4),
        ..ResourceConfiguration::default()
    };
    configuration.validate()?;
    Ok(configuration)
}
fn bound_index(control: &Path, health: &ServiceHealth) -> Result<IndexId> {
    let original = path("SOURCE_ROOT")?;
    let expected: IndexId = input("INDEX")?.parse()?;
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
    let catalog = registry
        .catalog_state()?
        .context("public catalog missing")?;
    ensure!(
        catalog.activated
            && health
                .catalog_clock
                .is_some_and(|clock| clock.namespace == catalog.namespace
                    && clock.revision == catalog.revision),
        "public catalog identity differs"
    );
    let matches = registry
        .catalog_clean_break_sources(catalog.revision)?
        .into_iter()
        .filter(|(definition, _, _)| definition.id == expected)
        .collect::<Vec<_>>();
    ensure!(matches.len() == 1, "public index is absent or ambiguous");
    let (definition, sources, _) = &matches[0];
    ensure!(
        sources.len() == 1
            && Path::new(&sources[0].canonical_path).canonicalize()? == original
            && Path::new(&definition.backing.root).canonicalize()? == original,
        "public index source/backing differs from original fixture root"
    );
    ensure!(
        registry
            .catalog_state()?
            .is_some_and(|current| current.namespace == catalog.namespace
                && current.revision == catalog.revision),
        "public catalog changed during index observation"
    );
    Ok(expected)
}
async fn status(control: &Path, instance: Incarnation) -> Result<airk_core::arbiter::Status> {
    match crate::ipc::control::request_once(
        control,
        ControlMethod::Arbiter {
            request: ManagementRequest::Status,
        },
        env!("CARGO_PKG_VERSION"),
        Some(instance),
    )
    .await?
    {
        ControlResponse::Arbiter { observation } => {
            observation.validate()?;
            Ok(*observation)
        }
        other => bail!(
            "actual public Status refused: {}",
            serde_json::to_string(&other)?
        ),
    }
}

/// Exact executable, PID, instance and user pins are caller-supplied from the
/// harness that started the service. No private fixture attachment is accepted.
#[tokio::test]
#[ignore = "actual public matched-runtime control; explicit process/artifact pins and fresh receipt required"]
async fn actual_public_arbiter_control() -> Result<()> {
    ensure!(
        crate::ipc::endpoint::current_user()? == input("USER_SID")?,
        "controller effective user differs from frozen human SID"
    );
    let output = path("OUTPUT")?;
    ensure!(
        output.starts_with(artifact_root()?),
        "public receipt leaves artifacts"
    );
    let _output_pin = OpenedDirectory::open(&output)?;
    ensure!(
        std::fs::read_dir(&output)?.next().is_none(),
        "public receipt directory is not fresh"
    );
    let started = std::time::Instant::now();
    let mut receipt = serde_json::json!({
        "schema":"airk.public-arbiter-control-run/1", "release_qualified":false,
        "controller_sha256":airk_host::runtime_pair::current_executable_sha256()?,
        "process_or_lease_cleanup_claimed":false, "passed":false,
    });
    let result = async {
        ensure!(env!("CARGO_PKG_VERSION") == "1.8.207", "controller version differs");
        let control = path("CONTROL")?;
        ensure!(!control.join("ready.json").try_exists()?, "private fixture record is not public admission");
        let runtime = path("RUNTIME")?;
        let image = runtime.join("airk.exe");
        let cli = runtime.join("airkctl.exe");
        let expected_image: Sha256Digest = input("SERVICE_SHA256")?.parse()?;
        let expected_cli: Sha256Digest = input("CLI_SHA256")?.parse()?;
        let expected_install: Sha256Digest = input("INSTALL_SHA256")?.parse()?;
        ensure!(hash(&image)? == expected_image && hash(&cli)? == expected_cli
            && hash(&runtime.join("airk-install.json"))? == expected_install,
            "public matched artifact bytes differ from frozen inputs");
        let (_, _, pair) = airk_host::runtime_pair::read_installed_pair(&image, "1.8.207")?;
        ensure!(pair.airk_sha256 == expected_image.to_string()
            && pair.split.as_ref().is_some_and(|split| split.airkctl_sha256 == expected_cli),
            "native matchedpair metadata differs from both executable pins");
        let runtime_sha256 = hash(&runtime.join("airk-runtime.json"))?;
        let expected_source: Sha256Digest = input("SOURCE_SHA256")?.parse()?;
        let expected_instance: Incarnation = input("INSTANCE")?.parse()?;
        let expected_pid: u32 = input("PID")?.parse()?;
        let expected_user = input("USER_SID")?;
        ensure!(expected_pid > 0 && expected_pid != std::process::id(), "public service PID differs");
        #[cfg(windows)]
        ensure!(crate::ipc::endpoint::process_user(expected_pid)? == expected_user,
            "actual public service process user differs from human SID");
        let response = crate::ipc::control::request_once(
            &control, ControlMethod::Health, env!("CARGO_PKG_VERSION"), Some(expected_instance),
        ).await?;
        let ControlResponse::Health { observation: health } = response else {
            bail!("public native Health refused")
        };
        ensure!(health.version == "1.8.207" && health.instance == expected_instance
            && health.process_id == expected_pid && health.user == expected_user
            && health.executable_sha256 == expected_image.to_string()
            && health.build.as_ref().is_some_and(|build| build.source_sha256 == expected_source),
            "public service Health differs from authentic process/artifact pins");
        receipt["health"] = serde_json::to_value(&health)?;
        receipt["artifacts"] = serde_json::json!({"runtime":runtime,"service_sha256":expected_image,
            "cli_sha256":expected_cli,"install_sha256":expected_install,
            "runtime_sha256":runtime_sha256,"source_sha256":expected_source,"native_pair":pair});
        let operation = input("OPERATION")?;
        receipt["operation"] = operation.clone().into();
        let observed = status(&control, expected_instance).await?;
        receipt["before"] = serde_json::to_value(&observed)?;
        let request = if operation == "resources" {
            ControlMethod::Resources
        } else if operation == "configure-resources" {
            ensure!(input("RESOURCE_CONSENT")? == "true", "fixture resource consent missing");
            ControlMethod::ConfigureResources { configuration: fixture_resources()? }
        } else if operation == "initialize" {
            ensure!(observed.initialization.is_some(), "public state already initialized");
            ControlMethod::Arbiter { request: ManagementRequest::Initialize {
                expected_legacy_configuration: observed.initialization.as_ref()
                    .and_then(|value| value.actual_legacy_digest),
            } }
        } else if operation == "status" {
            ControlMethod::Arbiter { request: ManagementRequest::Status }
        } else {
            let selected: Sha256Digest = input("PROFILE")?.parse()?;
            let profile = airk_retrieval::arbiter::runtime::catalog_profiles()?.into_iter()
                .find(|profile| profile.manifest == selected
                    && profile.model == ModelChoice::FocusQwenReranker06B)
                .context("public Focus is outside compiled catalog")?;
            ensure!(profile.max_tokens == 512
                && profile.tasks == BTreeSet::from([Task::FocusRank])
                && observed.focus.profiles.iter().any(|row| row.profile == Some(selected)),
                "actual product does not advertise exact compiled512 Focus profile");
            let revision = observed.configuration.revision;
            match operation.as_str() {
                "enable" | "disable" | "unenroll" | "stale-configure" => {
                    let index = bound_index(&control, &health)?;
                    let mode = match operation.as_str() {
                        "enable" | "stale-configure" => ConfigureMode::Enable,
                        "disable" => ConfigureMode::Disable,
                        _ => ConfigureMode::Unenroll,
                    };
                    let revision = if operation == "stale-configure" {
                        revision.checked_sub(1).filter(|value| *value > 0)
                            .context("no positive stale revision exists")?
                    } else { revision };
                    ControlMethod::Arbiter { request: focus_request(revision, selected, index, mode)? }
                }
                "import" => {
                    let directory = PathBuf::from(input("IMPORT_DIRECTORY")?);
                    let request = airk_core::native_offline_import::NativeOfflineImportRequest {
                        expected_revision: revision, profile: selected,
                        directory: directory.to_str().context("native import directory encoding")?.into(),
                    };
                    request.validate()?;
                    ControlMethod::ArbiterImport { request: Box::new(request) }
                }
                "provision" => {
                    ensure!(input("DOWNLOAD_CONSENT")? == "true", "explicit public download consent missing");
                    ControlMethod::Arbiter { request: ManagementRequest::Provision { expected_revision:revision,profile:selected } }
                }
                "remove" => ControlMethod::Arbiter { request: ManagementRequest::Remove { expected_revision:revision,profile:selected } },
                "cancel" => ControlMethod::Arbiter { request: ManagementRequest::Cancel { expected_revision:revision,stage:Stage::Focus } },
                "recover" => ControlMethod::Arbiter { request: ManagementRequest::Recover { expected_revision:revision,stage:Stage::Focus } },
                _ => bail!("unsupported explicit public control operation"),
            }
        };
        receipt["request"] = serde_json::to_value(&request)?;
        let response = crate::ipc::control::request_once(
            &control, request, env!("CARGO_PKG_VERSION"), Some(expected_instance),
        ).await?;
        receipt["response"] = serde_json::to_value(&response)?;
        let accepted = matches!(response,ControlResponse::Arbiter { .. }
            | ControlResponse::ArbiterImport { .. } | ControlResponse::Resources { configuration_error:false,.. });
        if operation == "stale-configure" {
            ensure!(matches!(&response,ControlResponse::Failure { code } if code == "arbiter_unavailable"), "stale public Configure lacked actual native refusal");
            let after = status(&control, expected_instance).await?;
            ensure!(after.configuration == observed.configuration, "stale Configure changed actual state");
            receipt["after"] = serde_json::to_value(&after)?;
        } else {
            ensure!(accepted, "actual public control was refused; inspect retained native response");
            if operation == "configure-resources" {
                let ControlResponse::Resources { observation, configuration_error:false,.. } = &response else {
                    bail!("resource configuration did not return actual snapshot")
                };
                ensure!(observation.configuration.preset == ResourcePreset::Throughput
                    && observation.configuration.memory_bytes == Some(64 << 30)
                    && observation.configuration.inference_threads == Some(4),
                    "actual resource configuration differs from explicit fixture request");
            }
        }
        let resources = crate::ipc::control::request_once(
            &control, ControlMethod::Resources, env!("CARGO_PKG_VERSION"), Some(expected_instance),
        ).await?;
        receipt["actual_resources_after"] = serde_json::to_value(&resources)?;
        ensure!(matches!(resources, ControlResponse::Resources { configuration_error:false,.. }),
            "actual post-operation resource/pressure observation unavailable");
        Ok::<(),anyhow::Error>(())
    }.await;
    receipt["elapsed_us"] = serde_json::to_value(started.elapsed().as_micros())?;
    receipt["passed"] = result.is_ok().into();
    if let Err(error) = &result {
        receipt["failure"] = format!("{error:#}").into();
    }
    write_new(&output, "receipt.json", &receipt)?;
    result
}

#[test]
fn public_focus_control_uses_exact_enrollment_and_explicit_off_states() -> Result<()> {
    let index = IndexId::from_identity(Incarnation::from_bytes([37; 16]));
    let profile = Sha256Digest::from_bytes([91; 32]);
    for (mode, enabled, count) in [
        (ConfigureMode::Enable, true, 1),
        (ConfigureMode::Disable, false, 1),
        (ConfigureMode::Unenroll, false, 0),
    ] {
        let request = focus_request(7, profile, index, mode)?;
        let ManagementRequest::Configure {
            expected_revision,
            stage,
            enabled: actual,
            selected_profile,
            indexes,
        } = request
        else {
            bail!("not Configure")
        };
        assert_eq!(expected_revision, 7);
        assert_eq!(stage, Stage::Focus);
        assert_eq!(actual, enabled);
        assert_eq!(selected_profile, Some(profile));
        assert_eq!(indexes.len(), count);
        if count == 1 {
            assert_eq!(indexes[0].index_id, index);
            assert_eq!(indexes[0].tasks, BTreeSet::from([Task::FocusRank]));
        }
    }
    assert!(focus_request(0, profile, index, ConfigureMode::Enable).is_err());
    let resource = fixture_resources()?;
    assert_eq!(resource.memory_bytes, Some(68719476736));
    assert_eq!(resource.inference_threads, Some(4));
    assert_eq!(resource.preset, ResourcePreset::Throughput);
    Ok(())
}
