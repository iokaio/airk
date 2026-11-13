// SPDX-License-Identifier: Apache-2.0
//! Native reports travel directly to their project worker, in exact bounded pages.
use airk_core::service::{ControlMethod, ControlResponse, ProjectBinding};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

// The native read result classifier moved to `native_read_result` (split
// step 2b) so supervisor tests use it below the frontend; the public paths stay.
pub use crate::native_read_result::{NativeReadFailure, require_native_success};

/// Administrative metadata transport. Credentials remain inside Runtime and
/// the complete canonical native value/error is preserved for native clients.
pub async fn administrative_read(
    control: &Path,
    project: airk_core::service::ProjectBinding,
    request: airk_core::native_reads::NativeRead,
    client: airk_core::usage::ClientFamily,
) -> Result<serde_json::Value> {
    let response = crate::console_client::request(
        control,
        ControlMethod::NativeRead {
            project,
            request: request.clone(),
            usage_client: client,
        },
    )
    .await?;
    match response {
        ControlResponse::NativeRead { result } => {
            crate::usage_client::require_native_success(result)
        }
        ControlResponse::Failure { code } if code == "native_administrative_legacy" => {
            let mut connection =
                crate::project_client::Connection::connect_as(control, project, client).await?;
            crate::usage_client::require_native_success(connection.read_native(request).await?)
        }
        response @ ControlResponse::Failure { .. } => Err(
            crate::usage_client::NativeReadFailure::from_response(serde_json::to_value(response)?)
                .into(),
        ),
        // Never retain an unexpected credential-plane response as error data.
        _ => anyhow::bail!("unexpected native administrative response"),
    }
}

#[derive(Clone, PartialEq, Eq)]
struct NativeScopeHeaders {
    observation: Option<airk_core::scoped_reads::ScopeObservation>,
    publication: Option<airk_core::evidence::RevisionEnvelope>,
}
fn observe_scope_headers(
    page: &airk_kernel::usage_page::Page,
    expected: &mut Option<NativeScopeHeaders>,
) -> Result<()> {
    ensure!(
        page.scope_observation.is_some() == page.scope_publication.is_some(),
        "incomplete native scope/publication headers"
    );
    if let Some(scope) = &page.scope_observation {
        scope.validate()?;
    }
    if let Some(publication) = &page.scope_publication {
        ensure!(
            publication.incarnation == publication.token.incarnation
                && publication.generation == publication.token.generation
                && publication.revision == publication.token.revision,
            "native publication envelope disagrees with its token"
        );
    }
    let current = NativeScopeHeaders {
        observation: page.scope_observation.clone(),
        publication: page.scope_publication.clone(),
    };
    if let Some(expected) = expected {
        ensure!(
            *expected == current,
            "native scope/publication changed during pagination"
        );
    } else {
        *expected = Some(current);
    }
    Ok(())
}
fn finish_native_document(
    mut document: Value,
    project: ProjectBinding,
    headers: &NativeScopeHeaders,
) -> Result<Value> {
    ensure!(
        document["project"] == serde_json::to_value(project)? && document.get("value").is_some(),
        "crossed native report scope"
    );
    let value = &document["value"];
    if let (Some(scope), Some(publication)) = (&headers.observation, &headers.publication) {
        ensure!(
            value.get("scope_observation") == Some(&serde_json::to_value(scope)?)
                && value.get("scope_publication") == Some(&serde_json::to_value(publication)?),
            "native reconstructed document lost its exact scope/publication"
        );
    } else {
        ensure!(
            value.get("scope_observation").is_none() && value.get("scope_publication").is_none(),
            "native scope metadata appeared without page headers"
        );
    }
    Ok(document["value"].take())
}

pub async fn report(control: &Path, project: ProjectBinding, days: u16) -> Result<Value> {
    render(
        control,
        project,
        days,
        airk_mcp::local::ReportFormat::Json,
        None,
    )
    .await
}
pub async fn render(
    control: &Path,
    project: ProjectBinding,
    days: u16,
    format: airk_mcp::local::ReportFormat,
    price: Option<airk_core::usage::PriceScenario>,
) -> Result<Value> {
    render_selected(control, project, days, format, price, None).await
}
pub async fn render_selected(
    control: &Path,
    project: ProjectBinding,
    days: u16,
    format: airk_mcp::local::ReportFormat,
    price: Option<airk_core::usage::PriceScenario>,
    selection: Option<&airk_core::service::ScopedConnectRequest>,
) -> Result<Value> {
    ensure!((1..=30).contains(&days), "usage report days must be 1..30");
    collect_selected(control, project, selection, move |cursor| {
        airk_mcp::local::NativeRead::UsageReport {
            days,
            cursor,
            max_bytes: 48 << 10,
            format,
            price: price.clone(),
        }
    })
    .await
}

