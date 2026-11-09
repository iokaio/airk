// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_store::service::{collection::Collection, registry::ServiceRegistry};

#[derive(Default)]
struct Prompt {
    answers: std::collections::VecDeque<String>,
    messages: Vec<String>,
}
impl SetupPrompt for Prompt {
    fn interactive(&self) -> bool {
        true
    }
    fn message(&mut self, text: &str) -> Result<()> {
        self.messages.push(text.into());
        Ok(())
    }
    fn answer(&mut self, text: &str) -> Result<String> {
        self.messages.push(text.into());
        self.answers.pop_front().context("unexpected prompt")
    }
}

struct NativeMetadata {
    registry: ServiceRegistry,
    methods: Vec<ControlMethod>,
    refuse_apply: bool,
}
impl Transport for NativeMetadata {
    async fn call(&mut self, method: ControlMethod) -> Result<ControlResponse> {
        self.methods.push(method.clone());
        Ok(match method {
            ControlMethod::RequestEpoch => {
                let (namespace, epoch) = self.registry.saved_request_epoch()?;
                ControlResponse::RequestEpoch { namespace, epoch }
            }
            ControlMethod::CatalogPage { request } => ControlResponse::CatalogPage {
                page: self.registry.catalog_page(&request)?,
                capabilities: airk_store::service::registry::catalog::capabilities(),
            },
            ControlMethod::CatalogDetail {
                entity,
                expected_catalog_revision,
            } => ControlResponse::CatalogDetail {
                detail: self
                    .registry
                    .catalog_detail(&entity, expected_catalog_revision)?,
                capabilities: airk_store::service::registry::catalog::capabilities(),
            },
            ControlMethod::CatalogHostContext {
                key,
                expected_catalog_revision,
            } => {
                let context = crate::catalog_host_context::NativeHostContext::load(
                    &mut self.registry,
                    key,
                    expected_catalog_revision,
                )?;
                ControlResponse::CatalogHostContext {
                    descriptor: context.descriptor().clone(),
                    capabilities: airk_store::service::registry::catalog::capabilities(),
                }
            }
            ControlMethod::CatalogGrantDraft {
                context_key,
                selection,
            } => {
                let context = crate::catalog_host_context::NativeHostContext::load(
                    &mut self.registry,
                    context_key,
                    Some(selection.expected_catalog_revision),
                )?
                .for_review(&selection)?;
                ControlResponse::CatalogGrantDraft {
                    draft: crate::catalog_grant_draft::build_reviewed_grant_draft(
                        &mut self.registry,
                        &context,
                        &selection,
                    )?,
                }
            }
            ControlMethod::CatalogApply {
                request,
                preview_digest,
            } => {
                if self.refuse_apply {
                    ControlResponse::CatalogOutcomeUnknown {
                        code: "fixture_outcome_unknown".into(),
                        caller: request.caller,
                        key: request.key.clone(),
                        request_digest: request.identity()?,
                        observation: None,
                    }
                } else {
                    ControlResponse::CatalogApplied {
                        receipt: self.registry.catalog_apply(&request, preview_digest)?,
                        fenced_receipt: None,
                    }
                }
            }
            other => bail!(
                "unexpected native metadata call: {}",
                serde_json::to_string(&other)?
            ),
        })
    }
}

