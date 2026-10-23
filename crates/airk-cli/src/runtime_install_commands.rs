// SPDX-License-Identifier: Apache-2.0
//! Trusted CLI actions for the offline runtime installer and launcher.
use super::*;

pub async fn run(command: &RuntimeCommand) -> Result<()> {
    let output = match command {
        RuntimeCommand::Location => {
            serde_json::json!({"install_dir":default_install_root()?,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"channel":"stable","protocol":1})
        }
        RuntimeCommand::Login {
            install_dir,
            action,
        } => {
            let manager = crate::service_manager::ServiceManager::for_runtime(install_dir)?;
            let _lock = if matches!(action, LoginCommand::Plan | LoginCommand::Status) {
                None
            } else {
                Some(install_lock(install_dir)?)
            };
            match action {
                LoginCommand::Plan => serde_json::to_value(manager.plan()?)?,
                LoginCommand::Status => serde_json::to_value(manager.status()?)?,
                LoginCommand::Install => {
                    let (_, main, approval) = selected(install_dir)?;
                    drain(install_dir, &main, &approval).await?;
                    serde_json::to_value(manager.install()?)?
                }
                LoginCommand::Start => serde_json::to_value(manager.start()?)?,
                LoginCommand::Stop | LoginCommand::Uninstall => {
                    let (_, main, approval) = selected(install_dir)?;
                    drain(install_dir, &main, &approval).await?;
                    if matches!(action, LoginCommand::Stop) {
                        serde_json::to_value(manager.stop()?)?
                    } else {
                        serde_json::to_value(manager.uninstall()?)?
                    }
                }
            }
        }
        RuntimeCommand::Foreground { install_dir } => {
            let (_, executable, _) = selected(install_dir)?;
            let mut command = Command::new(executable);
            command
                .arg("serve")
                .arg("--runtime-dir")
                .arg(install_dir.join("control"));
            return forward(command);
        }
        RuntimeCommand::Stage {
            source,
            install_dir,
            approved_sha256,
            verification_basis,
        } => serde_json::to_value(stage(
            source,
            install_dir,
            approved_sha256,
            *verification_basis,
        )?)?,
        RuntimeCommand::Activate {
            install_dir,
            runtime_sha256,
        } => serde_json::to_value(activate(install_dir, runtime_sha256).await?)?,
        RuntimeCommand::Ensure { install_dir } => {
            serde_json::to_value(ensure_running(install_dir).await?)?
        }
        RuntimeCommand::Inspect { install_dir } => {
            let (selection, executable, approval) = selected(install_dir)?;
            serde_json::json!({"selection":selection,"executable":executable,"approval":approval,"launcher":launcher_path(install_dir)})
        }
        RuntimeCommand::Remove {
            install_dir,
            runtime_sha256,
        } => {
            let _lock = install_lock(install_dir)?;
            let (selection, _, _) = selected(install_dir)?;
            ensure!(
                selection.active != *runtime_sha256
                    && selection.previous.as_ref() != Some(runtime_sha256),
                "active and rollback runtimes cannot be removed"
            );
            let _quiescence = quiescent(install_dir).context(
                "stop the service and wait for all owners to exit before removing an old runtime",
            )?;
            retirement::remove(install_dir, runtime_sha256)?;
            serde_json::json!({"removed_runtime":runtime_sha256})
        }
        RuntimeCommand::Uninstall { install_dir } => {
            let _lock = install_lock(install_dir)?;
            retirement::uninstall(install_dir).await?
        }
        RuntimeCommand::AbortStage {
            install_dir,
            runtime_sha256,
        } => {
            let _lock = install_lock(install_dir)?;
            staging::abort(install_dir, runtime_sha256)?;
            serde_json::json!({"aborted_stage":runtime_sha256,"promoted_runtime_preserved":install_dir.join("runtimes").join(runtime_sha256).try_exists()?})
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
            serde_json::json!({"runtime":selection.ok(),"runtime_error":selection_error,"bootstrap":bootstrap.ok().flatten(),"bootstrap_error":bootstrap_error,"pending_stages":stages.ok(),"staging_error":stages_error,"live_service_health":"not_checked"})
        }
        RuntimeCommand::ResetStartup { install_dir } => {
            let _lock = install_lock(install_dir)?;
            let (_, _, approval) = selected(install_dir)?;
            replace_json(
                install_dir,
                &format!("startup-{}.json", approval.manifest_sha256),
                &StartupAttempts::default(),
            )?;
            serde_json::json!({"startup_circuit":"reset"})
        }
        RuntimeCommand::Rollback { install_dir } => {
            let (selection, _, _) = selected(install_dir)?;
            let previous = selection
                .previous
                .context("no preceding verified runtime selection")?;
            serde_json::to_value(activate(install_dir, &previous).await?)?
        }
        RuntimeCommand::Launch { install_dir, args } => {
            let (_, executable, _) = selected(install_dir)?;
            let mut command = Command::new(executable);
            command.args(args);
            return forward(command);
        }
        RuntimeCommand::SelfTest => {
            let main = std::env::current_exe()?;
            let helper = runtime_pair::verify_helper(&main)?;
            crate::service_manager::check_executable_identity(
                &helper,
                &format!("airk-embed {}", env!("CARGO_PKG_VERSION")),
            )?;
            serde_json::json!({"protocol":1,"version":env!("CARGO_PKG_VERSION"),"self_test":"passed"})
        }
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn forward(mut command: Command) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        Err(command.exec().into())
    }
    #[cfg(not(unix))]
    {
        let mut child = crate::native_process::spawn(&mut command)?;
        let _containment = crate::child_job::ChildJob::attach(&mut child)?;
        let status = child.wait()?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
