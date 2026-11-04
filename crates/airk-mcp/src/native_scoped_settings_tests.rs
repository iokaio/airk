// SPDX-License-Identifier: Apache-2.0
//! Exercise the actual NativeRead handler, including its canonical page bodies.
use super::*;
use airk_core::{
    grant_provenance::{GrantProvenance, native_imported_grant_id},
    identity::{Incarnation, Sha256Digest},
    index_catalog::*,
    scoped_reads::{ScopeErrorObservation, ScopeReadRefused, SourceDescriptor},
};
use airk_kernel::{scoped_reads::ScopedReadAuthority, usage_page};
use anyhow::Result;
use serde_json::{Value, json};

const SAFE: [SettingsView; 5] = [
    SettingsView::ExtractionPolicy,
    SettingsView::VectorSelection,
    SettingsView::VectorStatus,
    SettingsView::Roots,
    SettingsView::ScanLimits,
];

fn authority(kernel: &Kernel, imported: bool, prefix: &str) -> Result<ScopedReadAuthority> {
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let index = IndexId::from_identity(Incarnation::from_bytes([2; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([3; 16]));
    let backing = airk_core::service::ProjectRegistration::new(
        kernel.root().to_str().unwrap().into(),
        kernel.data_dir().to_str().unwrap().into(),
        kernel.collection().into(),
    )?
    .binding;
    let host = Sha256Digest::from_bytes([5; 32]);
    let provenance = if imported {
        GrantProvenance::ImportedLegacySnapshot {
            upgrade_id: Incarnation::from_bytes([8; 16]),
            plan_digest: Sha256Digest::from_bytes([9; 32]),
            legacy_binding: backing,
            configuration_digest: Sha256Digest::from_bytes([10; 32]),
        }
    } else {
        GrantProvenance::UserReviewedProjectGrant
    };
    let id = if imported {
        native_imported_grant_id(Incarnation::from_bytes([11; 16]), host, &provenance)?
    } else {
        GrantId::from_identity(Incarnation::from_bytes([4; 16]))
    };
    Ok(ScopedReadAuthority {
        schema: 1,
        backing,
        index_id: index,
        project_ids: vec![project],
        provenance,
        grant: ReviewedGrant {
            id,
            host,
            revision: CatalogRevision::INITIAL,
            revoked: false,
            memberships: vec![GrantedMembership {
                membership: Membership {
                    project_id: project,
                    index_id: index,
                    revision: CatalogRevision::INITIAL,
                    scope: vec![ScopeAtom {
                        source_root_id: source,
                        kind: PathScope::Subtree,
                        path: prefix.into(),
                    }],
                },
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        },
        sources: vec![SourceDescriptor {
            source_root_id: source,
            original_root: kernel.root().to_str().unwrap().into(),
        }],
    })
}

async fn settings(
    tools: &LocalTools,
    project: airk_core::service::ProjectBinding,
    view: SettingsView,
    scoped: bool,
) -> Result<(Value, usize)> {
    let mut cursor = None;
    let mut document = Value::Null;
    let mut pages = 0;
    let mut identity = None;
    loop {
        let wire = tools
            .read_native(
                NativeRead::Settings {
                    view,
                    cursor,
                    max_bytes: 20 << 10,
                },
                project,
            )
            .await?;
        assert!(serde_json::to_vec(&wire)?.len() <= 20 << 10);
        assert_eq!(wire.get("scope_observation").is_some(), scoped);
        let page: usage_page::Page = serde_json::from_value(wire)?;
        let current = (
            page.scope_observation.clone(),
            page.scope_publication.clone(),
        );
        if let Some(expected) = &identity {
            assert_eq!(expected, &current);
        }
        identity = Some(current);
        for record in page.items {
            usage_page::assemble(&mut document, record)?;
        }
        pages += 1;
        cursor = page.cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(document["project"], serde_json::to_value(project)?);
    if scoped {
        let (observation, publication) = identity.unwrap();
        assert_eq!(
            document["value"]["scope_observation"],
            serde_json::to_value(observation.unwrap())?
        );
        assert_eq!(
            document["value"]["scope_publication"],
            serde_json::to_value(publication.unwrap())?
        );
    }
    Ok((document["value"].take(), pages))
}

fn fixture() -> Result<(tempfile::TempDir, Kernel)> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("source");
    std::fs::create_dir_all(root.join("public"))?;
    std::fs::write(root.join("public/a.rs"), "pub struct Public;\n")?;
    let kernel = Kernel::open(&root, Some(temporary.path().join("data")))?;
    kernel.configure_vectors(false)?;
    // A real bounded policy produces multiple native pages, rather than a renderer mock.
    let adapter = Kernel::extraction_adapters()[0].profile.adapter_id.clone();
    let policy = serde_json::from_value(json!({"version":1,"rules":(0..45).map(|n|json!({
        "selector":{"kind":"exact","path":format!("unused/{n}/{}", "x".repeat(700))},
        "case_sensitive":true,"adapter_id":adapter})).collect::<Vec<_>>()}))?;
    kernel.configure_extraction_policy(policy)?;
    kernel.index()?;
    Ok((temporary, kernel))
}

#[tokio::test]
async fn full_reviewed_and_imported_native_settings_preserve_complete_canonical_bodies()
-> Result<()> {
    let (_temporary, kernel) = fixture()?;
    let mut owner = LocalTools::new(kernel.clone()).await?;
    for imported in [false, true] {
        let grant = authority(&kernel, imported, "")?;
        let binding = grant.backing;
        let mut scoped = owner.scoped_session(grant).await?;
        for view in SAFE {
            let (expected, _) = settings(&owner, binding, view, false).await?;
            let (mut actual, pages) = settings(&scoped, binding, view, true).await?;
            actual.as_object_mut().unwrap().remove("scope_observation");
            actual.as_object_mut().unwrap().remove("scope_publication");
            assert_eq!(actual, expected);
            if matches!(view, SettingsView::ExtractionPolicy) {
                assert!(pages > 1);
            }
        }
        scoped.close().await?;
    }
    owner.close().await?;
    Ok(())
}

#[tokio::test]
async fn native_settings_refuse_prefix_pending_sources_revocation_and_arbitrary_inputs()
-> Result<()> {
    let (temporary, kernel) = fixture()?;
    let mut owner = LocalTools::new(kernel.clone()).await?;
    let full = authority(&kernel, true, "")?;
    let binding = full.backing;
    let mut prefix = owner
        .scoped_session(authority(&kernel, false, "public")?)
        .await?;
    for view in SAFE {
        let error = prefix
            .read_native(
                NativeRead::Settings {
                    view,
                    cursor: None,
                    max_bytes: 48 << 10,
                },
                binding,
            )
            .await
            .unwrap_err();
        assert!(error.is::<ScopeReadRefused>() && error.is::<ScopeErrorObservation>());
    }
    prefix.close().await?;
    let mut scoped = owner.scoped_session(full).await?;
    let native = kernel.with_scoped_reads(authority(&kernel, true, "")?)?;
    for request in [
        NativeRead::ValidateSettingsFile {
            view: SettingsView::ScanLimits,
            path: temporary
                .path()
                .join("never-open-this")
                .to_str()
                .unwrap()
                .into(),
            cursor: None,
            max_bytes: 48 << 10,
        },
        NativeRead::UsageReportFile {
            days: 1,
            price_scenario_path: temporary
                .path()
                .join("never-open-this-price")
                .to_str()
                .unwrap()
                .into(),
            cursor: None,
            max_bytes: 48 << 10,
            format: ReportFormat::Json,
        },
    ] {
        assert!(
            native
                .prepare_native_read(request.clone())
                .err()
                .unwrap()
                .is::<ScopeReadRefused>()
        );
        let error = scoped.read_native(request, binding).await.unwrap_err();
        assert!(error.is::<ScopeReadRefused>() && error.is::<ScopeErrorObservation>());
        assert!(!error.to_string().contains("never-open-this"));
    }
    assert!(
        scoped
            .read_native(
                NativeRead::Settings {
                    view: SettingsView::Roots,
                    cursor: None,
                    max_bytes: 48 << 10
                },
                Sha256Digest::from_bytes([99; 32])
            )
            .await
            .unwrap_err()
            .is::<ScopeReadRefused>()
    );
    for view in [SettingsView::QuotaPolicy, SettingsView::QuotaStatus] {
        let error = scoped
            .read_native(
                NativeRead::Settings {
                    view,
                    cursor: None,
                    max_bytes: 48 << 10,
                },
                binding,
            )
            .await
            .unwrap_err();
        assert!(error.is::<ScopeReadRefused>() && error.is::<ScopeErrorObservation>());
    }
    for view in SAFE {
        let error = scoped
            .read_native(
                NativeRead::ValidateSettings {
                    view,
                    input: airk_core::service_configuration::InputFile {
                        path: temporary
                            .path()
                            .join("never-open-this")
                            .to_str()
                            .unwrap()
                            .into(),
                        sha256: Sha256Digest::from_bytes([7; 32]),
                    },
                    cursor: None,
                    max_bytes: 48 << 10,
                },
                binding,
            )
            .await
            .unwrap_err();
        assert!(error.is::<ScopeReadRefused>() && error.is::<ScopeErrorObservation>());
    }
    for request in [
        NativeRead::UsagePolicy,
        NativeRead::UsageReport {
            days: 1,
            cursor: None,
            max_bytes: 48 << 10,
            format: ReportFormat::Json,
            price: None,
        },
    ] {
        assert!(
            scoped
                .read_native(request, binding)
                .await
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
    }
    let external = temporary.path().join("ungranted");
    std::fs::create_dir(&external)?;
    kernel.add_root("pending", &external)?;
    // Coherence uses native configuration, never a scan/stat of the newly selected source.
    std::fs::remove_dir(&external)?;
    for view in SAFE {
        assert!(
            scoped
                .read_native(
                    NativeRead::Settings {
                        view,
                        cursor: None,
                        max_bytes: 48 << 10
                    },
                    binding
                )
                .await
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
    }
    scoped.revoke_scoped_reads();
    assert!(
        scoped
            .read_native(
                NativeRead::Settings {
                    view: SettingsView::Roots,
                    cursor: None,
                    max_bytes: 48 << 10
                },
                binding
            )
            .await
            .unwrap_err()
            .is::<ScopeReadRefused>()
    );
    scoped.close().await?;
    owner.close().await?;
    Ok(())
}

#[tokio::test]
async fn native_file_intents_share_original_pinned_validation_and_report_bodies() -> Result<()> {
    let (temporary, kernel) = fixture()?;
    let binding = authority(&kernel, false, "")?.backing;
    let mut owner = LocalTools::new(kernel.clone()).await?;
    let path = temporary.path().join("scan-limits.json");
    std::fs::write(&path, serde_json::to_vec(&kernel.scan_limits()?)?)?;
    let input = airk_kernel::service_configuration::prepare_input(&path, 131_072)?;
    let original = owner
        .read_native(
            NativeRead::ValidateSettings {
                view: SettingsView::ScanLimits,
                input,
                cursor: None,
                max_bytes: 48 << 10,
            },
            binding,
        )
        .await?;
    let intent = owner
        .read_native(
            NativeRead::ValidateSettingsFile {
                view: SettingsView::ScanLimits,
                path: path.to_str().unwrap().into(),
                cursor: None,
                max_bytes: 48 << 10,
            },
            binding,
        )
        .await?;
    assert_eq!(intent, original);
    let path = temporary.path().join("price.json");
    let price: airk_core::usage::PriceScenario = serde_json::from_value(
        json!({"model":"fixture","currency":"USD","rate_date":"2026-11-03",
        "input_price_per_million":"1","output_price_per_million":null,"cached_input_treatment":"unknown",
        "cached_input_price_per_million":null,"context_assumes_uncached":false}),
    )?;
    price.validate()?;
    std::fs::write(&path, serde_json::to_vec(&price)?)?;
    for format in [
        ReportFormat::Json,
        ReportFormat::Csv,
        ReportFormat::Markdown,
    ] {
        let original = owner
            .read_native(
                NativeRead::UsageReport {
                    days: 1,
                    cursor: None,
                    max_bytes: 48 << 10,
                    format,
                    price: Some(price.clone()),
                },
                binding,
            )
            .await?;
        let intent = owner
            .read_native(
                NativeRead::UsageReportFile {
                    days: 1,
                    price_scenario_path: path.to_str().unwrap().into(),
                    cursor: None,
                    max_bytes: 48 << 10,
                    format,
                },
                binding,
            )
            .await?;
        assert_eq!(intent, original);
    }
    owner.close().await?;
    Ok(())
}

#[tokio::test]
async fn native_settings_continuations_bind_authority_publication_and_all_published_roots()
-> Result<()> {
    let (temporary, kernel) = fixture()?;
    let mut owner = LocalTools::new(kernel.clone()).await?;
    let grant = authority(&kernel, false, "")?;
    let binding = grant.backing;
    let mut first = owner.scoped_session(grant.clone()).await?;
    let page = first
        .read_native(
            NativeRead::Settings {
                view: SettingsView::ExtractionPolicy,
                cursor: None,
                max_bytes: 20 << 10,
            },
            binding,
        )
        .await?;
    let cursor = page["cursor"].as_str().unwrap().to_owned();
    let mut changed = grant.clone();
    changed.grant.id = GrantId::from_identity(Incarnation::from_bytes([12; 16]));
    let mut second = owner.scoped_session(changed).await?;
    let error = second
        .read_native(
            NativeRead::Settings {
                view: SettingsView::ExtractionPolicy,
                cursor: Some(cursor.clone()),
                max_bytes: 20 << 10,
            },
            binding,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed") && error.is::<ScopeErrorObservation>());
    std::fs::write(
        kernel.root().join("public/a.rs"),
        "pub struct ChangedPublication;\n",
    )?;
    kernel.index()?;
    let error = first
        .read_native(
            NativeRead::Settings {
                view: SettingsView::ExtractionPolicy,
                cursor: Some(cursor.clone()),
                max_bytes: 20 << 10,
            },
            binding,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("changed") && error.is::<ScopeErrorObservation>());
    let external = temporary.path().join("external");
    std::fs::create_dir(&external)?;
    std::fs::write(external.join("b.rs"), "pub struct External;\n")?;
    kernel.add_root("docs", &external)?;
    kernel.index()?;
    for view in SAFE {
        assert!(
            first
                .read_native(
                    NativeRead::Settings {
                        view,
                        cursor: None,
                        max_bytes: 48 << 10
                    },
                    binding
                )
                .await
                .unwrap_err()
                .is::<ScopeReadRefused>()
        );
    }
    let external = std::fs::canonicalize(external)?;
    let mut complete = grant;
    let source = SourceRootId::from_identity(Incarnation::from_bytes([13; 16]));
    complete.sources.push(SourceDescriptor {
        source_root_id: source,
        original_root: external.to_str().unwrap().into(),
    });
    complete.grant.memberships[0]
        .membership
        .scope
        .push(ScopeAtom {
            source_root_id: source,
            kind: PathScope::Subtree,
            path: String::new(),
        });
    let mut complete = owner.scoped_session(complete).await?;
    std::fs::remove_file(external.join("b.rs"))?;
    std::fs::remove_dir(&external)?;
    // Selected-index metadata observes the retained publication/configuration;
    // it never probes source availability merely to render settings.
    for view in SAFE {
        settings(&complete, binding, view, true).await?;
    }
    assert!(
        complete
            .read_native(
                NativeRead::Settings {
                    view: SettingsView::ExtractionPolicy,
                    cursor: Some(cursor),
                    max_bytes: 20 << 10
                },
                binding
            )
            .await
            .is_err()
    );
    complete.close().await?;
    second.close().await?;
    first.close().await?;
    owner.close().await?;
    Ok(())
}