fn fixture() -> Result<(
    tempfile::TempDir,
    Session<NativeMetadata>,
    QueryProjectId,
    IndexId,
    PathBuf,
)> {
    let owner = tempfile::tempdir()?;
    let base = owner.path().canonicalize()?;
    let control = base.join("control");
    crate::local_ipc::prepare_root(&control)?;
    let source = base.join("source");
    std::fs::create_dir(&source)?;
    std::fs::write(
        source.join("original.txt"),
        b"Native setup source stays unchanged.\n",
    )?;
    let data = base.join("data");
    std::fs::create_dir(&data)?;
    let namespace = Incarnation::from_bytes([121; 16]);
    let mut registry = ServiceRegistry::open(&control, namespace)?;
    let registration = airk_core::service::ProjectRegistration::new(
        source.to_str().unwrap().into(),
        data.to_str().unwrap().into(),
        "fixture".into(),
    )?;
    registry.register(&registration)?;
    airk_store::service::data_scope::bind(&data, &control, namespace)?;
    let collection = Collection::initialize(
        &data,
        &registration.root,
        &registration.collection,
        Incarnation::from_bytes([122; 16]),
    )?;
    drop(collection.selected_store()?.lock()?);
    drop(registry);
    let mut registry = crate::catalog_test_fixture::authority_one(&control, namespace)?;
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Projects,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 32,
        max_bytes: 48 * 1024,
    })?;
    let project = page
        .items
        .iter()
        .find_map(|item| {
            if let CatalogItem::Project(project) = item {
                Some(project.id)
            } else {
                None
            }
        })
        .context("migrated project missing")?;
    let page = registry.catalog_page(&CatalogPageRequest {
        kind: CatalogKind::Indexes,
        index_id: None,
        project_id: None,
        after: None,
        expected_catalog_revision: None,
        limit: 32,
        max_bytes: 48 * 1024,
    })?;
    let index = page
        .items
        .iter()
        .find_map(|item| {
            if let CatalogItem::Index(index) = item {
                Some(index.id)
            } else {
                None
            }
        })
        .context("migrated index missing")?;
    Ok((
        owner,
        Session {
            transport: NativeMetadata {
                registry,
                methods: Vec::new(),
                refuse_apply: false,
            },
            control,
            operation: Incarnation::from_bytes([123; 16]),
            step: 0,
            receipts: Vec::new(),
            journals: Vec::new(),
            authorized_directories: Vec::new(),
        },
        project,
        index,
        source,
    ))
}

fn options(project: QueryProjectId) -> Options {
    Options {
        client: Client::Claude,
        install_dir: None,
        control_dir: None,
        context_directory: ".".into(),
        project_ids: vec![project],
        index_ids: Vec::new(),
        create_root: None,
        data_root: None,
        name: None,
        watch: false,
        yes: true,
        connection_id: None,
    }
}

#[tokio::test]
async fn standalone_scope_review_uses_real_native_grant_builder_and_persisted_receipt() -> Result<()>
{
    let (_owner, mut session, project, index, source) = fixture()?;
    let before = std::fs::read(source.join("original.txt"))?;
    let mut prompt = Prompt::default();
    let selected = prepare_scope(
        &mut session,
        &options(project),
        Sha256Digest::from_bytes([124; 32]),
        Path::new("unused"),
        &mut prompt,
    )
    .await?;
    let HttpSelection::Combined { selection } = selected else {
        bail!("combined scope required")
    };
    assert_eq!(selection.project_ids, vec![project]);
    assert!(selection.index_ids.is_none());
    let detail = session.transport.registry.catalog_detail(
        &CatalogEntity::Grant {
            id: selection.grant_id,
        },
        Some(selection.expected_catalog_revision),
    )?;
    let Some(CatalogItem::Grant(grant)) = detail.item else {
        bail!("native grant was not persisted")
    };
    assert!(grant.grant.memberships.iter().all(
        |member| member.membership.index_id == index && member.membership.project_id == project
    ));
    // Human scope review quotes/escapes paths, including Windows separators.
    // Assert the complete source, exact prefix and scope kind rather than a
    // raw-path substring that cannot occur in the escaped rendering.
    assert!(prompt.messages.contains(&format!(
        "    {:?} / {:?} ({:?})",
        source.to_str().unwrap(),
        "",
        PathScope::Subtree
    )));
    assert_eq!(
        sole_authorized_directory(&session.authorized_directories),
        Some(source.clone())
    );
    assert_eq!(
        session
            .transport
            .methods
            .iter()
            .filter(|method| matches!(method, ControlMethod::CatalogApply { .. }))
            .count(),
        1
    );
    assert_eq!(session.journals.len(), 1);
    let record: Value = serde_json::from_slice(&std::fs::read(&session.journals[0])?)?;
    assert_eq!(record["response"], session.receipts[0]);
    assert_eq!(std::fs::read(source.join("original.txt"))?, before);
    Ok(())
}