pub async fn settings(
    control: &Path,
    project: ProjectBinding,
    view: airk_mcp::local::SettingsView,
) -> Result<Value> {
    settings_selected(control, project, view, None).await
}
pub async fn settings_selected(
    control: &Path,
    project: ProjectBinding,
    view: airk_mcp::local::SettingsView,
    selection: Option<&airk_core::service::ScopedConnectRequest>,
) -> Result<Value> {
    collect_selected(control, project, selection, move |cursor| {
        airk_mcp::local::NativeRead::Settings {
            view,
            cursor,
            max_bytes: 48 << 10,
        }
    })
    .await
}
pub async fn validate_settings(
    control: &Path,
    project: ProjectBinding,
    view: airk_mcp::local::SettingsView,
    input: airk_core::service_configuration::InputFile,
) -> Result<Value> {
    validate_settings_selected(control, project, view, input, None).await
}
pub async fn validate_settings_selected(
    control: &Path,
    project: ProjectBinding,
    view: airk_mcp::local::SettingsView,
    input: airk_core::service_configuration::InputFile,
    selection: Option<&airk_core::service::ScopedConnectRequest>,
) -> Result<Value> {
    collect_selected(control, project, selection, move |cursor| {
        airk_mcp::local::NativeRead::ValidateSettings {
            view,
            input: input.clone(),
            cursor,
            max_bytes: 48 << 10,
        }
    })
    .await
}

/// Send a file selector to the native owner before any client filesystem access.
pub async fn validate_settings_file_selected(
    control: &Path,
    project: ProjectBinding,
    view: airk_mcp::local::SettingsView,
    path: &Path,
    selection: Option<&airk_core::service::ScopedConnectRequest>,
) -> Result<Value> {
    let path = std::path::absolute(path)?;
    let path = path
        .to_str()
        .context("settings input path must be UTF-8")?
        .to_owned();
    collect_selected(control, project, selection, move |cursor| {
        airk_mcp::local::NativeRead::ValidateSettingsFile {
            view,
            path: path.clone(),
            cursor,
            max_bytes: 48 << 10,
        }
    })
    .await
}

pub async fn render_file_selected(
    control: &Path,
    project: ProjectBinding,
    days: u16,
    format: airk_mcp::local::ReportFormat,
    path: &Path,
    selection: Option<&airk_core::service::ScopedConnectRequest>,
) -> Result<Value> {
    let path = std::path::absolute(path)?;
    let path = path
        .to_str()
        .context("price scenario path must be UTF-8")?
        .to_owned();
    collect_selected(control, project, selection, move |cursor| {
        airk_mcp::local::NativeRead::UsageReportFile {
            days,
            price_scenario_path: path.clone(),
            cursor,
            max_bytes: 48 << 10,
            format,
        }
    })
    .await
}

