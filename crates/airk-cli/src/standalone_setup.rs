// SPDX-License-Identifier: Apache-2.0
//! Guided standalone setup delegates scope, grants, indexing and enrollment to
//! native owners. Prompts never manufacture a grant or expose a bearer value.
use airk_core::{
    catalog_host::GrantDraftSelection,
    catalog_management::{
        CatalogEntity, CatalogItem, CatalogKind, CatalogOperation, CatalogPageRequest,
        CatalogRequest,
    },
    catalog_setup::{IndexSetupSelection, SetupStage},
    identity::{Incarnation, Sha256Digest, framed_digest},
    index_catalog::{
        CatalogRevision, IndexId, MAX_DEFINITIONS, MAX_SELECTION, Membership, PathScope,
        QueryProjectId, ScopeAtom,
    },
    service::{CombinedConnectRequest, ControlMethod, ControlResponse, RequestKey},
    service_http::{HttpConnectionInfo, HttpSelection},
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

#[path = "standalone_setup_launch.rs"]
mod launch;
pub use launch::{launch_client, launch_instructions};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    Claude,
    Codex,
}
impl Client {
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

pub struct Options {
    pub client: Client,
    pub install_dir: Option<PathBuf>,
    pub control_dir: Option<PathBuf>,
    pub context_directory: PathBuf,
    pub project_ids: Vec<QueryProjectId>,
    pub index_ids: Vec<IndexId>,
    pub create_root: Option<PathBuf>,
    pub data_root: Option<PathBuf>,
    pub name: Option<String>,
    pub watch: bool,
    pub yes: bool,
    pub connection_id: Option<Incarnation>,
}

pub struct Outcome {
    pub connection: HttpConnectionInfo,
    pub native_receipts: Vec<Value>,
    pub journal_files: Vec<PathBuf>,
    pub suggested_directory: Option<PathBuf>,
}

pub trait SetupPrompt {
    fn interactive(&self) -> bool;
    fn message(&mut self, text: &str) -> Result<()>;
    fn answer(&mut self, text: &str) -> Result<String>;
}

pub struct Terminal;
impl SetupPrompt for Terminal {
    fn interactive(&self) -> bool {
        io::stdin().is_terminal()
    }
    fn message(&mut self, text: &str) -> Result<()> {
        writeln!(io::stderr().lock(), "{text}")?;
        Ok(())
    }
    fn answer(&mut self, text: &str) -> Result<String> {
        ensure!(
            self.interactive(),
            "interactive setup needs a terminal; supply an explicit scope and --yes"
        );
        let mut output = io::stderr().lock();
        write!(output, "{text} ")?;
        output.flush()?;
        drop(output);
        let mut line = String::new();
        ensure!(
            io::stdin().read_line(&mut line)? > 0,
            "setup cancelled at end of input"
        );
        ensure!(line.len() <= 16 * 1024, "setup answer exceeds bound");
        Ok(line.trim().to_owned())
    }
}

fn confirm(prompt: &mut impl SetupPrompt, yes: bool, text: &str) -> Result<()> {
    prompt.message(text)?;
    ensure!(
        yes || prompt.answer("Continue? [y/N]")?.eq_ignore_ascii_case("y"),
        "setup cancelled; no requested change was submitted"
    );
    Ok(())
}

pub fn installation(explicit: Option<&Path>) -> Result<PathBuf> {
    explicit
        .map(Path::to_path_buf)
        .map_or_else(crate::runtime_install::default_install_root, Ok)
}

/// The expected manifest digest comes from an explicitly trusted distributor or
/// user, never from hashing the same untrusted payload and trusting its result.
pub async fn install_payload(
    install: &Path,
    source: &Path,
    approved: &str,
    yes: bool,
    rebuild: bool,
    prompt: &mut impl SetupPrompt,
) -> Result<()> {
    approved.parse::<Sha256Digest>()?;
    ensure!(
        crate::runtime_pair::digest_file(&source.join(crate::runtime_install::main_name()))?
            == crate::runtime_pair::current_executable_sha256()?,
        "run setup using the reviewed payload's own AIRK executable before installing it"
    );
    let prior = if install.join("selected.json").try_exists()? {
        Some(crate::runtime_install::selected(install)?.0)
    } else {
        None
    };
    if prior
        .as_ref()
        .is_some_and(|selection| selection.active == approved)
    {
        ensure!(
            !rebuild,
            "early rebuilding requires a distinct reviewed upgrade candidate"
        );
        crate::runtime_install::stage(
            source,
            install,
            approved,
            crate::runtime_install::VerificationBasis::TrustedDistribution,
        )?;
        return Ok(());
    }
    confirm(
        prompt,
        yes,
        &format!(
            "Install the reviewed AIRK payload {approved} at {}? Source files and existing indexes stay under native ownership.",
            install.display()
        ),
    )?;
    let rebuild = if prior.is_some() && !yes && !rebuild {
        match prompt.answer("Upgrade: [1] migrate existing indexes, [2] rebuild from saved sources (previous state preserved). Choice [1]:")?.as_str() {
            ""|"1"=>false,"2"=>true,_=>bail!("select migration or rebuilding; no runtime was staged"),
        }
    } else {
        rebuild
    };
    ensure!(
        !rebuild || prior.is_some(),
        "rebuilding requires an installed predecessor"
    );
    crate::runtime_install::stage(
        source,
        install,
        approved,
        crate::runtime_install::VerificationBasis::TrustedDistribution,
    )?;
    match prior {
        Some(prior) if rebuild => {
            crate::runtime_install::activate_rebuilding_reviewed(install, approved, &prior.active)
                .await?;
        }
        Some(prior) => {
            crate::runtime_install::activate_reviewed(install, approved, &prior.active).await?;
        }
        None => {
            crate::runtime_install::activate(install, approved).await?;
        }
    }
    Ok(())
}

pub async fn uninstall(
    install: Option<&Path>,
    yes: bool,
    prompt: &mut impl SetupPrompt,
) -> Result<()> {
    let install = installation(install)?;
    confirm(
        prompt,
        yes,
        &format!(
            "Uninstall AIRK from {}? Native uninstall stops owned processes and removes owned runtime files; indexed source files and index data are preserved.",
            install.display()
        ),
    )?;
    let receipt = crate::runtime_install::uninstall_receipt(&install).await?;
    serde_json::to_writer_pretty(io::stdout().lock(), &receipt)?;
    writeln!(io::stdout().lock())?;
    require_completed_uninstall(&receipt)
}

fn require_completed_uninstall(receipt: &Value) -> Result<()> {
    ensure!(
        receipt.get("uninstalled").and_then(Value::as_bool) == Some(true),
        "native uninstall is incomplete; preserve the installer and its uninstall entry, then resume native uninstall using the complete receipt above"
    );
    Ok(())
}

trait Transport {
    async fn call(&mut self, method: ControlMethod) -> Result<ControlResponse>;
}
struct Native {
    control: PathBuf,
}
impl Transport for Native {
    async fn call(&mut self, method: ControlMethod) -> Result<ControlResponse> {
        crate::console_client::request(&self.control, method).await
    }
}

struct Session<T> {
    transport: T,
    control: PathBuf,
    operation: Incarnation,
    step: u64,
    receipts: Vec<Value>,
    journals: Vec<PathBuf>,
    authorized_directories: Vec<Option<PathBuf>>,
}

#[derive(Serialize)]
struct Submission<'a> {
    schema: u32,
    operation: Incarnation,
    control: &'a Path,
    method: &'a ControlMethod,
    response: Option<&'a ControlResponse>,
    transport_failure: Option<String>,
}

