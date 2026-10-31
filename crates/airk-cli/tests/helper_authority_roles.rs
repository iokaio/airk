// SPDX-License-Identifier: Apache-2.0
//! Derived cleanup owners never receive pending semantic authority.
use airk_cli::{
    project_protocol::{Command as Control, Init, Reply},
    service_ipc,
};
use airk_core::{identity::Sha256Digest, service::QueryAllowance};
use airk_store::service::registry::ServiceRegistry;
use anyhow::{Context, Result, ensure};
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn derived_worker_refuses_pending_helper_activation_before_opening_collection() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let root = base.join("runtime");
    airk_cli::local_ipc::prepare_root(&root)?;
    let source = base.join("source");
    let data = base.join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "cleanup")?;
    let service = airk_kernel::service_broker::new_instance_id()?;
    let mut registry = ServiceRegistry::open(&root, service)?;
    registry.register(&project)?;
    airk_store::service::data_scope::bind(&data, &root, registry.incarnation()?)?;
    fs::rename(&source, base.join("retired-source"))?;
    let epoch = registry.get(project.binding)?.fencing_generation;
    ensure!(
        registry.retire_missing_source(project.binding, epoch, 0)?,
        "fixture was not retired"
    );
    let (namespace, request_epoch) = registry.request_epoch()?;
    registry.enqueue_configuration(
        project.binding,
        service,
        &airk_core::service::RequestKey {
            namespace,
            epoch: request_epoch,
            nonce: airk_kernel::service_broker::new_instance_id()?,
        },
        &airk_core::service_configuration::ConfigurationRequest {
            prepare: false,
            incarnation: namespace,
            settings: project.binding,
            configuration:
                airk_core::service_configuration::ProjectConfiguration::PrepareDerivedCleanup,
        },
        Sha256Digest::from_bytes([0; 32]),
    )?;
    drop(registry);
    let before = inventory(&data)?;
    let stderr_path = base.join("stderr");
    let mut command = Command::new(env!("CARGO_BIN_EXE_airk"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = Owned(
        command
            .arg("derived-worker")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(&stderr_path)?)
            .spawn()?,
    );
    let mut input = child.0.stdin.take().context("child input missing")?;
    let mut output = child.0.stdout.take().context("child output missing")?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(4);
    let reader = std::thread::spawn(move || -> Result<()> {
        while let Some(reply) = service_ipc::read_frame::<_, Reply>(&mut output)? {
            sender
                .try_send(reply)
                .map_err(|_| anyhow::anyhow!("derived fixture reply queue overflow"))?;
        }
        Ok(())
    });
    let result = (|| -> Result<()> {
        service_ipc::write_frame(
            &mut input,
            &Init {
                engine: Sha256Digest::from_bytes([0; 32]),
                runtime: root.clone(),
                project: project.binding,
                service,
                worker: airk_kernel::service_broker::new_instance_id()?,
            },
        )?;
        let Reply::Ready { identity, .. } = receiver.recv_timeout(Duration::from_secs(10))? else {
            anyhow::bail!("derived identity missing")
        };
        service_ipc::write_frame(
            &mut input,
            &Control::Activate {
                identity,
                allowance: QueryAllowance {
                    concurrent_queries: 1,
                    response_bytes: airk_core::service::MAX_FRAME_BYTES,
                },
                pool: None,
                pool_pending: true,
            },
        )?;
        Ok(())
    })();
    drop(input);
    if result.is_err() {
        let _ = child.0.kill();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    let exited = (|| -> Result<std::process::ExitStatus> {
        loop {
            if let Some(status) = child.0.try_wait()? {
                break Ok(status);
            }
            if Instant::now() >= deadline {
                break Err(anyhow::anyhow!("derived child did not drain"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if exited.is_err() {
        let _ = child.0.kill();
        let _ = child.0.wait();
    }
    let read_result = reader
        .join()
        .map_err(|_| anyhow::anyhow!("derived reader panicked"));
    let stderr = fs::read_to_string(&stderr_path).context("read owned derived stderr")?;
    result
        .with_context(|| format!("derived handshake failed; exit={exited:?}; stderr={stderr}"))?;
    let status = exited?;
    read_result??;
    ensure!(!status.success(), "derived owner accepted helper pending");
    ensure!(
        stderr.contains("cleanup activation identity or role differs"),
        "derived owner failed at a different boundary"
    );
    ensure!(
        inventory(Path::new(&project.data_root))? == before,
        "rejected activation opened collection state"
    );
    Ok(())
}

type Inventory = Vec<(std::path::PathBuf, Option<Vec<u8>>)>;

fn inventory(root: &Path) -> Result<Inventory> {
    fn visit(root: &Path, current: &Path, files: &mut Inventory) -> Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            ensure!(!kind.is_symlink(), "unexpected derived fixture link");
            let path = entry.path();
            let relative = path.strip_prefix(root)?.to_path_buf();
            if kind.is_dir() {
                files.push((relative, None));
                visit(root, &path, files)?;
            } else {
                ensure!(kind.is_file(), "unexpected derived fixture data entry");
                files.push((relative, Some(fs::read(&path)?)));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(root, root, &mut files)?;
    files.sort();
    Ok(files)
}
