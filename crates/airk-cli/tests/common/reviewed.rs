// SPDX-License-Identifier: Apache-2.0
//! Explicit fresh installed-catalog fixtures. These are locally approved development
//! images, not signed release payloads or a replacement for release qualification.
use airk_core::{
    catalog_fences::FencedReceipt,
    catalog_management::{CatalogReceipt, CatalogRequest},
    catalog_setup::{IndexSetupDraft, IndexSetupReceipt, SetupStage},
    service::{ControlMethod, ProjectRegistration},
    service_http::HttpSelection,
};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};

pub(super) struct ReviewedService {
    pub install: PathBuf,
    pub control: PathBuf,
    pub executable: PathBuf,
    pub project: ProjectRegistration,
    pub selection: HttpSelection,
    namespace: airk_core::identity::Incarnation,
    runtime_digest: airk_core::identity::Sha256Digest,
    stopped: Mutex<bool>,
}

fn nonce() -> Result<String> {
    Ok(airk_kernel::service_broker::new_instance_id()?.to_string())
}

pub(super) fn git_environment(command: &mut Command, owner: &Path) {
    command
        .env("GIT_CONFIG_GLOBAL", owner.join("git.config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_COUNT", "0")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG");
}

fn git_configuration(owner: &Path) -> Result<()> {
    let excludes = owner.join("git-empty-excludes");
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&excludes)?;
    let excludes = excludes.to_str().context("non-UTF8 private Git excludes")?;
    ensure!(
        !excludes.chars().any(char::is_control),
        "invalid private Git excludes path"
    );
    let escaped = excludes.replace('\\', "/").replace('"', "\\\"");
    let mut config = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(owner.join("git.config"))?;
    write!(config, "[core]\n\texcludesFile = \"{escaped}\"\n")?;
    Ok(())
}

fn command(executable: &Path, owner: &Path) -> Command {
    let mut value = Command::new(executable);
    // The managed child hosts the HTTP frontend and project workers. Let the
    // fixture's persisted usage policy govern them; a process-wide opt-out
    // would silently defeat explicit Usage configuration and attribution tests.
    value
        .env_remove("AIRK_USAGE_MODE")
        .env("AIRK_MCP_RESPONSE_MODE", "full");
    git_environment(&mut value, owner);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        value.creation_flags(0x08000000);
    }
    value
}

/// The child is ours; its output is file-backed, bounded before parsing, and an
/// expired command never authorizes a replay. Background service ownership is
/// settled separately by the native lifecycle receipt.
fn cli(executable: &Path, args: &[&str], input: Option<&Value>, owner: &Path) -> Result<Value> {
    let input = input.map(serde_json::to_vec).transpose()?;
    ensure!(
        input.as_ref().is_none_or(|bytes| bytes.len() <= 16 * 1024),
        "fixture CLI input exceeds budget"
    );
    let id = nonce()?;
    let stdout = owner.join(format!("command-{id}.stdout"));
    let stderr = owner.join(format!("command-{id}.stderr"));
    let mut cmd = command(executable, owner);
    cmd.args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(fs::File::create(&stdout)?)
        .stderr(fs::File::create(&stderr)?);
    let mut child = cmd.spawn()?;
    if let Some(bytes) = input
        && let Err(error) = child
            .stdin
            .take()
            .context("missing owned stdin")?
            .write_all(&bytes)
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error.into());
    }
    let end = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= end
            || fs::metadata(&stdout)?.len() > 1024 * 1024
            || fs::metadata(&stderr)?.len() > 1024 * 1024
        {
            let _ = child.kill();
            let _ = child.wait();
            bail!("owned fixture command expired; retain {}", owner.display());
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    ensure!(
        fs::metadata(&stdout)?.len() <= 1024 * 1024 && fs::metadata(&stderr)?.len() <= 1024 * 1024,
        "fixture CLI output exceeds budget; retain {}",
        owner.display()
    );
    ensure!(
        status.success(),
        "owned fixture CLI failed ({status}); see {}",
        stderr.display()
    );
    serde_json::from_slice(&fs::read(stdout)?).context("invalid native fixture CLI JSON")
}