impl<T: Transport> Session<T> {
    async fn call(&mut self, method: ControlMethod) -> Result<ControlResponse> {
        self.transport.call(method).await
    }
    async fn epoch(&mut self) -> Result<(Incarnation, u64)> {
        match self.call(ControlMethod::RequestEpoch).await? {
            ControlResponse::RequestEpoch { namespace, epoch } => Ok((namespace, epoch)),
            other => refused(other),
        }
    }
    async fn key(&mut self) -> Result<RequestKey> {
        let (namespace, epoch) = self.epoch().await?;
        let identity = framed_digest(
            b"airk/standalone-setup-step/1",
            &[self.operation.as_bytes(), &self.step.to_le_bytes()],
        );
        self.step = self.step.checked_add(1).context("setup step exhausted")?;
        let mut nonce = [0; 16];
        nonce.copy_from_slice(&identity.as_bytes()[..16]);
        Ok(RequestKey {
            namespace,
            epoch,
            nonce: Incarnation::from_bytes(nonce),
        })
    }
    async fn submit(&mut self, method: ControlMethod) -> Result<ControlResponse> {
        ensure!(
            matches!(
                method,
                ControlMethod::CatalogApply { .. } | ControlMethod::CatalogIndexSetupApply { .. }
            ),
            "unsupported standalone setup mutation"
        );
        let name = format!("standalone-setup-{}-{}.json", self.operation, self.step);
        let path = self.control.join(&name);
        let before = Submission {
            schema: 1,
            operation: self.operation,
            control: &self.control,
            method: &method,
            response: None,
            transport_failure: None,
        };
        write_private_record(&self.control, &name, &serde_json::to_vec(&before)?, false)?;
        self.journals.push(path.clone());
        let response = self.transport.call(method.clone()).await;
        let after = Submission {
            schema: 1,
            operation: self.operation,
            control: &self.control,
            method: &method,
            response: response.as_ref().ok(),
            transport_failure: response.as_ref().err().map(|error| format!("{error:#}")),
        };
        if let Err(record_error) =
            write_private_record(&self.control, &name, &serde_json::to_vec(&after)?, true)
        {
            return match response {
                Err(original) => Err(original.context(format!("recording the setup outcome also failed: {record_error:#}; original request remains at {}",path.display()))),
                Ok(observed) => Err(record_error.context(format!("native response was {}; original request remains at {}",serde_json::to_string(&observed)?,path.display()))),
            };
        }
        let response = response.with_context(|| format!("native setup outcome may be unknown; exact submitted request is preserved at {}; no mutation was retried", path.display()))?;
        self.receipts.push(serde_json::to_value(&response)?);
        Ok(response)
    }
    async fn descriptor(
        &mut self,
        context: Sha256Digest,
        revision: Option<CatalogRevision>,
    ) -> Result<airk_core::catalog_host::NativeHostContextDescriptor> {
        match self
            .call(ControlMethod::CatalogHostContext {
                key: context,
                expected_catalog_revision: revision,
            })
            .await?
        {
            ControlResponse::CatalogHostContext { descriptor, .. } => {
                descriptor.validate()?;
                ensure!(
                    descriptor.key == context
                        && revision.is_none_or(|value| value == descriptor.catalog_revision),
                    "native context changed"
                );
                Ok(descriptor)
            }
            other => refused(other),
        }
    }
    async fn pages(
        &mut self,
        kind: CatalogKind,
        revision: Option<CatalogRevision>,
        project: Option<QueryProjectId>,
        index: Option<IndexId>,
    ) -> Result<(Incarnation, CatalogRevision, Vec<CatalogItem>)> {
        let mut request = CatalogPageRequest {
            kind,
            index_id: index,
            project_id: project,
            after: None,
            expected_catalog_revision: revision,
            limit: MAX_SELECTION,
            max_bytes: airk_core::catalog_management::MAX_PAGE_BYTES,
        };
        let mut items = Vec::new();
        let mut namespace = None;
        let mut revision = revision;
        loop {
            request.expected_catalog_revision = revision;
            let ControlResponse::CatalogPage { page, .. } = self
                .call(ControlMethod::CatalogPage {
                    request: request.clone(),
                })
                .await?
            else {
                bail!("unexpected catalog page response");
            };
            ensure!(
                namespace.is_none_or(|value| value == page.namespace)
                    && revision.is_none_or(|value| value == page.revision),
                "catalog listing changed during setup"
            );
            namespace = Some(page.namespace);
            revision = Some(page.revision);
            ensure!(
                items.len() + page.items.len() <= MAX_DEFINITIONS,
                "complete setup catalog exceeds bound; select explicit IDs instead"
            );
            items.extend(page.items);
            let Some(next) = page.continuation else {
                break;
            };
            ensure!(
                request
                    .after
                    .as_ref()
                    .is_none_or(|previous| previous.after < next.after),
                "catalog continuation did not advance"
            );
            request.after = Some(next);
        }
        Ok((
            namespace.context("catalog namespace missing")?,
            revision.context("catalog revision missing")?,
            items,
        ))
    }
    async fn detail(
        &mut self,
        entity: CatalogEntity,
        namespace: Incarnation,
        revision: CatalogRevision,
    ) -> Result<CatalogItem> {
        match self
            .call(ControlMethod::CatalogDetail {
                entity,
                expected_catalog_revision: Some(revision),
            })
            .await?
        {
            ControlResponse::CatalogDetail { detail, .. } => {
                ensure!(
                    detail.namespace == namespace && detail.revision == revision,
                    "catalog detail changed"
                );
                detail.item.context("selected catalog item is unavailable")
            }
            other => refused(other),
        }
    }
    async fn apply(
        &mut self,
        context: Sha256Digest,
        operation: CatalogOperation,
    ) -> Result<airk_core::catalog_management::CatalogReceipt> {
        let descriptor = self.descriptor(context, None).await?;
        let key = self.key().await?;
        ensure!(
            key.namespace == descriptor.namespace,
            "catalog namespace changed"
        );
        let request = CatalogRequest {
            caller: crate::local_ipc::caller_identity()?,
            key,
            expected_catalog_revision: descriptor.catalog_revision,
            operation,
        };
        request.validate()?;
        let (preview, digest) = match self
            .call(ControlMethod::CatalogPreview {
                request: request.clone(),
            })
            .await?
        {
            ControlResponse::CatalogPreview {
                preview,
                preview_digest,
            } => (preview, preview_digest),
            other => return refused(other),
        };
        ensure!(
            preview.request_digest == request.identity()?
                && preview.operation == request.operation
                && preview.identity()? == digest,
            "catalog preview crossed setup request"
        );
        match self
            .submit(ControlMethod::CatalogApply {
                request: request.clone(),
                preview_digest: digest,
            })
            .await?
        {
            ControlResponse::CatalogApplied { receipt, .. } => {
                ensure!(
                    receipt.key == request.key
                        && receipt.caller == request.caller
                        && receipt.request_digest == request.identity()?,
                    "catalog receipt crossed setup request"
                );
                Ok(receipt)
            }
            other => refused(other),
        }
    }
}

