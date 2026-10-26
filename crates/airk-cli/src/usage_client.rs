// SPDX-License-Identifier: Apache-2.0
//! Native reports travel directly to their project worker, in exact bounded pages.
use airk_core::service::ProjectBinding;
use anyhow::{Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

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
    ensure!((1..=30).contains(&days), "usage report days must be 1..30");
    collect(control, project, move |cursor| {
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
    collect(control, project, move |cursor| {
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
    collect(control, project, move |cursor| {
        airk_mcp::local::NativeRead::ValidateSettings {
            view,
            input: input.clone(),
            cursor,
            max_bytes: 48 << 10,
        }
    })
    .await
}

async fn collect(
    control: &Path,
    project: ProjectBinding,
    mut request: impl FnMut(Option<String>) -> airk_mcp::local::NativeRead,
) -> Result<Value> {
    let mut connection = crate::project_client::Connection::connect(control, project).await?;
    let mut cursor = None;
    let mut document = Value::Null;
    let mut expected_digest = None;
    let mut expected_count = None;
    let mut seen = BTreeSet::new();
    let mut bytes = 0usize;
    for _ in 0..256 {
        if !connection.reusable(std::time::Instant::now()) {
            connection = crate::project_client::Connection::connect(control, project).await?;
        }
        let value = connection.read_native(request(cursor)).await?;
        ensure!(
            value["isError"] != true && value.get("error").is_none(),
            "native project read failed: {value}"
        );
        let page: airk_kernel::usage_page::Page = serde_json::from_value(value)?;
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
            ensure!(
                document["project"] == serde_json::to_value(project)?
                    && document.get("value").is_some(),
                "crossed native report scope"
            );
            return Ok(document["value"].take());
        }
    }
    anyhow::bail!("usage report exceeds page budget")
}