struct Rpc {
    control: PathBuf,
    runtime: tokio::runtime::Runtime,
    end: Instant,
    count: usize,
    bytes: usize,
    evidence: PathBuf,
}
impl Rpc {
    fn new(control: &Path, evidence: &Path) -> Result<Self> {
        Ok(Self {
            control: control.into(),
            runtime: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
            end: Instant::now() + Duration::from_secs(120),
            count: 0,
            bytes: 0,
            evidence: evidence.into(),
        })
    }
    fn call(&mut self, method: Value) -> Result<Value> {
        ensure!(
            self.count < 128 && Instant::now() < self.end,
            "reviewed fixture RPC budget expired"
        );
        self.count += 1;
        let method: ControlMethod = serde_json::from_value(method)?;
        let timeout = self
            .end
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(30));
        let response = self.runtime.block_on(async {
            tokio::time::timeout(
                timeout,
                airk_cli::service_runtime::request(&self.control, method),
            )
            .await
        })??;
        let bytes = serde_json::to_vec(&response)?;
        self.bytes += bytes.len();
        ensure!(
            self.bytes <= 1024 * 1024,
            "reviewed fixture response budget exceeded"
        );
        fs::write(
            self.evidence
                .join(format!("rpc-{}-{}.json", nonce()?, self.count)),
            &bytes,
        )?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    fn key(&mut self) -> Result<Value> {
        let value = self.call(json!("request_epoch"))?;
        ensure!(
            value["status"] == "request_epoch",
            "missing native request epoch"
        );
        Ok(json!({"namespace":value["namespace"],"epoch":value["epoch"],"nonce":nonce()?}))
    }
}

fn regular(path: &Path, max: u64) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= max,
        "invalid fixture input {}",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "redirected fixture input"
        );
    }
    Ok(())
}
fn copy_file(source: &Path, target: &Path, max: u64) -> Result<()> {
    regular(source, max)?;
    ensure!(!target.try_exists()?, "fixture destination already exists");
    fs::create_dir_all(target.parent().context("fixture file has no parent")?)?;
    fs::copy(source, target)?;
    Ok(())
}
fn copy_notices(source: &Path, destination: &Path, count: &mut usize) -> Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let metadata = entry.file_type()?;
        ensure!(!metadata.is_symlink(), "linked fixture notice");
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_notices(&entry.path(), &target, count)?;
        } else {
            *count += 1;
            ensure!(
                *count <= 1800,
                "fixture notice roster exceeds installer bound"
            );
            copy_file(&entry.path(), &target, 2 * 1024 * 1024)?;
        }
    }
    Ok(())
}
fn inventory(root: &Path, entries: &mut BTreeMap<String, String>, directory: &Path) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        ensure!(!entry.file_type()?.is_symlink(), "linked fixture inventory");
        if entry.file_type()?.is_dir() {
            inventory(root, entries, &entry.path())?;
        } else {
            regular(&entry.path(), 2 * 1024 * 1024 * 1024)?;
            let name = entry
                .path()
                .strip_prefix(root)?
                .to_str()
                .context("non-UTF8 fixture inventory")?
                .replace('\\', "/");
            ensure!(
                name != "airk-install.json",
                "unexpected preexisting fixture manifest"
            );
            entries.insert(name, airk_cli::runtime_pair::digest_file(&entry.path())?);
        }
    }
    ensure!(entries.len() <= 2048, "fixture inventory exceeds bound");
    Ok(())
}