fn refused<T>(response: ControlResponse) -> Result<T> {
    bail!(
        "native setup refused: {}",
        serde_json::to_string(&response)?
    )
}

fn write_private_record(root: &Path, name: &str, bytes: &[u8], replace: bool) -> Result<()> {
    const MAX_RECORD: usize = airk_core::service::MAX_FRAME_BYTES * 4;
    write_private_file(root, name, bytes, MAX_RECORD, replace)
}

/// Enroll a previously reviewed selection while retaining its exact recovery identity.
pub async fn enroll_selection(
    install: Option<&Path>,
    control: Option<&Path>,
    selection: HttpSelection,
    id: Incarnation,
    client: &str,
    explorer_descriptor: Option<&Path>,
) -> Result<(Value, PathBuf)> {
    let control = match control {
        Some(control) => control.canonicalize()?,
        None => crate::runtime_install::installed_control(&installation(install)?).await?,
    };
    enroll_recorded(
        &control,
        airk_kernel::service_broker::new_instance_id()?,
        selection,
        id,
        client,
        explorer_descriptor,
        &mut NativeEnrollment,
    )
    .await
}

trait EnrollmentTransport {
    async fn enroll(
        &mut self,
        control: &Path,
        selection: HttpSelection,
        id: Incarnation,
        client: &str,
        explorer_descriptor: Option<&Path>,
    ) -> Result<Value>;
}
struct NativeEnrollment;
impl EnrollmentTransport for NativeEnrollment {
    async fn enroll(
        &mut self,
        control: &Path,
        selection: HttpSelection,
        id: Incarnation,
        client: &str,
        explorer_descriptor: Option<&Path>,
    ) -> Result<Value> {
        crate::runtime_install::setup_http_connection(
            None,
            Some(control),
            selection,
            id,
            client,
            explorer_descriptor,
        )
        .await
    }
}

