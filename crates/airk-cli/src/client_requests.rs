// SPDX-License-Identifier: Apache-2.0
//! Bounded durable CLI requests. Uncertain work is retried only with its saved key.
use airk_core::{
    identity::Incarnation,
    service::{ControlMethod, ControlResponse, JobKind, ProjectBinding, RequestKey},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const LIMIT: usize = 64;
const BYTES: u64 = 65536;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Saved {
    schema: u32,
    control: PathBuf,
    namespace: Incarnation,
    request: ControlMethod,
}
#[derive(Serialize)]
pub struct Pending {
    pub nonce: Incarnation,
    pub project: ProjectBinding,
    pub kind: JobKind,
}
fn namespace(control: &Path) -> Result<Incarnation> {
    crate::runtime_install::validate_control_root(control)?;
    airk_store::service::registry::ServiceRegistry::open_saved(control)?.incarnation()
}
fn directory(control: &Path) -> PathBuf {
    control.join("client-requests")
}
fn lock(control: &Path) -> Result<File> {
    namespace(control)?;
    let path = control.join("client-requests.lock");
    if path.try_exists()? {
        crate::runtime_install::safe_file(&path)?;
        ensure!(
            fs::metadata(&path)?.len() == 0,
            "changed CLI journal lock preserved"
        );
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)?;
    crate::runtime_install::safe_file(&path)?;
    file.try_lock()
        .context("another CLI request journal operation is active")?;
    Ok(file)
}
fn sync(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn entries(control: &Path) -> Result<Vec<PathBuf>> {
    let directory = directory(control);
    match fs::symlink_metadata(&directory) {
        Ok(metadata) => ensure!(
            metadata.is_dir()
                && !metadata.file_type().is_symlink()
                && directory.canonicalize()? == directory,
            "changed CLI journal directory preserved"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    }
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)?.take(LIMIT + 1) {
        let entry = entry?;
        let name = entry.file_name();
        let name = name
            .to_str()
            .context("unknown CLI journal file preserved")?;
        let nonce: Incarnation = name
            .strip_suffix(".json")
            .context("unfinished CLI journal file preserved; inspect before submitting more work")?
            .parse()?;
        ensure!(
            format!("{nonce}.json") == name,
            "noncanonical CLI journal file preserved"
        );
        entries.push(entry.path());
    }
    ensure!(entries.len() <= LIMIT, "CLI journal capacity exceeded");
    entries.sort();
    Ok(entries)
}
fn fields(saved: &Saved) -> Result<(Incarnation, ProjectBinding, JobKind)> {
    let (key, project, kind) = match &saved.request {
        ControlMethod::Enqueue {
            project, key, kind, ..
        } if matches!(
            kind,
            JobKind::Index
                | JobKind::Inspect
                | JobKind::Reconcile
                | JobKind::Vectors
                | JobKind::ModelSetup
                | JobKind::Verify
                | JobKind::Recover
                | JobKind::Maintain
        ) =>
        {
            (key, *project, *kind)
        }
        ControlMethod::ConfigureProject {
            project,
            key,
            request,
            ..
        } => {
            request.validate()?;
            (key, *project, JobKind::Configure)
        }
        _ => anyhow::bail!("unsupported saved CLI mutation"),
    };
    ensure!(
        saved.schema == 1 && key.namespace == saved.namespace,
        "changed CLI request contract"
    );
    Ok((key.nonce, project, kind))
}
fn read(control: &Path, nonce: Incarnation) -> Result<Saved> {
    let path = directory(control).join(format!("{nonce}.json"));
    crate::runtime_install::safe_file(&path)?;
    ensure!(
        fs::metadata(&path)?.len() <= BYTES,
        "CLI request exceeds byte bound"
    );
    let mut bytes = Vec::new();
    File::open(path)?.take(BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= BYTES,
        "CLI request grew beyond byte bound"
    );
    let saved: Saved = serde_json::from_slice(&bytes)?;
    ensure!(
        saved.control == control
            && saved.namespace == namespace(control)?
            && fields(&saved)?.0 == nonce
            && serde_json::to_vec(&saved)? == bytes,
        "saved CLI request scope or bytes changed"
    );
    Ok(saved)
}
fn persist(control: &Path, request: ControlMethod) -> Result<Incarnation> {
    let _lock = lock(control)?;
    let saved = Saved {
        schema: 1,
        control: control.into(),
        namespace: namespace(control)?,
        request,
    };
    let (nonce, project, _) = fields(&saved)?;
    let registry = airk_store::service::registry::ServiceRegistry::open_saved(control)?;
    if let ControlMethod::ConfigureProject { request, .. } = &saved.request
        && request.configuration.is_derived_cleanup()
    {
        registry.retained_project(project)?;
        ensure!(
            !registry.is_registered(project)?
                && request.incarnation == saved.namespace
                && request.settings == project,
            "cleanup journal request requires its exact inactive project authority"
        );
    } else {
        registry.get(project)?;
    }
    let entries = entries(control)?;
    ensure!(
        entries.len() < LIMIT,
        "CLI request journal is full; inspect saved outcomes before forgetting requests"
    );
    let directory = directory(control);
    if !directory.try_exists()? {
        fs::create_dir(&directory)?;
        sync(control)?;
    }
    let bytes = serde_json::to_vec(&saved)?;
    ensure!(
        bytes.len() as u64 <= BYTES,
        "CLI request exceeds byte bound"
    );
    let mut temporary = tempfile::NamedTempFile::new_in(&directory)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist_noclobber(directory.join(format!("{nonce}.json")))?;
    sync(&directory)?;
    Ok(nonce)
}
pub async fn prepare(
    control: &Path,
    project: ProjectBinding,
    kind: JobKind,
) -> Result<Incarnation> {
    let control = control.canonicalize()?;
    let ControlResponse::RequestEpoch { namespace, epoch } =
        crate::console_client::request(&control, ControlMethod::RequestEpoch).await?
    else {
        anyhow::bail!("request epoch unavailable")
    };
    persist(
        &control,
        ControlMethod::Enqueue {
            project,
            kind,
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
        },
    )
}
pub async fn prepare_configuration(
    control: &Path,
    project: ProjectBinding,
    draft: crate::service_configuration::Draft,
) -> Result<Incarnation> {
    let control = control.canonicalize()?;
    let request = crate::service_configuration::prepare(&control, project, draft).await?;
    let ControlResponse::RequestEpoch { namespace, epoch } =
        crate::console_client::request(&control, ControlMethod::RequestEpoch).await?
    else {
        anyhow::bail!("request epoch unavailable")
    };
    persist(
        &control,
        ControlMethod::ConfigureProject {
            project,
            request: Box::new(request),
            caller: airk_kernel::service_broker::new_instance_id()?,
            key: RequestKey {
                namespace,
                epoch,
                nonce: airk_kernel::service_broker::new_instance_id()?,
            },
        },
    )
}
pub fn list(control: &Path) -> Result<Vec<Pending>> {
    let control = control.canonicalize()?;
    let control = control.as_path();
    namespace(control)?;
    entries(control)?
        .iter()
        .map(|path| {
            let nonce = path
                .file_stem()
                .and_then(|name| name.to_str())
                .context("invalid CLI request file")?
                .parse()?;
            let (_, project, kind) = fields(&read(control, nonce)?)?;
            Ok(Pending {
                nonce,
                project,
                kind,
            })
        })
        .collect()
}
pub async fn submit(control: &Path, nonce: Incarnation) -> Result<ControlResponse> {
    let control = control.canonicalize()?;
    let control = control.as_path();
    let saved = {
        let _lock = lock(control)?;
        entries(control)?;
        read(control, nonce)?
    };
    let (_, project, kind) = fields(&saved)?;
    let response = crate::console_client::request(control, saved.request).await?;
    match &response {
        ControlResponse::Job { job } => ensure!(
            job.project == project && job.kind == kind,
            "crossed native CLI job receipt"
        ),
        ControlResponse::Failure { code } => {
            anyhow::bail!("native request refused: {code}; saved key is preserved")
        }
        _ => anyhow::bail!("native request outcome unknown; saved key is preserved"),
    }
    Ok(response)
}
/// Explicit forgetting never cancels native work or asserts an uncertain outcome.
pub fn forget(control: &Path, nonce: Incarnation) -> Result<()> {
    let control = control.canonicalize()?;
    let control = control.as_path();
    let _lock = lock(control)?;
    entries(control)?;
    let path = directory(control).join(format!("{nonce}.json"));
    if !path.try_exists()? {
        return Ok(());
    }
    read(control, nonce)?;
    fs::remove_file(path)?;
    sync(&directory(control))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_requests_survive_reopen_and_capacity_refuses_before_more_work() -> Result<()> {
        let fixture = tempfile::tempdir()?;
        let control = fixture.path().canonicalize()?;
        let mut registry = airk_store::service::registry::ServiceRegistry::open(
            &control,
            airk_kernel::service_broker::new_instance_id()?,
        )?;
        // The journal fixture uses sibling private/source directories, never a source containing control.
        let outside = tempfile::tempdir()?;
        let source = outside.path().join("source");
        let data = outside.path().join("data");
        fs::create_dir(&source)?;
        fs::create_dir(&data)?;
        let project = airk_kernel::service_projects::registration(&source, &data, "fixture")?;
        registry.register(&project)?;
        let namespace = registry.incarnation()?;
        let request = |number: u128| ControlMethod::Enqueue {
            project: project.binding,
            caller: Incarnation::from_bytes([1; 16]),
            kind: JobKind::Index,
            key: RequestKey {
                namespace,
                epoch: 0,
                nonce: Incarnation::from_bytes(number.to_be_bytes()),
            },
        };
        let nonce = persist(&control, request(1))?;
        let bytes = fs::read(directory(&control).join(format!("{nonce}.json")))?;
        assert_eq!(serde_json::to_vec(&read(&control, nonce)?)?, bytes);
        assert_eq!(list(&control)?.len(), 1);
        let record_path = directory(&control).join(format!("{nonce}.json"));
        let mut changed: Saved = serde_json::from_slice(&bytes)?;
        changed.control = outside.path().canonicalize()?;
        let changed_bytes = serde_json::to_vec(&changed)?;
        fs::write(&record_path, &changed_bytes)?;
        assert!(read(&control, nonce).is_err());
        assert!(forget(&control, nonce).is_err());
        assert_eq!(
            fs::read(&record_path)?,
            changed_bytes,
            "a changed request must be preserved without deletion or submission"
        );
        fs::write(&record_path, &bytes)?;
        for number in 2..=LIMIT as u128 {
            persist(&control, request(number))?;
        }
        assert!(persist(&control, request(65)).is_err());
        assert_eq!(list(&control)?.len(), LIMIT);
        forget(&control, nonce)?;
        forget(&control, nonce)?;
        assert_eq!(list(&control)?.len(), LIMIT - 1);
        assert!(fs::read_dir(source)?.next().is_none());
        Ok(())
    }
}