fn payload(owner: &Path) -> Result<(PathBuf, String)> {
    let root = owner.join("payload");
    fs::create_dir(&root)?;
    let current = Path::new(env!("CARGO_BIN_EXE_airk"));
    let current_hash = airk_cli::runtime_pair::digest_file(current)?;
    if let Some(source) = std::env::var_os("AIRK_TEST_RUNTIME_DIR") {
        let source = PathBuf::from(source).canonicalize()?;
        regular(&source.join("airk-install.json"), 512 * 1024)?;
        let manifest: airk_cli::runtime_install::InstallManifest =
            serde_json::from_slice(&fs::read(source.join("airk-install.json"))?)?;
        ensure!(
            manifest.schema == 1
                && manifest.version == env!("CARGO_PKG_VERSION")
                && manifest.os == std::env::consts::OS
                && manifest.arch == std::env::consts::ARCH,
            "external fixture target/version differs"
        );
        ensure!(
            manifest.files.len() <= 2048
                && manifest.files.get(airk_cli::runtime_install::main_name())
                    == Some(&current_hash),
            "external fixture does not contain current Cargo main"
        );
        for (name, hash) in &manifest.files {
            let path = Path::new(name);
            ensure!(
                !path.is_absolute()
                    && path
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_))),
                "invalid fixture inventory path"
            );
            regular(&source.join(path), 2 * 1024 * 1024 * 1024)?;
            ensure!(
                airk_cli::runtime_pair::digest_file(&source.join(path))? == *hash,
                "external fixture hash mismatch"
            );
            copy_file(&source.join(path), &root.join(path), 2 * 1024 * 1024 * 1024)?;
        }
        copy_file(
            &source.join("airk-install.json"),
            &root.join("airk-install.json"),
            512 * 1024,
        )?;
    } else {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        copy_file(
            current,
            &root.join(airk_cli::runtime_install::main_name()),
            2 * 1024 * 1024 * 1024,
        )?;
        for name in ["LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"] {
            copy_file(&repository.join(name), &root.join(name), 2 * 1024 * 1024)?;
        }
        copy_notices(
            &repository.join("third_party/licenses"),
            &root.join("notices/checked-in"),
            &mut 0,
        )?;
        fs::write(
            root.join("airk-runtime.json"),
            serde_json::to_vec(
                &json!({"schema":2,"version":env!("CARGO_PKG_VERSION"),"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"airk_sha256":current_hash}),
            )?,
        )?;
        fs::create_dir_all(root.join("notices"))?;
        fs::write(
            root.join("notices/fixture-provenance.json"),
            serde_json::to_vec(
                &json!({"schema":1,"purpose":"private Cargo integration fixture","approval":"explicit local organization digest","release_qualified":false,"signature_qualified":false,"linked_dependency_notice_qualified":false}),
            )?,
        )?;
        let mut files = BTreeMap::new();
        inventory(&root, &mut files, &root)?;
        fs::write(
            root.join("airk-install.json"),
            serde_json::to_vec(&airk_cli::runtime_install::InstallManifest {
                schema: 1,
                version: env!("CARGO_PKG_VERSION").into(),
                os: std::env::consts::OS.into(),
                arch: std::env::consts::ARCH.into(),
                files,
            })?,
        )?;
    }
    airk_cli::runtime_pair::verify_installed_pair(
        &root.join(airk_cli::runtime_install::main_name()),
        env!("CARGO_PKG_VERSION"),
    )?;
    let manifest = root.join("airk-runtime.json");
    regular(&manifest, 4096)?;
    let pair: airk_cli::runtime_pair::RuntimePair = serde_json::from_slice(&fs::read(&manifest)?)?;
    pair.validate_target(env!("CARGO_PKG_VERSION"))?;
    ensure!(
        pair.is_single_executable() && pair.airk_sha256 == current_hash,
        "fixture is not current single image"
    );
    let digest = airk_cli::runtime_pair::digest_file(&root.join("airk-install.json"))?;
    Ok((root, digest))
}

fn apply(rpc: &mut Rpc, request: Value, preview: Value) -> Result<Value> {
    let typed: CatalogRequest = serde_json::from_value(request.clone())?;
    let digest = typed.identity()?;
    // Submit once. Uncertain transport outcomes permit public receipt reads,
    // never another mutation request.
    let mut response = match rpc
        .call(json!({"catalog_apply":{"request":request,"preview_digest":preview}}))
    {
        Ok(value) => value,
        Err(error) => {
            fs::write(
                rpc.evidence
                    .join(format!("unknown-catalog-{}.txt", nonce()?)),
                format!("{error:#}"),
            )?;
            rpc.call(json!({"catalog_receipt":{"caller":request["caller"],"key":request["key"]}}))?
        }
    };
    loop {
        let receipt =
            if response["status"] == "catalog_applied" || response["status"] == "catalog_receipt" {
                response.get("receipt").filter(|v| !v.is_null())
            } else {
                None
            };
        if let Some(receipt) = receipt {
            let receipt: CatalogReceipt = serde_json::from_value(receipt.clone())?;
            ensure!(
                receipt.schema == 1
                    && receipt.operation_id
                        == airk_core::catalog_management::catalog_operation_id(
                            typed.caller,
                            &typed.key
                        )?
                    && receipt.caller == typed.caller
                    && receipt.key == typed.key
                    && receipt.request_digest == digest
                    && receipt.catalog_revision == typed.expected_catalog_revision.next()?,
                "crossed native catalog receipt"
            );
            if receipt.authority_fence_required {
                let proof = if response["status"] == "catalog_applied" {
                    response.get("fenced_receipt")
                } else {
                    response.pointer("/fence/value")
                };
                if let Some(proof) = proof.filter(|v| !v.is_null())
                    && (response["status"] == "catalog_applied"
                        || response
                            .pointer("/fence/state")
                            .is_some_and(|v| v == "applied"))
                {
                    let proof: FencedReceipt = serde_json::from_value(proof.clone())?;
                    proof.validate()?;
                    ensure!(
                        proof.request == typed
                            && serde_json::to_value(proof.preview_digest)? == preview
                            && proof.metadata == receipt,
                        "crossed native fence proof"
                    );
                    return Ok(serde_json::to_value(receipt.catalog_revision)?);
                }
            } else {
                return Ok(serde_json::to_value(receipt.catalog_revision)?);
            }
        } else if response["status"] == "catalog_outcome_unknown" {
            ensure!(
                response["caller"] == request["caller"]
                    && response["key"] == request["key"]
                    && response["request_digest"] == serde_json::to_value(digest)?,
                "crossed unknown catalog outcome"
            );
        } else if response["status"] != "catalog_receipt" {
            bail!("native reviewed mutation failed: {response}");
        }
        std::thread::sleep(Duration::from_millis(20));
        response =
            rpc.call(json!({"catalog_receipt":{"caller":request["caller"],"key":request["key"]}}))?;
    }
}

fn setup_once(rpc: &mut Rpc, draft: Value) -> Result<IndexSetupReceipt> {
    let typed: IndexSetupDraft = serde_json::from_value(draft.clone())?;
    typed.validate()?;
    let digest = typed.request.identity()?;
    let mut response = match rpc.call(json!({"catalog_index_setup_apply":{"draft":draft}})) {
        Ok(value) => value,
        Err(error) => {
            fs::write(
                rpc.evidence.join("unknown-index-setup.txt"),
                format!("{error:#}"),
            )?;
            rpc.call(json!({"catalog_index_setup_receipt":{"caller":typed.request.caller,"key":typed.request.key}}))?
        }
    };
    loop {
        if response["status"] == "catalog_index_setup_applied"
            || response["status"] == "catalog_index_setup_receipt"
        {
            if let Some(receipt) = response.get("receipt").filter(|value| !value.is_null()) {
                let receipt: IndexSetupReceipt = serde_json::from_value(receipt.clone())?;
                receipt.validate()?;
                ensure!(
                    receipt.request == typed.request && receipt.request_digest == digest,
                    "crossed native setup receipt"
                );
                if receipt.stage == SetupStage::Ready {
                    return Ok(receipt);
                }
            }
        } else if response["status"] == "catalog_outcome_unknown" {
            ensure!(
                response["caller"] == serde_json::to_value(typed.request.caller)?
                    && response["key"] == serde_json::to_value(&typed.request.key)?
                    && response["request_digest"] == serde_json::to_value(digest)?,
                "crossed unknown setup outcome"
            );
        } else {
            bail!("native reviewed setup failed: {response}");
        }
        std::thread::sleep(Duration::from_millis(50));
        response = rpc.call(json!({"catalog_index_setup_receipt":{"caller":typed.request.caller,"key":typed.request.key}}))?;
    }
}

fn selection(
    rpc: &mut Rpc,
    registration: &ProjectRegistration,
    setup: Option<IndexSetupReceipt>,
) -> Result<HttpSelection> {
    let context_key = format!("{}{}", nonce()?, nonce()?);
    let descriptor = rpc.call(
        json!({"catalog_host_context":{"key":context_key,"expected_catalog_revision":null}}),
    )?;
    let (index, sources, mut revision) = if let Some(setup) = setup {
        setup.validate()?;
        ensure!(
            setup.stage == SetupStage::Ready && setup.request.registration == *registration,
            "crossed or incomplete setup receipt"
        );
        (
            serde_json::to_value(setup.index_id)?,
            setup.source_ids,
            descriptor["descriptor"]["catalog_revision"].clone(),
        )
    } else {
        bail!("reviewed selection requires this fixture's exact setup receipt");
    };
    let project = format!("prj_{}", nonce()?);
    for operation in [
        json!({"operation":"create_project","id":project,"name":"Owned native fixture Project"}),
        json!({"operation":"set_membership","membership":{"project_id":project,"index_id":index,"revision":1,"scope":[{"source_root_id":sources.first().context("setup has no source")?,"kind":"subtree","path":""}]},"expected_scope_revision":1,"expected_membership_revision":null}),
    ] {
        let request = json!({"caller":nonce()?,"key":rpc.key()?,"expected_catalog_revision":revision,"operation":operation});
        let preview = rpc.call(json!({"catalog_preview":{"request":request}}))?;
        revision = apply(rpc, request, preview["preview_digest"].clone())?;
    }
    let descriptor_now = rpc.call(
        json!({"catalog_host_context":{"key":context_key,"expected_catalog_revision":revision}}),
    )?;
    ensure!(
        descriptor_now["descriptor"]["namespace"] == descriptor["descriptor"]["namespace"],
        "fixture namespace changed"
    );
    let key = rpc.key()?;
    let grant = rpc.call(json!({"catalog_grant_draft":{"context_key":context_key,"selection":{"key":key,"expected_catalog_revision":revision,"project_ids":[project],"index_ids":[index]}}}))?;
    let draft = &grant["draft"];
    let revision = apply(
        rpc,
        draft["request"].clone(),
        draft["preview_digest"].clone(),
    )?;
    let selection = json!({"kind":"scoped","selection":{"schema":1,"context_key":context_key,"namespace":descriptor_now["descriptor"]["namespace"],"expected_catalog_revision":revision,"grant_id":draft["request"]["operation"]["grant"]["id"],"expected_grant_revision":1,"index_id":index,"project_ids":[project]}});
    let detail = rpc.call(json!({"catalog_detail":{"entity":{"kind":"index","id":index},"expected_catalog_revision":revision}}))?;
    ensure!(
        detail["detail"]["item"]["value"]["backing"] == serde_json::to_value(registration)?,
        "fixture backing changed"
    );
    Ok(serde_json::from_value(selection)?)
}

fn installed_paths(
    executable: &Path,
    install: &Path,
    state: &Path,
    owner: &Path,
) -> Result<(PathBuf, PathBuf)> {
    let inspected = cli(
        executable,
        &[
            "runtime",
            "inspect",
            "--install-dir",
            install.to_str().context("non-UTF8 install")?,
        ],
        None,
        owner,
    )?;
    let control = PathBuf::from(
        inspected["control_dir"]
            .as_str()
            .context("missing installed control")?,
    )
    .canonicalize()?;
    let selected = PathBuf::from(
        inspected["executable"]
            .as_str()
            .context("missing selected executable")?,
    )
    .canonicalize()?;
    ensure!(
        control == state.join("control").canonicalize()?
            && selected.starts_with(install.canonicalize()?)
            && airk_cli::runtime_pair::digest_file(&selected)?
                == airk_cli::runtime_pair::digest_file(executable)?,
        "fixture installed paths/image escaped owner"
    );
    Ok((control, selected))
}

pub(super) fn start(
    root: &Path,
    data: &Path,
    label: &str,
    owner: &Path,
) -> Result<ReviewedService> {
    let root = root.canonicalize()?;
    let owner = owner.canonicalize()?;
    git_configuration(&owner)?;
    ensure!(
        !root.starts_with(&owner) && !owner.starts_with(&root),
        "fixture source overlaps service owner"
    );
    ensure!(
        !data.try_exists()? || fs::read_dir(data)?.next().is_none(),
        "reviewed fresh fixture refuses existing index data"
    );
    fs::create_dir_all(data)?;
    let data = data.canonicalize()?;
    ensure!(
        !data.starts_with(&root)
            && !root.starts_with(&data)
            && !data.starts_with(&owner)
            && !owner.starts_with(&data),
        "fixture data overlaps source or owner"
    );
    let (payload, digest) = payload(&owner)?;
    let executable = payload.join(airk_cli::runtime_install::main_name());
    let install = owner.join("install");
    let state = owner.join("state");
    let path = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .context("non-UTF8 fixture path")
    };
    cli(
        &executable,
        &[
            "runtime",
            "stage",
            "--source",
            &path(&payload)?,
            "--install-dir",
            &path(&install)?,
            "--state-dir",
            &path(&state)?,
            "--approved-sha256",
            &digest,
            "--verification-basis",
            "organization-digest",
        ],
        None,
        &owner,
    )?;
    let result = (|| -> Result<_> {
        cli(
            &executable,
            &[
                "runtime",
                "activate",
                "--install-dir",
                &path(&install)?,
                "--runtime-sha256",
                &digest,
            ],
            None,
            &owner,
        )?;
        let (control, executable) = installed_paths(&executable, &install, &state, &owner)?;
        let mut rpc = Rpc::new(&control, &owner)?;
        let descriptor = rpc.call(json!({"catalog_host_context":{"key":format!("{}{}",nonce()?,nonce()?),"expected_catalog_revision":null}}))?;
        let key = rpc.key()?;
        let draft = rpc.call(json!({"catalog_index_setup_draft":{"selection":{"key":key,"expected_catalog_revision":descriptor["descriptor"]["catalog_revision"],"name":"Owned native fixture Index","primary_root":root,"data_root":data,"collection":label,"additional_roots":[],"setup_watch":false}}}))?;
        let setup = setup_once(&mut rpc, draft["draft"].clone())?;
        let project = setup.request.registration.clone();
        let expected = airk_kernel::service_projects::registration(&root, &data, label)?;
        ensure!(project == expected, "native setup changed fixture roots");
        fs::write(
            owner.join("reviewed-setup.json"),
            serde_json::to_vec(&setup)?,
        )?;
        let namespace = setup.request.key.namespace;
        let selection = selection(&mut rpc, &project, Some(setup))?;
        Ok(ReviewedService {
            install: install.clone(),
            control,
            executable,
            project,
            selection,
            namespace,
            runtime_digest: digest.parse()?,
            stopped: Mutex::new(false),
        })
    })();
    if result.is_err() {
        // The caller retains its TempDir on any error, including an uncertain
        // activation. Never delete a possibly live service's private state.
        let cleanup = installed_paths(&executable, &install, &state, &owner)
            .and_then(|(control, selected)| shutdown_paths(&install, &control, &selected, None));
        let _ = fs::write(
            owner.join("start-failed-retain.txt"),
            format!(
                "Startup failed; retain fixture evidence. Canonical Stop observation: {cleanup:?}\n"
            ),
        );
    }
    result
}