#[derive(Serialize)]
struct EnrollmentRecord<'a> {
    schema: u32,
    operation: Incarnation,
    connection_id: Incarnation,
    control: &'a Path,
    client: &'a str,
    explorer_descriptor: Option<&'a Path>,
    selection: &'a HttpSelection,
    response: Option<&'a Value>,
    uncertain_error: Option<String>,
}

async fn enroll_recorded(
    control: &Path,
    operation: Incarnation,
    selection: HttpSelection,
    id: Incarnation,
    client: &str,
    explorer_descriptor: Option<&Path>,
    transport: &mut impl EnrollmentTransport,
) -> Result<(Value, PathBuf)> {
    selection.validate()?;
    // A new explicit invocation may reconcile the same native connection ID.
    // Keep every attempt's original intent/result; native authority decides
    // whether the same-ID request is idempotent or conflicts.
    let name = format!("standalone-enrollment-{id}-{operation}.json");
    let path = control.join(&name);
    let intent = EnrollmentRecord {
        schema: 1,
        operation,
        connection_id: id,
        control,
        client,
        explorer_descriptor,
        selection: &selection,
        response: None,
        uncertain_error: None,
    };
    write_private_record(control, &name, &serde_json::to_vec(&intent)?, false)?;
    let response = transport
        .enroll(control, selection.clone(), id, client, explorer_descriptor)
        .await;
    let record = EnrollmentRecord {
        response: response.as_ref().ok(),
        uncertain_error: response.as_ref().err().map(|error| format!("{error:#}")),
        ..intent
    };
    let recovery = format!(
        "connection {id}; protected enrollment record {}; inspect this ID with setup --existing-connection-id {id} --service-install-dir <verified-installation> before creating any new connection; no enrollment was retried",
        path.display()
    );
    if let Err(record_error) =
        write_private_record(control, &name, &serde_json::to_vec(&record)?, true)
    {
        return match response {
            Err(original) => Err(original.context(format!(
                "recording enrollment also failed: {record_error:#}; {recovery}"
            ))),
            Ok(observed) => Err(record_error.context(format!(
                "native enrollment returned {}; {recovery}",
                serde_json::to_string(&observed)?
            ))),
        };
    }
    Ok((
        response
            .with_context(|| format!("native enrollment outcome may be uncertain; {recovery}"))?,
        path,
    ))
}

