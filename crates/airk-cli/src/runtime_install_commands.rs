// SPDX-License-Identifier: Apache-2.0
//! Trusted CLI actions for the offline runtime installer and launcher.
use super::*;

pub async fn run(command: &RuntimeCommand) -> Result<()> {
    let output = match command {
        RuntimeCommand::LoginForeground { channel } => {
            #[cfg(target_os = "macos")]
            {
                let root = macos_login::foreground_root(*channel)?;
                return Box::pin(run(&RuntimeCommand::Foreground { install_dir: root })).await;
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = channel;
                bail!("Mac login entry is unavailable on this platform");
            }
        }
        RuntimeCommand::MacLogin {
            install_dir,
            runtime_sha256,
            channel,
            action,
        } => {
            #[cfg(target_os = "macos")]
            {
                macos_login::native(install_dir, runtime_sha256, *channel, action)?
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (install_dir, runtime_sha256, channel, action);
                bail!("Mac login API is unavailable on this platform");
            }
        }
        RuntimeCommand::ContainerHost {
            source,
            approved_sha256,
            channel,
        } => {
            ensure!(
                cfg!(target_os = "linux"),
                "container host requires Linux; native desktop operation uses the installed runtime"
            );
            let source = source.canonicalize()?;
            ensure!(
                std::env::current_exe()?.canonicalize()?
                    == source.join(main_name()).canonicalize()?,
                "container host must run the approved image's own main executable"
            );
            let paths = crate::user_paths::UserPaths::discover(*channel)?;
            managed::configure(
                &source,
                &paths.install,
                approved_sha256,
                VerificationBasis::TrustedDistribution,
                ManagedLifecycle::External,
                Some(&paths.state),
                *channel,
            )
            .await?;
            // The container is the explicit external supervisor. It starts this
            // already verified image directly; on-demand/user login APIs remain disabled.
            return service_runtime::serve(&layout::control(&paths.install)?).await;
        }
        RuntimeCommand::Location { channel } => {
            let paths = crate::user_paths::UserPaths::discover(*channel)?;
            serde_json::json!({"install_dir":paths.install,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"channel":channel,"protocol":1,"storage_formats":airk_core::storage_format::CURRENT,"user_paths":paths})
        }
        RuntimeCommand::Login {
            install_dir,
            action,
        } => {
            managed::require_unmanaged(install_dir)?;
            let manager = crate::service_manager::ServiceManager::for_runtime(install_dir)?;
            let _lock = if matches!(action, LoginCommand::Plan | LoginCommand::Status) {
                None
            } else {
                Some(install_lock(install_dir)?)
            };
            if !matches!(
                action,
                LoginCommand::Plan | LoginCommand::Status | LoginCommand::Stop
            ) {
                payload_repair::require_settled(install_dir)?;
                activation::require_settled(install_dir)?;
            }
            match action {
                LoginCommand::Plan => serde_json::to_value(manager.plan()?)?,
                LoginCommand::Status => serde_json::to_value(manager.status()?)?,
                LoginCommand::Install => {
                    let (_, main, approval) = selected(install_dir)?;
                    drain(install_dir, &main, &approval).await?;
                    serde_json::to_value(manager.install()?)?
                }
                LoginCommand::Start => serde_json::to_value(manager.start()?)?,
                LoginCommand::Stop | LoginCommand::Uninstall => serde_json::to_value(
                    stop_login(
                        install_dir,
                        &manager,
                        matches!(action, LoginCommand::Uninstall),
                    )
                    .await?,
                )?,
            }
        }
        RuntimeCommand::Foreground { install_dir } => {
            if let Some(policy) = managed::read(install_dir)? {
                ensure!(
                    policy.lifecycle == ManagedLifecycle::UserOnDemand,
                    "organization supervisor owns this runtime; AIRK cannot start another foreground instance"
                );
            }
            launcher_repair::require_settled(install_dir)?;
            let (executable, _) = startup_selected(install_dir)?;
            let current = std::env::current_exe()?.canonicalize()?;
            if current != executable.canonicalize()?
                && runtime_pair::digest_file(&current)? != runtime_pair::digest_file(&executable)?
            {
                // A different implementation must delegate lifecycle interpretation.
                // An exact verified launcher copy already has that implementation
                // and can dispatch serve without another complete payload scan.
                let mut command = Command::new(executable);
                command
                    .arg("runtime")
                    .arg("foreground")
                    .arg("--install-dir")
                    .arg(install_dir);
                return crate::native_process::forward_stdio(command);
            }
            let mut command = Command::new(executable);
            command
                .arg("serve")
                .arg("--runtime-dir")
                .arg(layout::control(install_dir)?);
            return crate::native_process::forward_stdio(command);
        }
        RuntimeCommand::Stage {
            source,
            install_dir,
            approved_sha256,
            verification_basis,
            state_dir,
            channel,
        } => serde_json::to_value(staging::stage_with_layout(
            source,
            install_dir,
            approved_sha256,
            *verification_basis,
            state_dir.as_deref(),
            *channel,
        )?)?,
        RuntimeCommand::AbortLayout { install_dir } => {
            let _lock = install_lock(install_dir)?;
            layout::abort(install_dir)?;
            serde_json::json!({"pending_layout_aborted":true,"state_preserved":true})
        }
        RuntimeCommand::AbortActivation {
            install_dir,
            expected_candidate,
            expected_active,
        } => {
            let _lock = install_lock(install_dir)?;
            managed::require_unmanaged(install_dir)?;
            if let (Some(candidate), Some(active)) = (expected_candidate, expected_active) {
                activation::require_superseded(install_dir, candidate, active)?;
                let owner = crate::service_manager::ServiceManager::for_runtime(install_dir)?;
                if owner.owns_restart()? {
                    owner.stop()?;
                }
                upgrade_stop::stop(install_dir, active)?;
            }
            activation::abort(install_dir).await?;
            serde_json::json!({"activation_aborted":true,"payloads_preserved":true})
        }
        RuntimeCommand::Managed {
            source,
            install_dir,
            approved_sha256,
            verification_basis,
            lifecycle,
            state_dir,
            channel,
        } => {
            managed::configure(
                source,
                install_dir,
                approved_sha256,
                *verification_basis,
                *lifecycle,
                state_dir.as_deref(),
                *channel,
            )
            .await?
        }
        RuntimeCommand::Activate {
            install_dir,
            runtime_sha256,
            expected_active,
        } => serde_json::to_value(match expected_active {
            Some(expected) => activate_reviewed(install_dir, runtime_sha256, expected).await?,
            None => activate(install_dir, runtime_sha256).await?,
        })?,
        RuntimeCommand::InspectCandidate {
            install_dir,
            runtime_sha256,
        } => {
            managed::require_unmanaged(install_dir)?;
            let (main, approval) = verify_runtime(install_dir, runtime_sha256)?;
            serde_json::json!({"runtime_sha256":runtime_sha256,"executable":main,"approval":approval,"metadata_only":true,"selection_changed":false,"executed":false})
        }
        RuntimeCommand::Ensure { install_dir } => {
            let mut value = serde_json::to_value(ensure_running(install_dir).await?)?;
            // Keep the version-free universal launcher pointing at this default installation.
            // A failure is reported, never fatal: the per-installation launcher still works.
            if managed::read(install_dir)?.is_none() && value.is_object() {
                let published = install_lock(install_dir).and_then(|_lock| {
                    let (_, channel) = layout::retained_parameters(install_dir)?;
                    crate::universal_launcher::publish_for_default(install_dir, channel)
                });
                match published {
                    Ok(path) => value["universal_launcher"] = serde_json::json!(path),
                    Err(error) => {
                        value["universal_launcher_error"] = serde_json::json!(
                            error.to_string().chars().take(512).collect::<String>()
                        )
                    }
                }
            }
            value
        }
        RuntimeCommand::Start { install_dir } => {
            serde_json::to_value(lifecycle::explicit_start(install_dir).await?)?
        }
        RuntimeCommand::AbandonLifecycle {
            install_dir,
            request_id,
        } => {
            serde_json::json!({"status":"lifecycle","record":lifecycle::abandon(install_dir,*request_id)?})
        }
        RuntimeCommand::Lifecycle { install_dir } => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .lock()
                .take(16385)
                .read_to_end(&mut bytes)?;
            ensure!(bytes.len() <= 16384, "lifecycle request exceeds budget");
            let input = serde_json::from_slice(&bytes)?;
            serde_json::json!({"status":"lifecycle","record":lifecycle::run(install_dir,input).await?})
        }
        RuntimeCommand::Snapshot {
            install_dir,
            after,
            revision,
            limit,
        } => snapshot::read(install_dir, *after, *revision, *limit)?,
        RuntimeCommand::RepairPayload {
            install_dir,
            source,
            approved_sha256,
            preview,
        } => match preview {
            Some(preview) => payload_repair::apply(install_dir, source, approved_sha256, *preview)?,
            None => payload_repair::preview(install_dir, source, approved_sha256)?,
        },
        RuntimeCommand::PrepareClient { install_dir } => {
            let _lock = install_lock(install_dir)?;
            activation::require_settled(install_dir)?;
            launcher_repair::require_settled(install_dir)?;
            ensure!(
                !managed::detaching(install_dir)?,
                "finish managed detachment before preparing a client launcher"
            );
            let (_, main, _) = selected(install_dir)?;
            prepare_launcher(install_dir, &main)?;
            serde_json::json!({"launcher":verified_launcher(install_dir)?,"service_lifecycle_changed":false})
        }
        RuntimeCommand::Inspect { install_dir } => {
            let (selection, executable, approval) = selected(install_dir)?;
            let policy = managed::read(install_dir)?;
            let launcher = if policy.is_none() || launcher_path(install_dir).try_exists()? {
                Some(verified_launcher(install_dir)?)
            } else {
                None
            };
            let control = layout::control(install_dir);
            let state_error = control
                .as_ref()
                .err()
                .map(|error| error.to_string().chars().take(512).collect::<String>());
            let lifecycle = lifecycle::inspect(install_dir)?;
            let registry_namespace = control
                .as_ref()
                .ok()
                .filter(|path| path.join("service-1.4.sqlite3").is_file())
                .map(|path| {
                    airk_store::service::registry::ServiceRegistry::open_saved(path)
                        .and_then(|registry| registry.incarnation())
                })
                .transpose()?;
            let universal_launcher = match layout::retained_parameters(install_dir) {
                Ok((_, channel)) if policy.is_none() => {
                    crate::universal_launcher::current_for(install_dir, channel)
                        .ok()
                        .flatten()
                }
                _ => None,
            };
            serde_json::json!({"selection":selection,"executable":executable,"approval":approval,"launcher":launcher,"universal_launcher":universal_launcher,"managed_policy":policy,"control_dir":control.ok(),"state_error":state_error,"lifecycle":lifecycle,"registry_namespace":registry_namespace,"payload_verified":true,"payload_repair":payload_repair::status(install_dir)?})
        }
        RuntimeCommand::Remove {
            install_dir,
            runtime_sha256,
        } => {
            managed::require_unmanaged(install_dir)?;
            let _lock = install_lock(install_dir)?;
            activation::require_settled(install_dir)?;
            match fs::symlink_metadata(install_dir.join("selected.json")) {
                Ok(_) => {
                    let selection = selection_record(install_dir)?;
                    ensure!(
                        selection.active != *runtime_sha256
                            && selection.previous.as_ref() != Some(runtime_sha256),
                        "active and rollback runtimes cannot be removed"
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
            let _quiescence = quiescent(install_dir).context(
                "stop the service and wait for all owners to exit before removing an old runtime",
            )?;
            retirement::remove(install_dir, runtime_sha256)?;
            serde_json::json!({"removed_runtime":runtime_sha256})
        }
        RuntimeCommand::RetirePrevious {
            install_dir,
            expected_active,
        } => retirement::retire_previous(install_dir, expected_active).await?,
        RuntimeCommand::Uninstall { install_dir } => {
            let _lock = install_lock(install_dir)?;
            payload_repair::require_settled(install_dir)?;
            retirement::uninstall(install_dir).await?
        }
        RuntimeCommand::AbortStage {
            install_dir,
            runtime_sha256,
        } => {
            let _lock = install_lock(install_dir)?;
            payload_repair::require_settled(install_dir)?;
            staging::abort(install_dir, runtime_sha256)?;
            serde_json::json!({"aborted_stage":runtime_sha256,"promoted_runtime_preserved":install_dir.join("runtimes").join(runtime_sha256).try_exists()?})
        }
        RuntimeCommand::RepairLauncher { install_dir } => {
            managed::require_unmanaged(install_dir)?;
            let _lock = install_lock(install_dir)?;
            activation::require_settled(install_dir)?;
            crate::service_manager::ServiceManager::for_runtime(install_dir)?
                .require_stopped_for_maintenance()?;
            let _quiescence = quiescent(install_dir).context(
                "stop the service and wait for all owners before repairing the launcher",
            )?;
            let (selection, _, _) = selected(install_dir)?;
            launcher_repair::repair(install_dir, &selection.active)?;
            serde_json::json!({"launcher_repaired":true,"runtime":selection.active})
        }
        RuntimeCommand::AbortLauncherRepair { install_dir } => {
            let _lock = install_lock(install_dir)?;
            activation::require_settled(install_dir)?;
            crate::service_manager::ServiceManager::for_runtime(install_dir)?
                .require_stopped_for_maintenance()?;
            let _quiescence = quiescent(install_dir)?;
            launcher_repair::abort(install_dir)?;
            serde_json::json!({"launcher_repair_aborted":true})
        }
        RuntimeCommand::Diagnose { install_dir } => {
            ensure!(install_dir.is_dir(), "runtime installation is missing");
            local_ipc::prepare_root(install_dir)?;
            let selection = selected(install_dir).map(|(selection, main, approval)| serde_json::json!({"selection":selection,"executable":main,"approval":approval}));
            let selection_error = selection
                .as_ref()
                .err()
                .map(|error| format!("{error:#}").chars().take(1024).collect::<String>());
            let bootstrap = if install_dir.join("bootstrap.json").try_exists()? {
                read_bounded(&install_dir.join("bootstrap.json"), 4096)
                    .and_then(|bytes| Ok(serde_json::from_slice::<BootstrapObservation>(&bytes)?))
                    .map(Some)
            } else {
                Ok(None)
            };
            let bootstrap_error = bootstrap
                .as_ref()
                .err()
                .map(|error| format!("{error:#}").chars().take(1024).collect::<String>());
            let stages = staging::pending(install_dir);
            let stages_error = stages
                .as_ref()
                .err()
                .map(|error| format!("{error:#}").chars().take(1024).collect::<String>());
            let repair = launcher_repair::pending(install_dir);
            let repair_error = repair
                .as_ref()
                .err()
                .map(|error| error.to_string().chars().take(512).collect::<String>());
            let state_layout = layout::inspect(install_dir);
            let layout_error = state_layout
                .as_ref()
                .err()
                .map(|error| error.to_string().chars().take(512).collect::<String>());
            let policy = managed::read(install_dir);
            let policy_error = policy
                .as_ref()
                .err()
                .map(|error| error.to_string().chars().take(512).collect::<String>());
            let activation = activation::pending(install_dir);
            let activation_error = activation
                .as_ref()
                .err()
                .map(|error| error.to_string().chars().take(512).collect::<String>());
            let activation_result = activation::result(install_dir);
            let activation_result_error = activation_result
                .as_ref()
                .err()
                .map(|error| error.to_string().chars().take(512).collect::<String>());
            let logs = layout::control(install_dir)
                .and_then(|control| crate::service_logs::recent(&control, 32));
            let log_error = logs.is_err();
            serde_json::json!({"logs":logs.ok(),"logs_unavailable":log_error,"runtime":selection.ok(),"runtime_error":selection_error,"bootstrap":bootstrap.ok().flatten(),"bootstrap_error":bootstrap_error,"pending_stages":stages.ok(),"staging_error":stages_error,"pending_launcher_repair":repair.ok().flatten(),"launcher_repair_error":repair_error,"state_layout":state_layout.ok(),"layout_error":layout_error,"managed_policy":policy.ok().flatten(),"managed_policy_error":policy_error,"pending_activation":activation.ok().flatten(),"activation_error":activation_error,"activation_result":activation_result.ok().flatten(),"activation_result_error":activation_result_error,"live_service_health":"not_checked"})
        }
        RuntimeCommand::ResetStartup { install_dir } => {
            let _lock = install_lock(install_dir)?;
            if let Some(policy) = managed::read(install_dir)? {
                ensure!(
                    policy.lifecycle == ManagedLifecycle::UserOnDemand,
                    "organization supervisor owns startup recovery"
                );
            }
            let (_, _, approval) = selected(install_dir)?;
            replace_json(
                install_dir,
                &format!("startup-{}.json", approval.manifest_sha256),
                &StartupAttempts::default(),
            )?;
            serde_json::json!({"startup_circuit":"reset"})
        }
        RuntimeCommand::Rollback { install_dir } => {
            managed::require_unmanaged(install_dir)?;
            let (selection, _, _) = selected(install_dir)?;
            let previous = selection
                .previous
                .context("no preceding verified runtime selection")?;
            serde_json::to_value(activate(install_dir, &previous).await?)?
        }
        RuntimeCommand::Launch { install_dir, args } => {
            launcher_repair::require_settled(install_dir)?;
            let (_, executable, _) = selected(install_dir)?;
            let mut command = Command::new(executable);
            ensure!(
                !args.iter().any(|value| value == "--service-install-dir"
                    || value
                        .to_str()
                        .is_some_and(|value| value.starts_with("--service-install-dir="))),
                "launcher owns service installation context"
            );
            command.arg("--service-install-dir").arg(install_dir);
            command.args(args);
            return crate::native_process::forward_stdio(command);
        }
        RuntimeCommand::SelfTest => {
            let main = std::env::current_exe()?;
            let helper = runtime_pair::verify_helper(&main)?;
            crate::service_manager::check_executable_identity(
                &helper,
                &format!("airk-embed {}", env!("CARGO_PKG_VERSION")),
            )?;
            serde_json::json!({"protocol":1,"version":env!("CARGO_PKG_VERSION"),"self_test":"passed","state_layout":1,"foreground_dispatch":1,"managed_reference":1,"activation_resume":1,"metadata_console":1,"io_worker_threads":2})
        }
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