pub(super) fn additional_project(
    service: &ReviewedService,
    root: &Path,
    data: &Path,
    label: &str,
) -> Result<ProjectRegistration> {
    let root = root.canonicalize()?;
    let owner = service.install.parent().context("reviewed owner missing")?;
    ensure!(
        !root.starts_with(owner) && !owner.starts_with(&root),
        "additional fixture source overlaps service owner"
    );
    ensure!(
        data.is_absolute(),
        "additional fixture data root must be explicit"
    );
    let mut rpc = Rpc::new(&service.control, owner)?;
    let descriptor = rpc.call(json!({"catalog_host_context":{"key":format!("{}{}",nonce()?,nonce()?),"expected_catalog_revision":null}}))?;
    let key = rpc.key()?;
    let draft = rpc.call(json!({"catalog_index_setup_draft":{"selection":{"key":key,
        "expected_catalog_revision":descriptor["descriptor"]["catalog_revision"],"name":format!("Owned additional {label}"),
        "primary_root":root,"data_root":data,"collection":label,"additional_roots":[],"setup_watch":false}}}))?;
    ensure!(
        draft["draft"].is_object(),
        "additional native IndexSetup draft refused: {draft}"
    );
    let setup = setup_once(&mut rpc, draft["draft"].clone())?;
    let project = setup.request.registration.clone();
    ensure!(
        project == airk_kernel::service_projects::registration(&root, data, label)?,
        "additional canonical setup changed source/storage identity"
    );
    fs::write(
        owner.join(format!("additional-setup-{}.json", project.binding)),
        serde_json::to_vec(&setup)?,
    )?;
    Ok(project)
}