fn write_private_file(
    root: &Path,
    name: &str,
    bytes: &[u8],
    maximum: usize,
    replace: bool,
) -> Result<()> {
    ensure!(
        bytes.len() <= maximum && !name.contains(['/', '\\']),
        "setup file exceeds bound"
    );
    crate::local_ipc::require_existing_root(root)?;
    let target = root.join(name);
    match std::fs::symlink_metadata(&target) {
        Ok(_) => {
            crate::http_endpoint::read_private(root, name, maximum)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(windows)]
    crate::local_ipc::require_owner_private(file.path())?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    if replace {
        file.persist(&target).map_err(|error| error.error)?;
    } else {
        file.persist_noclobber(&target)
            .map_err(|error| error.error)?;
    }
    crate::runtime_install::safe_file(&target)?;
    #[cfg(unix)]
    std::fs::File::open(root)?.sync_all()?;
    Ok(())
}

pub async fn enroll(options: Options, prompt: &mut impl SetupPrompt) -> Result<Outcome> {
    ensure!(
        options.create_root.is_none()
            || options.project_ids.is_empty() && options.index_ids.is_empty(),
        "choose an existing scope or create one index, not both"
    );
    ensure!(
        !options.yes
            || !options.project_ids.is_empty()
            || !options.index_ids.is_empty()
            || options.create_root.is_some(),
        "noninteractive setup requires --project, --index or --create-root; no scope is selected automatically"
    );
    let install = installation(options.install_dir.as_deref())?;
    let control = match options.control_dir.as_ref() {
        Some(control) => control.canonicalize()?,
        None => crate::runtime_install::installed_control(&install).await?,
    };
    let operation = airk_kernel::service_broker::new_instance_id()?;
    let context_dir = options.context_directory.canonicalize()?;
    let context = framed_digest(
        b"airk/standalone-client-context/1",
        &[
            options.client.label().as_bytes(),
            context_dir
                .to_str()
                .context("setup folder is not UTF-8")?
                .as_bytes(),
        ],
    );
    let mut session = Session {
        transport: Native {
            control: control.clone(),
        },
        control: control.clone(),
        operation,
        step: 0,
        receipts: Vec::new(),
        journals: Vec::new(),
        authorized_directories: Vec::new(),
    };
    let selection = prepare_scope(&mut session, &options, context, &install, prompt).await?;
    let id = options
        .connection_id
        .map(Ok)
        .unwrap_or_else(airk_kernel::service_broker::new_instance_id)?;
    let (value, enrollment_record) = enroll_recorded(
        &control,
        operation,
        selection,
        id,
        options.client.label(),
        None,
        &mut NativeEnrollment,
    )
    .await?;
    session.journals.push(enrollment_record);
    session.receipts.push(value.clone());
    let connection: HttpConnectionInfo = serde_json::from_value(value)?;
    connection.validate()?;
    prompt.message("AIRK created a scoped connection. The bearer remains in its protected native credential file; the configuration uses an environment reference.")?;
    Ok(Outcome {
        connection,
        native_receipts: session.receipts,
        journal_files: session.journals,
        suggested_directory: sole_authorized_directory(&session.authorized_directories),
    })
}

async fn prepare_scope<T: Transport>(
    session: &mut Session<T>,
    options: &Options,
    context: Sha256Digest,
    install: &Path,
    prompt: &mut impl SetupPrompt,
) -> Result<HttpSelection> {
    let mut projects = options.project_ids.clone();
    let mut indexes = options.index_ids.clone();
    let mut create = options.create_root.clone();
    if projects.is_empty() && indexes.is_empty() && create.is_none() {
        let (_, revision, project_items) = session
            .pages(CatalogKind::Projects, None, None, None)
            .await?;
        let (_, _, index_items) = session
            .pages(CatalogKind::Indexes, Some(revision), None, None)
            .await?;
        let choices = scope_choices(&project_items, &index_items)?;
        if choices.is_empty() {
            let answer = prompt.answer(
                "No indexes exist. Enter the source folder for a new index (blank cancels):",
            )?;
            ensure!(!answer.is_empty(), "setup cancelled");
            create = Some(answer.into());
        } else {
            prompt.message("Choose an index set (pN) or individual indexes (iN). No source is indexed merely by listing it:")?;
            for (key, description) in &choices {
                prompt.message(&format!("  {key}: {description}"))?;
            }
            let answer = prompt.answer("Selection (comma-separated; blank cancels):")?;
            let (selected_projects, selected_indexes) =
                parse_choice(&answer, &project_items, &index_items)?;
            projects = selected_projects;
            indexes = selected_indexes;
        }
    }
    if let Some(source) = create {
        let source = source
            .canonicalize()
            .context("resolve the explicitly selected source folder")?;
        ensure!(source.is_dir(), "source folder is not a directory");
        let data = options
            .data_root
            .clone()
            .map(Ok)
            .unwrap_or_else(|| crate::runtime_install::default_project_data(install))?;
        let data = crate::user_paths::canonical_location(&data)?;
        let name = options
            .name
            .clone()
            .or_else(|| {
                source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "AIRK index".into());
        let descriptor = session.descriptor(context, None).await?;
        let key = session.key().await?;
        ensure!(
            key.namespace == descriptor.namespace,
            "catalog namespace changed"
        );
        let selection = IndexSetupSelection {
            key,
            expected_catalog_revision: descriptor.catalog_revision,
            name,
            primary_root: source.to_str().context("source is not UTF-8")?.into(),
            data_root: data.to_str().context("data is not UTF-8")?.into(),
            collection: format!("setup-{}", session.operation),
            additional_roots: Vec::new(),
            setup_watch: options.watch,
        };
        selection.validate()?;
        let draft = match session
            .call(ControlMethod::CatalogIndexSetupDraft {
                selection: selection.clone(),
            })
            .await?
        {
            ControlResponse::CatalogIndexSetupDraft {
                draft,
                request_digest,
            } => {
                ensure!(
                    draft.request.identity()? == request_digest,
                    "native setup draft digest changed"
                );
                draft
            }
            other => return refused(other),
        };
        draft.validate()?;
        ensure!(
            draft.request.key == selection.key
                && draft.request.expected_catalog_revision == selection.expected_catalog_revision
                && draft.request.caller == crate::local_ipc::caller_identity()?
                && draft.request.name == selection.name
                && draft.request.registration.root == selection.primary_root
                && draft.request.registration.data_root == selection.data_root
                && draft.request.registration.collection == selection.collection
                && draft.request.setup_watch == selection.setup_watch
                && draft.request.sources.len() == 1
                && draft.request.sources[0].owner
                && draft.request.sources[0].aliases.is_empty()
                && draft.request.sources[0].canonical_path == selection.primary_root,
            "native index setup changed the selected source or storage"
        );
        confirm(
            prompt,
            options.yes,
            &format!(
                "Create index {:?} from {} using storage {} ({:?})? Watching: {}. Native indexing is automatic; source files remain read-only.",
                selection.name,
                source.display(),
                data.display(),
                draft.storage_action,
                options.watch
            ),
        )?;
        let expected = draft.request.clone();
        let receipt = match session
            .submit(ControlMethod::CatalogIndexSetupApply { draft })
            .await?
        {
            ControlResponse::CatalogIndexSetupApplied { receipt } => receipt,
            other => return refused(other),
        };
        receipt.validate()?;
        ensure!(
            receipt.request == expected && receipt.stage == SetupStage::Ready,
            "index setup has not completed native preparation"
        );
        indexes.push(receipt.index_id);
    }
    unique_selection(&projects, &indexes)?;
    if projects.is_empty() {
        ensure!(!indexes.is_empty(), "no index scope selected");
        let descriptor = session.descriptor(context, None).await?;
        let mut sources = Vec::new();
        for index in &indexes {
            let (_, _, items) = session
                .pages(
                    CatalogKind::Sources,
                    Some(descriptor.catalog_revision),
                    None,
                    Some(*index),
                )
                .await?;
            let mut atoms = Vec::new();
            for item in items {
                let CatalogItem::Source(source) = item else {
                    bail!("unexpected source catalog item");
                };
                ensure!(
                    source.index_id == *index && source.enabled,
                    "source is unavailable"
                );
                prompt.message(&format!("Selected source: {:?}", source.canonical_path))?;
                atoms.push(ScopeAtom {
                    source_root_id: source.id,
                    kind: PathScope::Subtree,
                    path: String::new(),
                });
            }
            ensure!(
                !atoms.is_empty(),
                "selected index has no complete source scope"
            );
            sources.push((*index, atoms));
        }
        confirm(
            prompt,
            options.yes,
            "Create a dedicated index set containing exactly the selected indexes and their displayed source roots?",
        )?;
        let id = QueryProjectId::from_identity(session.operation);
        session
            .apply(
                context,
                CatalogOperation::CreateProject {
                    id,
                    name: format!("{} standalone setup", options.client.label()),
                },
            )
            .await?;
        for (index, scope) in sources {
            let descriptor = session.descriptor(context, None).await?;
            let CatalogItem::Project(project) = session
                .detail(
                    CatalogEntity::Project { id },
                    descriptor.namespace,
                    descriptor.catalog_revision,
                )
                .await?
            else {
                bail!("unexpected project detail");
            };
            session
                .apply(
                    context,
                    CatalogOperation::SetMembership {
                        membership: Membership {
                            project_id: id,
                            index_id: index,
                            revision: CatalogRevision::INITIAL,
                            scope,
                        },
                        expected_scope_revision: project.scope_revision,
                        expected_membership_revision: None,
                    },
                )
                .await?;
        }
        let descriptor = session.descriptor(context, None).await?;
        let CatalogItem::Project(project) = session
            .detail(
                CatalogEntity::Project { id },
                descriptor.namespace,
                descriptor.catalog_revision,
            )
            .await?
        else {
            bail!("unexpected project detail");
        };
        session
            .apply(
                context,
                CatalogOperation::SetProjectDefaults {
                    project_id: id,
                    expected_defaults_revision: project.defaults_revision,
                    index_ids: indexes.clone(),
                },
            )
            .await?;
        projects.push(id);
    }
    let descriptor = session.descriptor(context, None).await?;
    let key = session.key().await?;
    ensure!(
        key.namespace == descriptor.namespace,
        "catalog namespace changed"
    );
    let selection = GrantDraftSelection {
        key,
        expected_catalog_revision: descriptor.catalog_revision,
        project_ids: projects.clone(),
        index_ids: (!indexes.is_empty()).then_some(indexes.clone()),
    };
    selection.validate()?;
    let draft = match session
        .call(ControlMethod::CatalogGrantDraft {
            context_key: context,
            selection: selection.clone(),
        })
        .await?
    {
        ControlResponse::CatalogGrantDraft { draft } => draft,
        other => return refused(other),
    };
    draft.request.validate()?;
    ensure!(
        draft.request.key == selection.key
            && draft.request.expected_catalog_revision == selection.expected_catalog_revision
            && draft.preview_digest == draft.preview.identity()?
            && draft.preview.request_digest == draft.request.identity()?,
        "native grant review crossed selected request"
    );
    let CatalogOperation::CreateGrant { grant } = &draft.request.operation else {
        bail!("native grant review returned another operation");
    };
    review_grant_scope(session, &descriptor, grant, &projects, &indexes, prompt).await?;
    confirm(
        prompt,
        options.yes,
        &format!(
            "Allow {} to read exactly this scope through AIRK's local HTTPS MCP service?",
            options.client.label()
        ),
    )?;
    let request = draft.request.clone();
    let grant_id = grant.id;
    let grant_revision = grant.revision;
    let receipt = match session
        .submit(ControlMethod::CatalogApply {
            request: draft.request,
            preview_digest: draft.preview_digest,
        })
        .await?
    {
        ControlResponse::CatalogApplied { receipt, .. } => receipt,
        other => return refused(other),
    };
    ensure!(
        receipt.key == request.key
            && receipt.caller == request.caller
            && receipt.request_digest == request.identity()?,
        "native grant receipt crossed request"
    );
    let combined = CombinedConnectRequest {
        schema: 1,
        context_key: context,
        namespace: descriptor.namespace,
        expected_catalog_revision: receipt.catalog_revision,
        grant_id,
        expected_grant_revision: grant_revision,
        project_ids: projects,
        index_ids: selection.index_ids,
    };
    combined.validate()?;
    Ok(HttpSelection::Combined {
        selection: combined,
    })
}

async fn review_grant_scope<T: Transport>(
    session: &mut Session<T>,
    descriptor: &airk_core::catalog_host::NativeHostContextDescriptor,
    grant: &airk_core::index_catalog::ReviewedGrant,
    projects: &[QueryProjectId],
    indexes: &[IndexId],
    prompt: &mut impl SetupPrompt,
) -> Result<()> {
    grant.validate()?;
    ensure!(
        grant.host == descriptor.host && !grant.revoked,
        "native grant host changed"
    );
    let actual_projects = grant
        .memberships
        .iter()
        .map(|member| member.membership.project_id)
        .collect::<BTreeSet<_>>();
    let actual_indexes = grant
        .memberships
        .iter()
        .map(|member| member.membership.index_id)
        .collect::<BTreeSet<_>>();
    ensure!(
        actual_projects == projects.iter().copied().collect()
            && (indexes.is_empty() || actual_indexes == indexes.iter().copied().collect()),
        "native grant widened or dropped selected scope"
    );
    prompt.message("Review the complete native read scope:")?;
    for member in &grant.memberships {
        let CatalogItem::Index(index) = session
            .detail(
                CatalogEntity::Index {
                    id: member.membership.index_id,
                },
                descriptor.namespace,
                descriptor.catalog_revision,
            )
            .await?
        else {
            bail!("unexpected index detail");
        };
        prompt.message(&format!("  Index {:?} ({})", index.name, index.id))?;
        for atom in &member.membership.scope {
            let CatalogItem::Source(source) = session
                .detail(
                    CatalogEntity::Source {
                        id: atom.source_root_id,
                    },
                    descriptor.namespace,
                    descriptor.catalog_revision,
                )
                .await?
            else {
                bail!("unexpected source detail");
            };
            ensure!(
                source.index_id == index.id && source.enabled,
                "native grant source changed"
            );
            session.authorized_directories.push(authorized_directory(
                Path::new(&source.canonical_path),
                atom,
            ));
            prompt.message(&format!(
                "    {:?} / {:?} ({:?})",
                source.canonical_path, atom.path, atom.kind
            ))?;
        }
    }
    Ok(())
}

fn authorized_directory(source: &Path, atom: &ScopeAtom) -> Option<PathBuf> {
    (atom.kind == PathScope::Subtree).then(|| source.join(&atom.path))
}

fn sole_authorized_directory(paths: &[Option<PathBuf>]) -> Option<PathBuf> {
    let first = paths.first()?.as_ref()?;
    paths
        .iter()
        .all(|path| path.as_ref() == Some(first))
        .then(|| first.clone())
}

fn unique_selection(projects: &[QueryProjectId], indexes: &[IndexId]) -> Result<()> {
    airk_core::index_catalog::unique_bounded(projects, MAX_SELECTION)?;
    airk_core::index_catalog::unique_bounded(indexes, MAX_SELECTION)
}

fn scope_choices(
    projects: &[CatalogItem],
    indexes: &[CatalogItem],
) -> Result<Vec<(String, String)>> {
    let mut choices = Vec::new();
    for (position, item) in projects.iter().enumerate() {
        let CatalogItem::Project(project) = item else {
            bail!("unexpected project catalog item");
        };
        choices.push((
            format!("p{}", position + 1),
            format!("index set {:?} ({})", project.name, project.id),
        ));
    }
    for (position, item) in indexes.iter().enumerate() {
        let CatalogItem::Index(index) = item else {
            bail!("unexpected index catalog item");
        };
        if !index.tombstone {
            choices.push((
                format!("i{}", position + 1),
                format!(
                    "index {:?}: {:?} ({})",
                    index.name, index.backing.root, index.id
                ),
            ));
        }
    }
    Ok(choices)
}

fn parse_choice(
    answer: &str,
    projects: &[CatalogItem],
    indexes: &[CatalogItem],
) -> Result<(Vec<QueryProjectId>, Vec<IndexId>)> {
    let mut selected_projects = Vec::new();
    let mut selected_indexes = Vec::new();
    for choice in answer.split(',').map(str::trim) {
        let (kind, position) = choice
            .split_at_checked(1)
            .context("select at least one displayed scope")?;
        let position = position
            .parse::<usize>()
            .context("select a displayed pN or iN entry")?
            .checked_sub(1)
            .context("scope positions start at one")?;
        match kind {
            "p" => match projects.get(position) {
                Some(CatalogItem::Project(project)) => selected_projects.push(project.id),
                _ => bail!("selected index set was not displayed"),
            },
            "i" => match indexes.get(position) {
                Some(CatalogItem::Index(index)) if !index.tombstone => {
                    selected_indexes.push(index.id)
                }
                _ => bail!("selected index was not displayed"),
            },
            _ => bail!("select a displayed pN or iN entry"),
        }
    }
    ensure!(
        selected_projects.is_empty() || selected_indexes.is_empty(),
        "choose index sets or individual indexes; explicit --project and --index can narrow a set"
    );
    unique_selection(&selected_projects, &selected_indexes)?;
    ensure!(
        !selected_projects.is_empty() || !selected_indexes.is_empty(),
        "no scope selected"
    );
    Ok((selected_projects, selected_indexes))
}

#[cfg(test)]
#[path = "standalone_setup_tests.rs"]
mod tests;