async fn collect_selected(
    control: &Path,
    project: ProjectBinding,
    selection: Option<&airk_core::service::ScopedConnectRequest>,
    mut request: impl FnMut(Option<String>) -> airk_mcp::local::NativeRead,
) -> Result<Value> {
    let mut connection = if let Some(selection) = selection {
        Some(
            crate::project_client::Connection::connect_scoped(
                control,
                project,
                selection,
                airk_core::usage::ClientFamily::Cli,
            )
            .await?,
        )
    } else {
        None
    };
    let mut cursor = None;
    let mut document = Value::Null;
    let mut expected_digest = None;
    let mut expected_count = None;
    let mut seen = BTreeSet::new();
    let mut bytes = 0usize;
    let mut scope_headers = None;
    for _ in 0..256 {
        let context = airk_mcp::read_attempt::ReadAttemptContext::new(
            std::time::Instant::now() + std::time::Duration::from_secs(125),
            Default::default(),
        );
        let value = if let Some(selection) = selection {
            if connection
                .as_ref()
                .is_none_or(|connection| !connection.reusable(std::time::Instant::now()))
            {
                connection = Some(
                    crate::project_client::Connection::connect_scoped(
                        control,
                        project,
                        selection,
                        airk_core::usage::ClientFamily::Cli,
                    )
                    .await?,
                );
            }
            connection
                .as_mut()
                .context("selected native connection missing")?
                .read_native_with_context(request(cursor), context)
                .await?
        } else {
            administrative_read(
                control,
                project,
                request(cursor),
                airk_core::usage::ClientFamily::Cli,
            )
            .await?
        };
        let value = require_native_success(value)?;
        let page: airk_kernel::usage_page::Page = serde_json::from_value(value)?;
        observe_scope_headers(&page, &mut scope_headers)?;
        ensure!(
            expected_digest
                .as_ref()
                .is_none_or(|digest| digest == &page.digest)
                && expected_count.is_none_or(|count| count == page.total_items),
            "usage report changed during pagination"
        );
        expected_digest = Some(page.digest.clone());
        expected_count = Some(page.total_items);
        ensure!(
            page.total_items <= 4096 && !page.items.is_empty(),
            "usage page exceeds item budget"
        );
        let mut record_bytes = 0;
        for record in page.items {
            ensure!(
                seen.insert(serde_json::to_string(&record.path)?),
                "duplicate usage page record"
            );
            record_bytes += serde_json::to_vec(&record)?.len();
            airk_kernel::usage_page::assemble(&mut document, record)?;
        }
        ensure!(
            record_bytes == page.record_bytes,
            "usage page byte count differs"
        );
        bytes += record_bytes;
        ensure!(
            bytes <= 8 << 20,
            "usage report reconstruction exceeds byte budget"
        );
        cursor = page.cursor;
        if cursor.is_none() {
            ensure!(
                seen.len() == page.total_items
                    && format!("{:x}", Sha256::digest(serde_json::to_vec(&document)?))
                        == page.digest,
                "usage report reconstruction does not match exact snapshot"
            );
            return finish_native_document(
                document,
                project,
                scope_headers
                    .as_ref()
                    .context("native page headers missing")?,
            );
        }
    }
    anyhow::bail!("usage report exceeds page budget")
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        evidence::RevisionEnvelope,
        identity::{Incarnation, RevisionToken, Sha256Digest},
        index_catalog::*,
        scoped_reads::*,
    };

    fn headers() -> (ScopeObservation, RevisionEnvelope) {
        (
            ScopeObservation {
                schema: 1,
                qualifier: ScopeQualifier::SelectedSubset,
                fingerprint: Sha256Digest::from_bytes([1; 32]),
                provenance: Some(
                    airk_core::grant_provenance::GrantProvenanceKind::UserReviewedProjectGrant,
                ),
                grant: Some(ScopeGrantObservation {
                    id: GrantId::from_identity(Incarnation::from_bytes([2; 16])),
                    revision: CatalogRevision::INITIAL,
                }),
                selection: None,
                members: vec![ScopeMemberObservation {
                    index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
                    memberships: vec![ScopeMembershipObservation {
                        project_id: QueryProjectId::from_identity(Incarnation::from_bytes([4; 16])),
                        membership_revision: CatalogRevision::INITIAL,
                    }],
                    source_root_ids: vec![SourceRootId::from_identity(Incarnation::from_bytes(
                        [5; 16],
                    ))],
                    source_revision: CatalogRevision::INITIAL,
                    policy_epoch: 0,
                    policy_digest: None,
                }],
            },
            RevisionEnvelope::new(
                "fixture".into(),
                RevisionToken {
                    incarnation: Incarnation::from_bytes([6; 16]),
                    generation: 1,
                    revision: Sha256Digest::from_bytes([7; 32]),
                },
            ),
        )
    }
    #[test]
    fn collector_rejects_changed_missing_or_unretained_scope_headers() -> Result<()> {
        let (scope, publication) = headers();
        let project = Sha256Digest::from_bytes([8; 32]);
        let document = serde_json::json!({"project":project,"value":{"complete":"data","scope_observation":scope,"scope_publication":publication}});
        let mut page = airk_kernel::usage_page::scoped_page(
            document.clone(),
            None,
            48 << 10,
            scope.clone(),
            publication.clone(),
        )?;
        let mut expected = None;
        observe_scope_headers(&page, &mut expected)?;
        observe_scope_headers(&page, &mut expected)?;
        let retained =
            finish_native_document(document.clone(), project, expected.as_ref().unwrap())?;
        assert_eq!(retained, document["value"]);
        page.scope_observation.as_mut().unwrap().fingerprint = Sha256Digest::from_bytes([9; 32]);
        assert!(observe_scope_headers(&page, &mut expected).is_err());
        page.scope_observation = Some(scope);
        page.scope_publication.as_mut().unwrap().generation += 1;
        assert!(observe_scope_headers(&page, &mut None).is_err());
        page.scope_publication = None;
        assert!(observe_scope_headers(&page, &mut None).is_err());
        let mut missing = document.clone();
        missing["value"]
            .as_object_mut()
            .unwrap()
            .remove("scope_publication");
        assert!(finish_native_document(missing, project, expected.as_ref().unwrap()).is_err());
        let unscoped = NativeScopeHeaders {
            observation: None,
            publication: None,
        };
        assert!(finish_native_document(document, project, &unscoped).is_err());
        let plain = serde_json::json!({"project":project,"value":{"complete":"data"}});
        assert_eq!(
            finish_native_document(plain, project, &unscoped)?,
            serde_json::json!({"complete":"data"})
        );
        Ok(())
    }
}