/// Actual native IPC draft/apply with one mutation and its original fenced
/// receipt. This is not a separate qualification of CLI JSON-stream framing.
pub(super) fn retire_index(service: &ReviewedService) -> Result<Value> {
    let owner = service.install.parent().context("reviewed owner missing")?;
    let HttpSelection::Scoped { selection } = &service.selection else {
        bail!("retirement fixture requires its exact scoped Index");
    };
    let mut rpc = Rpc::new(&service.control, owner)?;
    let descriptor = rpc.call(json!({"catalog_host_context":{
        "key":format!("{}{}",nonce()?,nonce()?),"expected_catalog_revision":null
    }}))?;
    let key = rpc.key()?;
    let response = rpc.call(json!({"catalog_source_mutation_draft":{"selection":{
        "key":key,"expected_catalog_revision":descriptor["descriptor"]["catalog_revision"],
        "index_id":selection.index_id,"change":{"operation":"retire_index"}
    }}}))?;
    let draft: airk_core::catalog_source_draft::SourceMutationDraft =
        serde_json::from_value(response["draft"].clone())
            .with_context(|| format!("native retirement draft refused: {response}"))?;
    draft.validate()?;
    ensure!(
        matches!(&draft.request.operation,
        airk_core::catalog_management::CatalogOperation::RetireIndex { id, .. } if *id == selection.index_id),
        "retirement draft crossed the selected Index"
    );
    fs::write(
        owner.join("headless-retirement-draft.json"),
        serde_json::to_vec(&draft)?,
    )?;
    apply(
        &mut rpc,
        serde_json::to_value(&draft.request)?,
        serde_json::to_value(draft.preview_digest)?,
    )
}