#[tokio::test]
async fn standalone_unknown_apply_is_retained_once_without_replay() -> Result<()> {
    let (_owner, mut session, project, _index, _source) = fixture()?;
    session.transport.refuse_apply = true;
    let error = prepare_scope(
        &mut session,
        &options(project),
        Sha256Digest::from_bytes([124; 32]),
        Path::new("unused"),
        &mut Prompt::default(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("fixture_outcome_unknown"));
    assert_eq!(
        session
            .transport
            .methods
            .iter()
            .filter(|method| matches!(method, ControlMethod::CatalogApply { .. }))
            .count(),
        1
    );
    let record: Value = serde_json::from_slice(&std::fs::read(&session.journals[0])?)?;
    assert_eq!(record["response"]["status"], "catalog_outcome_unknown");
    assert_eq!(record["response"]["code"], "fixture_outcome_unknown");
    assert!(record["method"]["catalog_apply"]["request"]["key"]["nonce"].is_string());
    Ok(())
}

#[tokio::test]
async fn standalone_declined_scope_never_submits_native_mutation() -> Result<()> {
    let (_owner, mut session, project, _index, _source) = fixture()?;
    let mut options = options(project);
    options.yes = false;
    let mut prompt = Prompt {
        answers: ["n".into()].into(),
        messages: Vec::new(),
    };
    assert!(
        prepare_scope(
            &mut session,
            &options,
            Sha256Digest::from_bytes([124; 32]),
            Path::new("unused"),
            &mut prompt
        )
        .await
        .is_err()
    );
    assert!(!session.transport.methods.iter().any(|method| matches!(
        method,
        ControlMethod::CatalogApply { .. } | ControlMethod::CatalogIndexSetupApply { .. }
    )));
    assert!(session.journals.is_empty());
    Ok(())
}

#[test]
fn standalone_explicit_scope_choice_rejects_empty_duplicate_mixed_and_unseen_entries() -> Result<()>
{
    let (_owner, mut session, _project, _index, _source) = fixture()?;
    let projects = session
        .transport
        .registry
        .catalog_page(&CatalogPageRequest {
            kind: CatalogKind::Projects,
            index_id: None,
            project_id: None,
            after: None,
            expected_catalog_revision: None,
            limit: 32,
            max_bytes: 48 * 1024,
        })?
        .items;
    let indexes = session
        .transport
        .registry
        .catalog_page(&CatalogPageRequest {
            kind: CatalogKind::Indexes,
            index_id: None,
            project_id: None,
            after: None,
            expected_catalog_revision: None,
            limit: 32,
            max_bytes: 48 * 1024,
        })?
        .items;
    assert_eq!(parse_choice("p1", &projects, &indexes)?.0.len(), 1);
    assert_eq!(parse_choice("i1", &projects, &indexes)?.1.len(), 1);
    for invalid in ["", "p0", "p2", "i2", "p1,p1", "p1,i1", "everything", "界1"] {
        assert!(
            parse_choice(invalid, &projects, &indexes).is_err(),
            "{invalid}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn standalone_enrollment_commit_then_transport_failure_keeps_exact_recoverable_identity()
-> Result<()> {
    struct CommitThenFailure {
        calls: usize,
        committed: Option<(Incarnation, HttpSelection)>,
    }
    impl EnrollmentTransport for CommitThenFailure {
        async fn enroll(
            &mut self,
            _control: &Path,
            selection: HttpSelection,
            id: Incarnation,
            _client: &str,
            _explorer_descriptor: Option<&Path>,
        ) -> Result<Value> {
            self.calls += 1;
            self.committed = Some((id, selection));
            Err(
                anyhow::anyhow!("fictional response lost after native enrollment commit")
                    .context("original transport failure"),
            )
        }
    }
    let (_owner, mut session, project, _index, _source) = fixture()?;
    let selection = prepare_scope(
        &mut session,
        &options(project),
        Sha256Digest::from_bytes([124; 32]),
        Path::new("unused"),
        &mut Prompt::default(),
    )
    .await?;
    let id = Incarnation::from_bytes([125; 16]);
    let mut transport = CommitThenFailure {
        calls: 0,
        committed: None,
    };
    let error = enroll_recorded(
        &session.control,
        session.operation,
        selection.clone(),
        id,
        "claude",
        None,
        &mut transport,
    )
    .await
    .unwrap_err();
    assert_eq!(transport.calls, 1);
    assert_eq!(transport.committed, Some((id, selection.clone())));
    let failure = format!("{error:#}");
    assert!(
        failure.contains(&id.to_string())
            && failure.contains("--existing-connection-id")
            && failure.contains("no enrollment was retried")
    );
    assert!(
        failure.contains("original transport failure")
            && failure.contains("fictional response lost after native enrollment commit")
    );
    let path = session.control.join(format!(
        "standalone-enrollment-{id}-{}.json",
        session.operation
    ));
    let record: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
    assert_eq!(record["connection_id"], id.to_string());
    assert_eq!(record["selection"], serde_json::to_value(&selection)?);
    assert_eq!(record["client"], "claude");
    assert!(record["response"].is_null());
    assert!(
        record["uncertain_error"]
            .as_str()
            .unwrap()
            .contains("original transport failure")
    );
    let original = std::fs::read(&path)?;
    let second_operation = Incarnation::from_bytes([126; 16]);
    let second_error = enroll_recorded(
        &session.control,
        second_operation,
        selection.clone(),
        id,
        "claude",
        None,
        &mut transport,
    )
    .await
    .unwrap_err();
    assert_eq!(transport.calls, 2, "each explicit invocation submits once");
    assert_eq!(transport.committed, Some((id, selection.clone())));
    assert!(format!("{second_error:#}").contains(&id.to_string()));
    assert_eq!(
        std::fs::read(&path)?,
        original,
        "prior intent and outcome remain unchanged"
    );
    let second_path = session.control.join(format!(
        "standalone-enrollment-{id}-{second_operation}.json"
    ));
    let second_record: Value = serde_json::from_slice(&std::fs::read(second_path)?)?;
    assert_eq!(second_record["connection_id"], id.to_string());
    assert_eq!(
        second_record["selection"],
        serde_json::to_value(&selection)?
    );
    assert_eq!(second_record["operation"], second_operation.to_string());
    Ok(())
}

#[test]
fn standalone_launch_directory_retains_exact_subtree_and_rejects_ambiguous_scope() {
    let atom = ScopeAtom {
        source_root_id: airk_core::index_catalog::SourceRootId::from_identity(
            Incarnation::from_bytes([99; 16]),
        ),
        kind: PathScope::Subtree,
        path: "allowed".into(),
    };
    let path = authorized_directory(Path::new("source"), &atom);
    assert_eq!(path, Some(PathBuf::from("source").join("allowed")));
    assert_eq!(
        sole_authorized_directory(&[path.clone(), path.clone()]),
        path
    );
    assert_eq!(
        sole_authorized_directory(&[path.clone(), Some(PathBuf::from("other"))]),
        None
    );
    assert_eq!(sole_authorized_directory(&[path, None]), None);
    assert_eq!(sole_authorized_directory(&[]), None);
    let mut exact = atom;
    exact.kind = PathScope::Exact;
    assert_eq!(authorized_directory(Path::new("source"), &exact), None);
}

#[test]
fn standalone_uninstall_requires_explicit_native_completion() -> Result<()> {
    require_completed_uninstall(
        &serde_json::json!({"uninstalled":true,"project_data_preserved":true}),
    )?;
    for receipt in [
        serde_json::json!({"uninstalled":false,"deferred_runtimes":["owned-runtime"],"resume":"runtime uninstall","project_data_preserved":true}),
        serde_json::json!({"uninstalled":false,"deferred_launcher":true,"resume":"runtime uninstall","project_data_preserved":true}),
        serde_json::json!({"project_data_preserved":true}),
    ] {
        assert!(
            require_completed_uninstall(&receipt)
                .unwrap_err()
                .to_string()
                .contains("preserve the installer")
        );
    }
    Ok(())
}
