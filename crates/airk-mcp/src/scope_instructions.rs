// SPDX-License-Identifier: Apache-2.0
//! Recipes derived from the catalog actually served by this combined frontend.
use airk_core::combined_reads::ResolvedSelection;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};

pub fn generate(selected: &ResolvedSelection, metadata: &Value) -> Result<Value> {
    selected.validate()?;
    let catalog = crate::combined::public_catalog();
    let discovery = catalog
        .iter()
        .find(|t| t.name == "query_scope")
        .context("scope discovery unavailable")?;
    let search = catalog
        .iter()
        .find(|t| t.name == "search")
        .context("search unavailable")?;
    let properties = &search.input_schema["properties"];
    for key in [
        "project_ids",
        "index_ids",
        "query",
        "mode",
        "limit",
        "max_bytes",
        "member_availability",
        "verification",
    ] {
        ensure!(
            properties.get(key).is_some(),
            "catalog does not support {key}"
        );
    }
    ensure!(
        discovery.input_schema["properties"]["view"]["enum"]
            .as_array()
            .is_some_and(|v| v.contains(&json!("projects"))),
        "discovery view unavailable"
    );
    let project_ids = &selected.project_ids;
    let index_ids = selected
        .members
        .iter()
        .map(|m| m.index_id)
        .collect::<Vec<_>>();
    let selected_indexes = metadata["available"]
        .as_array()
        .context("available indexes absent")?
        .iter()
        .filter(|row| row["selected"] == true)
        .map(|row| json!({"index_id":row["index_id"],"name":row["name"]}))
        .collect::<Vec<_>>();
    let arguments = json!({"project_ids":project_ids,"index_ids":index_ids,"query":"replace with your task terms","mode":"lexical","limit":12,"max_bytes":8192,"member_availability":selected.member_availability,"verification":"off"});
    let mut native = arguments.as_object().unwrap().clone();
    native.remove("project_ids");
    native.remove("index_ids");
    native.remove("member_availability");
    crate::combined::validate_native_arguments("search", &native)?;
    let examples = json!([
        {"name":discovery.name,"arguments":{"view":"projects"}},
        {"name":search.name,"arguments":arguments}
    ]);
    let catalog_digest = airk_core::identity::framed_digest(
        b"airk/scope-instruction-catalog/1",
        &[&serde_json::to_vec(&catalog)?],
    );
    let text = format!(
        "AIRK query scope instructions\nGenerated from the active installed catalog at {} Unix ms. Scope identity: {}. Catalog: {}.\nProjects (names, IDs and saved defaults): {}\nSelected source/prefix boundaries: {}\nOmitted indexes: {}\nConnection: active; pending scope: none. These instructions are not an access grant. Revalidate IDs and current permitted scope with query_scope before use; follow its bounded pages and valid continuations.\n\nActual tools/call examples:\n{}\n\nUse explicit project_ids AND index_ids for this subset. One combined request reads a shared index once. Index IDs only narrow the authorized Project union. Preserve originating index, exact publication, source hash, original location and valid handles; follow-up reads select that origin and its publication. Inspect member outcomes, readiness, truncation, shared total budgets, continuations and verification receipts. member_availability=require rejects unavailable members; verification=require checks returned saved sources. Neither proves whole-scope completeness. Use only typed retries and valid continuations. Zero results means no matches returned under these publications, filters, readiness and budgets. Broaden only by an explicit reported user choice within current grants; never select all, drop failed members or request broader access automatically.\n\nSaved defaults affect future selections after separate native review; they never widen an active grant or replace a restored session's explicit scope. Reuse, membership removal, index deletion and permanent denial are different actions. Permanent denial and split remain pending P9/P10.\n\nLauncher paths and client configuration are per-user/per-machine; use this machine's approved installed launcher and review configuration separately. Copying this block writes no configuration, AGENTS.md, CLAUDE.md or sources. Lifecycle start/stop/repair is an explicit reviewed management action. On an older catalog, use only its supported legacy single-index project binding and report combined selection unavailable; never send these new fields, reinterpret legacy bindings as Project IDs or synthesize merged provenance. If discovery fails, stop with unavailable guidance; do not broaden scope.\n",
        metadata["observed_unix_ms"],
        selected.identity()?,
        catalog_digest,
        json!({"projects":metadata["projects"],"selected_indexes":selected_indexes}),
        metadata["selected_scope"],
        metadata["omitted_index_ids"],
        serde_json::to_string_pretty(&examples)?
    );
    Ok(
        json!({"status":"available","catalog_digest":catalog_digest,"examples":examples,"text":text}),
    )
}