pub(super) fn reviewed_http_selection(
    control: &Path,
    registration: &ProjectRegistration,
) -> Result<HttpSelection> {
    let control = control.canonicalize()?;
    let owner = control
        .parent()
        .and_then(Path::parent)
        .context("missing reviewed fixture owner")?;
    ensure!(
        control == owner.join("state/control").canonicalize()?,
        "reviewed control is outside exact fixture layout"
    );
    regular(&owner.join("reviewed-setup.json"), 16 * 1024)?;
    let mut setup: IndexSetupReceipt =
        serde_json::from_slice(&fs::read(owner.join("reviewed-setup.json"))?)?;
    if setup.request.registration != *registration {
        let path = owner.join(format!("additional-setup-{}.json", registration.binding));
        regular(&path, 16 * 1024)?;
        setup = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(
            setup.request.registration == *registration,
            "additional setup receipt crossed project"
        );
    }
    selection(&mut Rpc::new(&control, owner)?, registration, Some(setup))
}

pub(super) fn shutdown(service: &ReviewedService) -> Result<()> {
    let mut stopped = service
        .stopped
        .lock()
        .map_err(|_| anyhow::anyhow!("fixture lifecycle lock poisoned"))?;
    if !*stopped {
        shutdown_paths(
            &service.install,
            &service.control,
            &service.executable,
            Some((service.namespace, service.runtime_digest)),
        )?;
        *stopped = true;
    }
    verify_stopped(service)?;
    Ok(())
}

