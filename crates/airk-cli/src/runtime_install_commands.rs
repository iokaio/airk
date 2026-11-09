// SPDX-License-Identifier: Apache-2.0
//! Trusted CLI actions for the offline runtime installer and launcher.
use super::*;

pub async fn run(command: &RuntimeCommand) -> Result<()> {
    let output = match command {
        RuntimeCommand::HttpIssuerRotation {
            install_dir,
            action,
            operation_id,
            expected_ca_sha256,
            expected_descriptor_sha256,
            expected_receipt_sha256,
            overlap_seconds,
        } => {
            http_rotation::run(
                install_dir,
                http_rotation::Input {
                    action: *action,
                    operation: *operation_id,
                    expected_ca: *expected_ca_sha256,
                    expected_descriptor: *expected_descriptor_sha256,
                    expected_receipt: *expected_receipt_sha256,
                    overlap_seconds: *overlap_seconds,
                },
            )
            .await?
        }
        RuntimeCommand::HttpLegacyUnionConnection {
            install_dir,
            registrations_file,
            connection_id,
            client,
            explorer_descriptor,
        } => {
            http_commands::enroll_legacy_union(
                install_dir,
                registrations_file,
                *connection_id,
                client,
                explorer_descriptor.as_deref(),
            )
            .await?
        }
        RuntimeCommand::HttpTrust {
            install_dir,
            action,
            expected_ca_sha256,
        } => http_commands::trust(install_dir, *action, *expected_ca_sha256).await?,
        RuntimeCommand::HttpEndpoint { install_dir } => {
            http_commands::endpoint(install_dir).await?
        }
        RuntimeCommand::HttpConnectionInfo {
            install_dir,
            connection_id,
        } => http_commands::existing_connection(install_dir, *connection_id).await?,
        RuntimeCommand::LookupHttpConnection {
            install_dir,
            expected_active,
            connection_id,
        } => http_commands::lookup_connection(install_dir, expected_active, *connection_id).await?,
        RuntimeCommand::HttpConnection {
            install_dir,
            selection_file,
            connection_id,
            client,
            explorer_descriptor,
        } => {
            http_commands::enroll(
                install_dir,
                selection_file,
                *connection_id,
                client,
                explorer_descriptor.as_deref(),
            )
            .await?
        }
        RuntimeCommand::HttpRevoke {
            install_dir,
            connection_id,
        } => http_commands::revoke(install_dir, *connection_id).await?,
        RuntimeCommand::HttpLegacyConnection {
            install_dir,
            registration_file,
            configuration_receipt,
            connection_id,
            client,
            explorer_descriptor,
            origin_sha256,
            reattach_connection_id,
        } => {
            http_commands::enroll_legacy(
                install_dir,
                registration_file,
                *configuration_receipt,
                *connection_id,
                client,
                explorer_descriptor.as_deref(),
                *origin_sha256,
                *reattach_connection_id,
            )
            .await?
        }
        RuntimeCommand::LegacyRetirementCandidates { install_dir, after } => {
            legacy_retirement::candidates(install_dir, *after)?
        }
        RuntimeCommand::LegacyRetirementReview {
            install_dir,
            index_id,
        } => serde_json::to_value(legacy_retirement::review(install_dir, *index_id)?)?,
        RuntimeCommand::LegacyRetirementAuthorize { install_dir } => {
            serde_json::to_value(legacy_retirement::authorize(install_dir)?)?
        }
        RuntimeCommand::LegacyRetirementReceipt { install_dir } => {
            legacy_retirement::receipt(install_dir)?
        }
        RuntimeCommand::LegacyRetirementResume { install_dir } => {
            serde_json::to_value(legacy_retirement::resume(install_dir).await?)?
        }
        RuntimeCommand::SourceScope {
            install_dir,
            expected_control,
            expected_runtime,
            source_root,
            index_collection,
        } => {
            // The lock binds selection/policy/control metadata throughout the one
            // registry transaction. No payload proof, lifecycle settlement or start.
            let scope =
                crate::service_frontend::RootScope::new(source_root, index_collection.clone())?;
            let _lock = existing_install_lock(install_dir)?;
            let approval = match managed::read(install_dir)? {
                Some(policy) => policy.metadata_approval()?,
                None => selected_preflight(install_dir)?.approval,
            };
            ensure!(
                approval.version == env!("CARGO_PKG_VERSION")
                    && approval.manifest_sha256 == *expected_runtime,
                "source scope runtime changed"
            );
            let control = layout::saved_control(install_dir)?;
            local_ipc::require_existing_root(control.parent().context("control parent missing")?)?;
            local_ipc::require_existing_root(&control)?;
            layout::validate_control(&control)?;
            ensure!(control == *expected_control, "source scope control changed");
            let observed = scope.metadata_before(
                &control,
                std::time::Instant::now() + std::time::Duration::from_secs(2),
            )?;
            serde_json::json!({"install_dir":install_dir,"control_dir":control,"runtime_sha256":approval.manifest_sha256,"scope":observed})
        }

        RuntimeCommand::ManagedServe { install_dir } => {
            return managed_service::serve(install_dir).await;
        }
        RuntimeCommand::LoginServe { channel } => {
            #[cfg(target_os = "macos")]
            {
                let root = macos_login::direct_root(*channel)?;
                let control = layout::saved_control(&root)?;
                return service_runtime::serve(&control).await;
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = channel;
                bail!("Mac direct login entry is unavailable on this platform");
            }
        }
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
            require_login_registration_change(install_dir, action)?;
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
            let (executable, _) = startup_selected(install_dir)?;
            let current = std::env::current_exe()?.canonicalize()?;
            if runtime_pair::single_executable(&executable)? {
                ensure!(
                    current == executable.canonicalize()?,
                    "legacy_foreground_retired: restart the installed AIRK service through its direct managed/login entry; a copied launcher cannot supervise AIRK 1.8.14"
                );
                return service_runtime::serve(&layout::control(install_dir)?).await;
            }
            launcher_repair::require_settled(install_dir)?;
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
            let mut command = Command::new(crate::process_roles::executable(
                &executable,
                crate::process_roles::Role::Service,
            )?);
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
            require_issuer_rotation_settled(install_dir)?;
            layout::abort(install_dir)?;
            serde_json::json!({"pending_layout_aborted":true,"state_preserved":true})
        }
        RuntimeCommand::AbortActivation {
            install_dir,
            expected_candidate,
            expected_active,
        } => {
            let _lock = install_lock(install_dir)?;
            require_issuer_rotation_settled(install_dir)?;
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
            rebuild_indexes,
        } => serde_json::to_value(if *rebuild_indexes {
            activate_rebuilding_reviewed(
                install_dir,
                runtime_sha256,
                expected_active
                    .as_deref()
                    .context("rebuild requires the reviewed preceding selection")?,
            )
            .await?
        } else {
            match expected_active {
                Some(expected) => activate_reviewed(install_dir, runtime_sha256, expected).await?,
                None => activate(install_dir, runtime_sha256).await?,
            }
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
            // Only retained schema-1 clients use a universal launcher. A modern
            // Ensure must never republish old copies or report their absence as
            // a transport failure after successful HTTPS migration.
            let (_, executable, _) = selected(install_dir)?;
            if runtime_pair::single_executable(&executable)? {
                value["universal_launcher"] = serde_json::Value::Null;
            } else if managed::read(install_dir)?.is_none() && value.is_object() {
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
        RuntimeCommand::Inventory { install_dir } => inventory::inventory(install_dir)?,
        RuntimeCommand::Purge {
            install_dir,
            select,
            confirm,
        } => inventory::purge(install_dir, select, confirm.as_deref())?,
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
            if !cfg!(target_os = "macos") {
                launcher_repair::require_settled(install_dir)?;
            }
            ensure!(
                !managed::detaching(install_dir)?,
                "finish managed detachment before preparing a client launcher"
            );
            let (_, main, approval, inventory) = selected_inventory(install_dir)?;
            prepare_client_launcher(install_dir, &main, &approval, &inventory)?;
            let launcher = if runtime_pair::single_executable(&main)? {
                None
            } else {
                Some(verified_launcher(install_dir)?)
            };
            serde_json::json!({"launcher":launcher,"executable":main,"service_lifecycle_changed":false})
        }
        RuntimeCommand::Inspect { install_dir } => {
            let (selection, executable, approval) = selected(install_dir)?;
            let policy = managed::read(install_dir)?;
            let launcher = if launcher_path(install_dir).try_exists()?
                || (policy.is_none() && !runtime_pair::single_executable(&executable)?)
            {
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
            // Optional, current receipt observations only; not an execution proof
            // across lock waits. Actual native startup retains all locked guards.
            let connection_observation = connection_observation(install_dir);
            let (rebuild, rebuild_error) = rebuild_report(install_dir);
            serde_json::json!({"selection":selection,"executable":executable,"approval":approval,"launcher":launcher,"universal_launcher":universal_launcher,"managed_policy":policy,"control_dir":control.ok(),"state_error":state_error,"lifecycle":lifecycle,"registry_namespace":registry_namespace,"payload_verified":true,"payload_repair":payload_repair::status(install_dir)?,"connection_observation":connection_observation,"rebuild":rebuild,"rebuild_error":rebuild_error})
        }
        RuntimeCommand::Remove {
            install_dir,
            runtime_sha256,
        } => {
            managed::require_unmanaged(install_dir)?;
            let _lock = install_lock(install_dir)?;
            require_issuer_rotation_settled(install_dir)?;
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
        RuntimeCommand::RetireLegacyLaunchers {
            install_dir,
            expected_active,
        } => legacy_launchers::retire(install_dir, expected_active).await?,
        RuntimeCommand::InspectLegacyLaunchers {
            install_dir,
            expected_active,
        } => legacy_launchers::inspect(install_dir, expected_active)?,
        RuntimeCommand::InspectLegacyCaches {
            install_dir,
            expected_active,
            global_storage,
            origin_locators_file,
        } => legacy_caches::inspect(
            install_dir,
            expected_active,
            global_storage,
            origin_locators_file.as_deref(),
            false,
        )?,
        RuntimeCommand::PrepareLegacyCaches {
            install_dir,
            expected_active,
            global_storage,
            origin_locators_file,
        } => legacy_caches::inspect(
            install_dir,
            expected_active,
            global_storage,
            origin_locators_file.as_deref(),
            true,
        )?,
        RuntimeCommand::RetireLegacyCaches {
            install_dir,
            expected_active,
            expected_inspection_sha256,
        } => legacy_caches::retire(install_dir, expected_active, expected_inspection_sha256)?,
        RuntimeCommand::ReprepareLegacyCaches {
            install_dir,
            expected_active,
            global_storage,
            origin_locators_file,
            expected_inspection_sha256,
        } => legacy_caches::reprepare(
            install_dir,
            expected_active,
            global_storage,
            origin_locators_file.as_deref(),
            expected_inspection_sha256,
        )?,
        RuntimeCommand::AdoptLegacyIndex {
            install_dir,
            expected_active,
            operation_id,
            locator_file,
            expected_cache_inspection_sha256,
        } => {
            legacy_index_adoption::adopt(
                install_dir,
                expected_active,
                operation_id,
                locator_file,
                expected_cache_inspection_sha256,
            )
            .await?
        }
        RuntimeCommand::AdoptLegacyIndexes {
            install_dir,
            expected_active,
            operation_id,
            locators_file,
            expected_cache_inspection_sha256,
        } => {
            legacy_index_adoption::adopt_many(
                install_dir,
                expected_active,
                operation_id,
                locators_file,
                expected_cache_inspection_sha256,
            )
            .await?
        }
        RuntimeCommand::Uninstall { install_dir } => {
            let _lock = install_lock(install_dir)?;
            legacy_caches::require_settled(install_dir)?;
            require_issuer_rotation_settled(install_dir)?;
            payload_repair::require_settled(install_dir)?;
            retirement::uninstall(install_dir).await?
        }
        RuntimeCommand::AbortStage {
            install_dir,
            runtime_sha256,
        } => {
            let _lock = install_lock(install_dir)?;
            require_issuer_rotation_settled(install_dir)?;
            payload_repair::require_settled(install_dir)?;
            staging::abort(install_dir, runtime_sha256)?;
            serde_json::json!({"aborted_stage":runtime_sha256,"promoted_runtime_preserved":install_dir.join("runtimes").join(runtime_sha256).try_exists()?})
        }
        RuntimeCommand::RepairLauncher { install_dir } => {
            managed::require_unmanaged(install_dir)?;
            let _lock = install_lock(install_dir)?;
            require_issuer_rotation_settled(install_dir)?;
            activation::require_settled(install_dir)?;
            crate::service_manager::ServiceManager::for_runtime(install_dir)?
                .require_stopped_for_maintenance()?;
            let _quiescence = quiescent(install_dir).context(
                "stop the service and wait for all owners before repairing the launcher",
            )?;
            let (selection, _, _) = selected(install_dir)?;
            let repaired = launcher_repair::repair(install_dir, &selection.active)?;
            serde_json::json!({"launcher_repaired":repaired,"runtime":selection.active})
        }
        RuntimeCommand::AbortLauncherRepair { install_dir } => {
            let _lock = install_lock(install_dir)?;
            require_issuer_rotation_settled(install_dir)?;
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
            let checkpoint = activation::checkpoint_status(install_dir);
            let checkpoint_error = checkpoint
                .as_ref()
                .err()
                .map(|error| error.to_string().chars().take(512).collect::<String>());
            let logs = layout::control(install_dir)
                .and_then(|control| crate::service_logs::recent(&control, 32));
            let log_error = logs.is_err();
            let (rebuild, rebuild_error) = rebuild_report(install_dir);
            serde_json::json!({"logs":logs.ok(),"logs_unavailable":log_error,"runtime":selection.ok(),"runtime_error":selection_error,"bootstrap":bootstrap.ok().flatten(),"bootstrap_error":bootstrap_error,"pending_stages":stages.ok(),"staging_error":stages_error,"pending_launcher_repair":repair.ok().flatten(),"launcher_repair_error":repair_error,"state_layout":state_layout.ok(),"layout_error":layout_error,"managed_policy":policy.ok().flatten(),"managed_policy_error":policy_error,"pending_activation":activation.ok().flatten(),"activation_error":activation_error,"activation_result":activation_result.ok().flatten(),"activation_result_error":activation_result_error,"upgrade_checkpoint":checkpoint.ok().flatten(),"upgrade_checkpoint_error":checkpoint_error,"live_service_health":"not_checked","rebuild":rebuild,"rebuild_error":rebuild_error})
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
            let (_, executable, _) = selected(install_dir)?;
            runtime_pair::require_legacy_stdio(&executable)?;
            crate::client_owner::watch()?;
            launcher_repair::require_settled(install_dir)?;
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
        RuntimeCommand::CatalogUpgradeMigrate {
            control_root,
            upgrade_id,
        } => {
            ensure!(
                airk_store::service::registry::catalog::upgrade::pending(control_root)?.is_some(),
                "catalog admission is not fenced"
            );
            let installer = runtime_pair::engine_identity()?;
            let namespace = airk_store::service::registry::catalog::upgrade::CatalogUpgrade::check_installer_resume(control_root, installer)?;
            layout::validate_catalog_control(control_root, namespace)?;
            let exclusion =
                airk_store::service::registry::catalog::upgrade::PendingCatalogExclusion::acquire(
                    control_root,
                )?;
            ensure!(
                exclusion.upgrade_id() == *upgrade_id,
                "catalog migration request differs from the admitted upgrade"
            );
            ensure!(
                exclusion.namespace() == namespace,
                "catalog namespace changed during native admission"
            );
            layout::validate_catalog_control(exclusion.control(), exclusion.namespace())?;
            let owner = exclusion.into_upgrade(installer)?;
            serde_json::to_value(owner.migrate()?)?
        }
        RuntimeCommand::CatalogUpgradeProbe {
            control_root,
            upgrade_id,
            nonce,
        } => {
            let observation = airk_store::service::registry::catalog::upgrade::observe(
                control_root,
                *upgrade_id,
                *nonce,
            )?;
            ensure!(
                runtime_pair::engine_identity()? == observation.installer_identity,
                "catalog candidate payload identity differs from the admitted installer"
            );
            layout::validate_catalog_control(control_root, observation.catalog.namespace)?;
            serde_json::to_value(observation)?
        }
        RuntimeCommand::CatalogAuthorityProbe {
            control_root,
            operation,
            native_host,
            nonce,
        } => {
            use airk_store::service::registry::catalog::{
                genesis::activation, upgrade::authority_v1,
            };
            let observation = if let Some(host) = native_host {
                authority_v1::observe_authority_v1(control_root, *operation, *host, *nonce)?
            } else {
                activation::observe_authority_v1(control_root, *operation, *nonce)?
            };
            ensure!(
                runtime_pair::engine_identity()? == observation.installer_identity,
                "catalog authority candidate differs from the admitted installer"
            );
            layout::validate_catalog_control(control_root, observation.catalog.namespace)?;
            serde_json::to_value(observation)?
        }
        RuntimeCommand::SelfTest => {
            let main = std::env::current_exe()?;
            if runtime_pair::single_executable(&main)? {
                #[cfg(feature = "local-inference")]
                {
                    crate::bundled_assets::verify_inventory()?;
                }
                #[cfg(not(feature = "local-inference"))]
                anyhow::bail!("single executable runtime requires embedded inference");
            } else {
                let helper = runtime_pair::verify_helper(&main)?;
                crate::service_manager::check_executable_identity(
                    &helper,
                    &format!("airk-embed {}", env!("CARGO_PKG_VERSION")),
                )?;
            }
            serde_json::json!({"protocol":1,"version":env!("CARGO_PKG_VERSION"),"self_test":"passed","state_layout":1,"foreground_dispatch":1,"managed_reference":1,"activation_resume":1,"metadata_console":1,"io_worker_threads":2,"index_catalog":airk_store::service::registry::catalog::capabilities()})
        }
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

pub(super) fn require_login_registration_change(root: &Path, action: &LoginCommand) -> Result<()> {
    if matches!(action, LoginCommand::Install | LoginCommand::Uninstall) {
        require_manager_change_settled(root)?;
    }
    Ok(())
}

fn connection_observation(root: &Path) -> Option<serde_json::Value> {
    match (
        activation::pending(root),
        launcher_repair::pending(root),
        activation::rebuild::pending(root),
    ) {
        (Ok(activation), Ok(launcher), Ok(rebuild)) => Some(serde_json::json!({"schema":1,
            "activation_pending":activation.is_some() || rebuild,"launcher_repair_pending":launcher.is_some()})),
        _ => None,
    }
}

#[cfg(test)]
mod connection_observation_tests {
    use super::*;

    fn inventory(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
        let mut files = BTreeMap::new();
        if !root.exists() {
            return Ok(files);
        }
        for entry in fs::read_dir(root)? {
            let path = entry?.path();
            if path.is_dir() {
                files.insert(path.clone(), Vec::new());
                files.extend(inventory(&path)?);
            } else {
                files.insert(path.clone(), fs::read(path)?);
            }
        }
        Ok(files)
    }

    fn query(root: &Path, source: &Path, control: PathBuf, digest: String) -> RuntimeCommand {
        RuntimeCommand::SourceScope {
            install_dir: root.into(),
            expected_control: control,
            expected_runtime: digest,
            source_root: source.into(),
            index_collection: None,
        }
    }

    #[tokio::test]
    async fn source_scope_rejected_metadata_never_creates_install_lock_or_state() -> Result<()> {
        let owner = tempfile::tempdir()?;
        let base = owner.path().canonicalize()?;
        let root = base.join("absent-install");
        let source = base.join("source");
        fs::create_dir(&source)?;
        assert!(
            run(&query(&root, &source, root.join("control"), "a".repeat(64)))
                .await
                .is_err()
        );
        assert!(!root.exists());
        local_ipc::prepare_root(&root)?;
        let before = inventory(&root)?;
        assert!(
            run(&query(&root, &source, root.join("control"), "a".repeat(64)))
                .await
                .is_err()
        );
        assert_eq!(inventory(&root)?, before);
        assert!(!root.join("install-1.4.lock").exists());
        drop(install_lock(&root)?);
        fs::write(root.join("selected.json"), b"{}")?;
        let before = inventory(&root)?;
        assert!(
            run(&query(&root, &source, root.join("control"), "a".repeat(64)))
                .await
                .is_err()
        );
        assert_eq!(inventory(&root)?, before);
        Ok(())
    }

    #[tokio::test]
    async fn source_scope_authenticates_digest_control_and_missing_state_without_creation()
    -> Result<()> {
        let (owner, root, digest) = super::super::tests::unselected_fixture()?;
        replace_json(
            &root,
            "selected.json",
            &Selection {
                schema: 1,
                active: digest.clone(),
                previous: None,
            },
        )?;
        drop(install_lock(&root)?);
        let source = owner.path().canonicalize()?.join("source");
        fs::create_dir(&source)?;
        let control = layout::saved_control(&root)?;
        local_ipc::prepare_root(&control)?;
        let before = inventory(&root)?;
        run(&query(&root, &source, control.clone(), digest.clone())).await?;
        assert_eq!(
            inventory(&root)?,
            before,
            "valid absent registry remains read-only"
        );
        for (expected_control, expected_digest) in [
            (control.clone(), "0".repeat(64)),
            (root.join("wrong-control"), digest.clone()),
        ] {
            assert!(
                run(&query(&root, &source, expected_control, expected_digest))
                    .await
                    .is_err()
            );
            assert_eq!(inventory(&root)?, before);
        }
        fs::remove_dir(&control)?;
        let before = inventory(&root)?;
        assert!(
            run(&query(&root, &source, control.clone(), digest))
                .await
                .is_err()
        );
        assert_eq!(inventory(&root)?, before);
        assert!(!control.exists());
        Ok(())
    }

    #[tokio::test]
    async fn source_scope_preserves_pending_stop_and_activation_without_election() -> Result<()> {
        use airk_core::identity::{Incarnation, Sha256Digest};
        use airk_core::service::RequestKey;
        use airk_core::service_lifecycle::{LifecycleAction, LifecycleRequest};
        use airk_store::service::registry::ServiceRegistry;
        let (owner, root, digest) = super::super::tests::unselected_fixture()?;
        replace_json(
            &root,
            "selected.json",
            &Selection {
                schema: 1,
                active: digest.clone(),
                previous: None,
            },
        )?;
        drop(install_lock(&root)?);
        let control = layout::saved_control(&root)?;
        local_ipc::prepare_root(&control)?;
        let source = owner.path().canonicalize()?.join("source");
        fs::create_dir(&source)?;
        let mut registry = ServiceRegistry::open(&control, Incarnation::from_bytes([71; 16]))?;
        let (namespace, epoch) = registry.request_epoch()?;
        let record = registry.begin_lifecycle(&LifecycleRequest {
            caller: Incarnation::from_bytes([72; 16]),
            key: RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([73; 16]),
            },
            action: LifecycleAction::Stop,
            expected_instance: Incarnation::from_bytes([71; 16]),
            runtime: Sha256Digest::from_bytes([74; 32]),
            manager: false,
        })?;
        assert!(registry.service_explicitly_stopped()?);
        drop(registry);
        replace_json(
            &root,
            "activation.json",
            &serde_json::json!({"schema":1,"install":root,
            "control":control,"candidate":digest,"prior":null,"manager":false,"phase":"probe"}),
        )?;
        let before = inventory(&root)?;
        run(&query(&root, &source, control.clone(), digest.clone())).await?;
        assert_eq!(inventory(&root)?, before);
        let registry = ServiceRegistry::open_saved(&control)?;
        assert!(registry.service_explicitly_stopped()?);
        assert_eq!(
            registry
                .pending_lifecycle()?
                .context("Stop disappeared")?
                .id,
            record.id
        );
        assert!(!root.join("bootstrap.json").exists());
        assert!(!root.join(format!("startup-{digest}.json")).exists());
        Ok(())
    }

    #[tokio::test]
    async fn source_scope_managed_metadata_authenticates_both_modes_without_starting() -> Result<()>
    {
        for lifecycle in ["external", "user_on_demand"] {
            let (owner, root, digest) = super::super::tests::unselected_fixture()?;
            drop(install_lock(&root)?);
            let control = layout::saved_control(&root)?;
            local_ipc::prepare_root(&control)?;
            let source = owner.path().canonicalize()?.join("source");
            fs::create_dir(&source)?;
            let provider = owner.path().join("provider");
            fs::rename(root.join("runtimes").join(&digest), &provider)?;
            let provider = provider.canonicalize()?;
            let approval: serde_json::Value =
                serde_json::from_slice(&fs::read(provider.join("approval.json"))?)?;
            fs::remove_file(provider.join("approval.json"))?;
            let executable = runtime_pair::digest_file(&provider.join(main_name()))?;
            let mut policy = serde_json::json!({"schema":1,"install":root,"source":provider,
                "approval":approval,"executable_sha256":executable,"lifecycle":lifecycle});
            replace_json(&root, "managed-runtime.json", &policy)?;
            managed::read(&root)?
                .context("managed policy absent")?
                .verify()?;
            let before = inventory(owner.path())?;
            run(&query(&root, &source, control.clone(), digest.clone())).await?;
            assert_eq!(inventory(owner.path())?, before);
            policy["executable_sha256"] = "0".repeat(64).into();
            replace_json(&root, "managed-runtime.json", &policy)?;
            let before = inventory(owner.path())?;
            assert!(
                run(&query(&root, &source, control, digest.clone()))
                    .await
                    .is_err()
            );
            assert_eq!(inventory(owner.path())?, before);
            assert!(!root.join("bootstrap.json").exists());
            assert!(!root.join(format!("startup-{digest}.json")).exists());
        }
        Ok(())
    }
    #[test]
    fn current_pending_or_malformed_activation_cannot_report_settled() -> Result<()> {
        let (_owner, root, identity) = super::super::tests::unselected_fixture()?;
        assert_eq!(
            connection_observation(&root),
            Some(serde_json::json!({"schema":1,
            "activation_pending":false,"launcher_repair_pending":false}))
        );
        replace_json(
            &root,
            "selected.json",
            &Selection {
                schema: 1,
                active: identity.clone(),
                previous: None,
            },
        )?;
        replace_json(
            &root,
            "activation.json",
            &serde_json::json!({"schema":1,"install":root,
            "control":layout::control(&root)?,"candidate":identity,"prior":null,"manager":false,"phase":"probe"}),
        )?;
        assert_eq!(
            connection_observation(&root).context("valid current intent was not observed")?["activation_pending"],
            true
        );
        fs::write(root.join("activation.json"), b"{")?;
        assert!(
            connection_observation(&root).is_none(),
            "malformed receipt is unknown, never settled"
        );
        Ok(())
    }
}