fn verify_stopped(service: &ReviewedService) -> Result<()> {
    let owner = service.install.parent().context("missing fixture owner")?;
    let observed = cli(
        &service.executable,
        &[
            "runtime",
            "inspect",
            "--install-dir",
            service.install.to_str().context("non-UTF8 install")?,
        ],
        None,
        owner,
    )?;
    ensure!(
        observed["lifecycle"]["explicitly_stopped"] == true
            && observed["lifecycle"]["pending"].is_null()
            && observed["registry_namespace"] == serde_json::to_value(service.namespace)?
            && observed["selection"]["active"] == service.runtime_digest.to_string(),
        "fixture Stop no longer owns the same stopped installation"
    );
    let _lease = airk_store::service::ServiceLease::acquire(&service.control)?;
    Ok(())
}

pub(super) fn with_stopped_owner<T>(
    service: &ReviewedService,
    callback: impl FnOnce(&ProjectRegistration) -> Result<T>,
) -> Result<T> {
    let mut stopped = service
        .stopped
        .lock()
        .map_err(|_| anyhow::anyhow!("fixture lifecycle lock poisoned"))?;
    if !*stopped {
        shutdown_paths(
            &service.install,
            &service.control,
            &service.executable,
            Some((service.namespace, service.runtime_digest)),
        )?;
        *stopped = true;
    }
    verify_stopped(service)?;
    let lease = airk_store::service::ServiceLease::acquire(&service.control)?;
    let outcome = callback(&service.project);
    drop(lease);
    // Start is submitted once. A lost response leaves this owner retained;
    // subsequent cleanup may only stop a newly authenticated owned instance.
    *stopped = false;
    let owner = service.install.parent().context("missing fixture owner")?;
    cli(
        &service.executable,
        &[
            "runtime",
            "start",
            "--install-dir",
            service.install.to_str().context("non-UTF8 install")?,
        ],
        None,
        owner,
    )?;
    let mut rpc = Rpc::new(&service.control, owner)?;
    let response = rpc.call(json!("health"))?;
    ensure!(
        response["status"] == "health",
        "fixture restart health missing"
    );
    let health: airk_core::service::ServiceHealth =
        serde_json::from_value(response["observation"].clone())?;
    ensure!(
        health.version == env!("CARGO_PKG_VERSION")
            && health.executable_sha256
                == airk_cli::runtime_pair::digest_file(&service.executable)?
            && health.runtime_sha256.as_deref()
                == Some(service.runtime_digest.to_string().as_str())
            && health
                .catalog_clock
                .is_some_and(|clock| clock.namespace == service.namespace),
        "fixture restart namespace/image identity changed"
    );
    outcome
}

fn shutdown_paths(
    install: &Path,
    control: &Path,
    executable: &Path,
    expected: Option<(
        airk_core::identity::Incarnation,
        airk_core::identity::Sha256Digest,
    )>,
) -> Result<()> {
    let owner = install.parent().context("missing fixture owner")?;
    ensure!(
        control.canonicalize()? == owner.join("state/control").canonicalize()?
            && executable
                .canonicalize()?
                .starts_with(install.canonicalize()?),
        "shutdown fixture owner changed"
    );
    let mut rpc = Rpc::new(control, owner)?;
    let health = rpc.call(json!("health"))?;
    ensure!(
        health["status"] == "health",
        "missing owned service health observation"
    );
    let health: airk_core::service::ServiceHealth =
        serde_json::from_value(health["observation"].clone())?;
    ensure!(
        health.version == env!("CARGO_PKG_VERSION")
            && health.executable_sha256 == airk_cli::runtime_pair::digest_file(executable)?,
        "shutdown service image differs"
    );
    let inspected = cli(
        executable,
        &[
            "runtime",
            "inspect",
            "--install-dir",
            install.to_str().context("non-UTF8 install")?,
        ],
        None,
        owner,
    )?;
    let runtime: airk_core::identity::Sha256Digest = health
        .runtime_sha256
        .as_ref()
        .context("owned health has no installed runtime digest")?
        .parse()?;
    if let Some((namespace, expected_runtime)) = expected {
        ensure!(
            runtime == expected_runtime
                && health
                    .catalog_clock
                    .is_some_and(|clock| clock.namespace == namespace),
            "shutdown owned namespace/runtime changed"
        );
    }
    ensure!(
        inspected["selection"]["active"] == runtime.to_string()
            && inspected["approval"]["manifest_sha256"] == runtime.to_string()
            && inspected["executable"]
                .as_str()
                .map(Path::new)
                .context("missing inspected executable")?
                .canonicalize()?
                == executable.canonicalize()?,
        "shutdown runtime approval/selection changed"
    );
    let key = rpc.key()?;
    let request = json!({"caller":nonce()?,"key":key,"action":"stop","expected_instance":health.instance,"runtime":runtime,"force":false});
    fs::write(
        owner.join("shutdown-request.json"),
        serde_json::to_vec(&request)?,
    )?;
    let response = cli(
        executable,
        &[
            "runtime",
            "lifecycle",
            "--install-dir",
            install.to_str().context("non-UTF8 install")?,
        ],
        Some(&request),
        owner,
    )?;
    let mut expected = request;
    expected
        .as_object_mut()
        .context("invalid lifecycle request")?
        .remove("force");
    expected["manager"] = json!(false);
    ensure!(
        response["status"] == "lifecycle"
            && response["record"]["phase"] == "complete"
            && response["record"]["request"] == expected,
        "missing exact native Stop receipt"
    );
    fs::write(
        owner.join("shutdown-complete.json"),
        serde_json::to_vec(&response)?,
    )?;
    ensure!(
        Rpc::new(control, owner)?.call(json!("health")).is_err(),
        "owned service remains available after Stop"
    );
    Ok(())
}
