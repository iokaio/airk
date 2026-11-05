// SPDX-License-Identifier: Apache-2.0
//! One bounded plan across a frozen native combined selection. Reads remain
//! callbacks to the owning adapter; the kernel never opens another collection.
use super::*;
use airk_core::{
    combined_reads::QuerySelection,
    identity::Sha256Digest,
    index_catalog::{IndexId, QueryProjectId, unique_bounded},
    verification::{VerificationControls, VerificationScope},
};
use std::future::Future;

const SELECTORS: &[&str] = &["project_ids", "index_ids", "member_availability"];

/// Catalog for combined sessions. Legacy sessions retain their closed input
/// catalog and cannot silently acquire a broader authority.
pub fn operation_catalog() -> Value {
    let mut catalog = super::operation_catalog();
    for (name, operation) in catalog["operations"].as_object_mut().unwrap().iter_mut() {
        if name == "query_scope" {
            continue;
        }
        let properties = operation["properties"].as_object_mut().unwrap();
        for (key, pattern) in [
            ("project_ids", "^prj_[0-9a-f]{32}$"),
            ("index_ids", "^idx_[0-9a-f]{32}$"),
        ] {
            properties.insert(key.into(), json!({"type":"array","minItems":1,"maxItems":32,"uniqueItems":true,"items":{"type":"string","pattern":pattern}}));
        }
        properties.insert(
            "member_availability".into(),
            json!({"type":"string","enum":["report","require"]}),
        );
        if properties.contains_key("view") && properties.contains_key("project_id") {
            // Grant discovery has no evidence publication to pin.
        } else {
            properties.insert("origin_publication".into(), json!({"type":"string","maxLength":256,"description":"Exact originating member publication; expires on any publication change."}));
        }
        let constraints = json!([
            {"if":{"required":["index_ids"]},"then":{"required":["project_ids"]}},
            {"if":{"required":["member_availability"]},"then":{"required":["project_ids"]}}
        ]);
        let object = operation.as_object_mut().unwrap();
        object
            .entry("allOf")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap()
            .extend(constraints.as_array().unwrap().iter().cloned());
    }
    catalog["scope"] = json!("explicit frozen Project/index selection");
    catalog
}

fn canonical(mut selection: QuerySelection) -> Result<QuerySelection> {
    selection.validate()?;
    selection.project_ids.sort();
    if let Some(indexes) = selection.index_ids.as_mut() {
        indexes.sort();
    }
    Ok(selection)
}

fn parse_combined_text(text: &str) -> Result<(Vec<Step>, Vec<String>)> {
    if !text.trim_start().starts_with('/') {
        return parse_text(text);
    }
    let words = tokens(text)?;
    let mut native = Vec::new();
    let mut controls = Map::new();
    let mut at = 0;
    while at < words.len() {
        let key = match words[at].as_str() {
            "--project-ids" => Some("project_ids"),
            "--index-ids" => Some("index_ids"),
            "--member-availability" => Some("member_availability"),
            "--origin-publication" => Some("origin_publication"),
            _ => None,
        };
        if let Some(key) = key {
            at += 1;
            let value = words
                .get(at)
                .context("combined command option requires a value")?;
            ensure!(
                !value.starts_with("--"),
                "combined command option requires a value"
            );
            let value = if key.ends_with("_ids") {
                serde_json::from_str(value)?
            } else {
                json!(value)
            };
            ensure!(
                controls.insert(key.into(), value).is_none(),
                "duplicate combined command option"
            );
        } else {
            native.push(words[at].clone());
        }
        at += 1;
    }
    if controls.is_empty() {
        return parse_text(text);
    }
    let text = native
        .into_iter()
        .enumerate()
        .map(|(index, word)| {
            if index == 0 {
                word
            } else {
                format!("\"{}\"", word.replace('\\', "\\\\").replace('"', "\\\""))
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let (mut steps, needs) = parse_text(&text)?;
    for step in &mut steps {
        step.args.extend(controls.clone());
    }
    Ok((steps, needs))
}

fn selected_args(args: &Map<String, Value>, inherited: &QuerySelection) -> Result<QuerySelection> {
    if !SELECTORS.iter().any(|key| args.contains_key(*key)) {
        return Ok(inherited.clone());
    }
    let value: Map<String, Value> = SELECTORS
        .iter()
        .filter_map(|key| args.get(*key).map(|value| ((*key).into(), value.clone())))
        .collect();
    let selection = canonical(serde_json::from_value(Value::Object(value))?)?;
    ensure!(
        selection
            .project_ids
            .iter()
            .all(|id| inherited.project_ids.contains(id)),
        "step widens selected Projects"
    );
    if let Some(allowed) = &inherited.index_ids {
        ensure!(
            selection
                .index_ids
                .as_ref()
                .is_some_and(|indexes| indexes.iter().all(|id| allowed.contains(id))),
            "step widens selected indexes"
        );
    }
    Ok(selection)
}

fn install_selection(args: &mut Map<String, Value>, selection: &QuerySelection) -> Result<()> {
    args.extend(
        serde_json::to_value(selection)?
            .as_object()
            .context("query selection object")?
            .clone(),
    );
    Ok(())
}

fn validate_combined_step(
    step: &Step,
    prefix: &str,
    bound: bool,
    inherited: &QuerySelection,
) -> Result<()> {
    if step.tool == "query_scope" {
        return validate_step(step, prefix, bound);
    }
    let selection = selected_args(&step.args, inherited)?;
    let mut native = step.clone();
    for key in SELECTORS {
        native.args.remove(*key);
    }
    if let Some(publication) = native.args.remove("origin_publication") {
        publication
            .as_str()
            .context("origin_publication requires an exact token")?
            .parse::<airk_core::identity::RevisionToken>()?;
        ensure!(
            selection
                .index_ids
                .as_ref()
                .is_some_and(|indexes| indexes.len() == 1),
            "origin_publication requires one originating index"
        );
    }
    validate_step(&native, prefix, bound)
}

fn needs_origin(tool: &str) -> bool {
    matches!(
        tool,
        "file_info" | "read_file" | "context" | "imports" | "outline" | "relationships" | "changes"
    )
}

fn origin_index(record: &Value) -> Result<IndexId> {
    serde_json::from_value(record["origin"]["index_id"].clone())
        .context("combined location has no originating index")
}

fn narrowed_origin(
    args: &mut Map<String, Value>,
    record: &Value,
    inherited: &QuerySelection,
) -> Result<()> {
    let index = origin_index(record)?;
    let mut selected = selected_args(args, inherited)?;
    ensure!(
        selected
            .index_ids
            .as_ref()
            .is_none_or(|indexes| indexes.contains(&index)),
        "bound origin is outside the selected index subset"
    );
    let projects: Vec<QueryProjectId> =
        serde_json::from_value(record["origin"]["project_ids"].clone())?;
    unique_bounded(&projects, 32)?;
    selected.project_ids.retain(|id| projects.contains(id));
    ensure!(
        !selected.project_ids.is_empty(),
        "bound origin is outside selected Project memberships"
    );
    selected.index_ids = Some(vec![index]);
    install_selection(args, &selected)?;
    if let Some(token) = record["origin"]["publication"]["token"].as_str() {
        token.parse::<airk_core::identity::RevisionToken>()?;
        args.insert("origin_publication".into(), json!(token));
    }
    Ok(())
}

fn expand_combined(
    step: &Step,
    outputs: &BTreeMap<String, Value>,
    cap: usize,
    inherited: &QuerySelection,
) -> Result<(Vec<Map<String, Value>>, bool)> {
    let Some(binding) = step.bind.first() else {
        return Ok((vec![step.args.clone()], false));
    };
    let source = outputs
        .get(&binding.from)
        .context("dependency unavailable")?;
    let records = locations(source)?;
    ensure!(!records.is_empty(), "dependency has no qualified locations");
    if binding.select == "unique_definition_path" {
        ensure!(
            !clipped(source),
            "definition enumeration is incomplete; narrow the name or scope"
        );
        let mut identities = BTreeMap::new();
        for record in &records {
            let path = record["citation"]["path"]
                .as_str()
                .context("definition path unavailable")?;
            identities
                .entry((origin_index(record)?, path.to_owned()))
                .or_insert(record);
        }
        if identities.len() != 1 {
            return Err(DefinitionChoice(
                identities
                    .keys()
                    .map(|(index, path)| format!("{index}:{path}"))
                    .collect(),
            )
            .into());
        }
        let record = *identities.values().next().unwrap();
        let mut args = step.args.clone();
        args.insert("path".into(), record["citation"]["path"].clone());
        narrowed_origin(&mut args, record, inherited)?;
        return Ok((vec![args], false));
    }
    let mut args_list = Vec::new();
    let mut seen = BTreeSet::new();
    for record in &records {
        let mut args = step.args.clone();
        narrowed_origin(&mut args, record, inherited)?;
        if binding.select == "selected_symbol_ids" {
            let id = record.pointer("/fact/code/symbol_id").or_else(|| record.pointer("/fact/symbol_id")).and_then(Value::as_str).context("dependency has no native symbol IDs; use an explicit compact:false lookup at the cited origin")?;
            args.insert("symbol".into(), json!(id));
        } else {
            for key in ["path", "start_line", "end_line"] {
                args.insert(key.into(), record["citation"][key].clone());
            }
            args.insert(
                "known_hash".into(),
                record["citation"]["content_hash"].clone(),
            );
        }
        if seen.insert(serde_json::to_string(&args)?) {
            args_list.push(args);
        }
    }
    let omitted = args_list.len() > cap;
    args_list.truncate(cap);
    Ok((args_list, omitted))
}

fn failed(value: &Value) -> bool {
    value["isError"] == true
        || value["is_error"] == true
        || value.get("error").is_some_and(|error| !error.is_null())
}

fn member_partial(value: &Value) -> bool {
    value["partial"] == true || value["structuredContent"]["partial"] == true
}

/// Execute each plan step once through the same combined native owner. An
/// internal final verification callback receives only the bounded projection;
/// it is not an exposed plan operator and cannot add query authority.
pub async fn execute<F, Fut>(
    request: Request,
    selection: QuerySelection,
    scope_identity: Sha256Digest,
    mut reader: F,
) -> Result<ResultEnvelope>
where
    F: FnMut(String, Map<String, Value>) -> Fut,
    Fut: Future<Output = Result<Value>>,
{
    ensure!(
        serde_json::to_vec(&request)?.len() <= 32768,
        "parse request exceeds 32 KiB"
    );
    ensure!(
        request.schema == REQUEST_SCHEMA,
        "unsupported parse request schema"
    );
    ensure!(
        usize::from(request.text.is_some())
            + usize::from(request.recipe.is_some())
            + usize::from(request.plan.is_some())
            == 1,
        "choose text, recipe or plan"
    );
    let selection = canonical(selection)?;
    let started = Instant::now();
    let mut needs = Vec::new();
    let mut plan = if let Some(plan) = request.plan {
        plan
    } else {
        let (steps, tier) = if let Some(recipe_request) = request.recipe {
            (recipe(&recipe_request)?, "recipe")
        } else {
            let (steps, missing) = parse_combined_text(request.text.as_deref().unwrap())?;
            needs = missing;
            (steps, "command")
        };
        Plan {
            schema: PLAN_SCHEMA.into(),
            steps,
            tier: tier.into(),
            path_prefix: request.path_prefix.clone(),
            ..Plan::default()
        }
    };
    ensure!(
        plan.schema.is_empty() || plan.schema == PLAN_SCHEMA,
        "unsupported plan schema"
    );
    ensure!(
        plan.collection.is_empty() || plan.collection == "selected_indexes",
        "plan belongs to a legacy collection"
    );
    ensure!(
        plan.catalog.is_empty() || plan.catalog == CATALOG,
        "plan catalog changed; prepare again"
    );
    ensure!(
        plan.publication.is_none(),
        "combined plans require member publications, never a scalar publication"
    );
    if let Some(previous) = &plan.query_scope {
        ensure!(
            serde_json::to_value(canonical(previous.clone())?)?
                == serde_json::to_value(&selection)?,
            "combined plan selection changed; prepare again"
        );
    }
    ensure!(
        plan.scope_identity
            .is_none_or(|identity| identity == scope_identity),
        "combined plan authority changed; prepare again"
    );
    let expected = request
        .expected_digest
        .or_else(|| (!plan.digest.is_empty()).then(|| plan.digest.clone()));
    if let Some(expected) = &expected {
        ensure!(
            *expected == digest(&plan)?,
            "plan digest changed; prepare the edited plan again"
        );
    }
    plan.schema = PLAN_SCHEMA.into();
    plan.collection = "selected_indexes".into();
    plan.catalog = CATALOG.into();
    plan.query_scope = Some(selection.clone());
    plan.scope_identity = Some(scope_identity);
    ensure!(
        safe_path(&plan.path_prefix)
            && safe_path(&request.path_prefix)
            && plan.path_prefix.starts_with(&request.path_prefix),
        "plan widens requested path scope"
    );
    if let Some(budgets) = request.budgets {
        plan.budgets = budgets;
    }
    if let Some(output) = request.output {
        plan.output = output;
    }
    let b = &plan.budgets;
    ensure!(
        (1..=12).contains(&b.calls)
            && (1..=12).contains(&b.fanout)
            && (1..=30).contains(&b.seconds)
            && (1024..=524288).contains(&b.max_bytes),
        "plan budgets exceed limits"
    );
    ensure!(
        matches!(plan.output.view.as_str(), "raw" | "evidence")
            && (1024..=524288).contains(&plan.output.max_bytes),
        "invalid output selection"
    );
    ensure!(
        plan.steps.len() <= 6 && (!plan.steps.is_empty() || !needs.is_empty()),
        "plan needs one to six steps"
    );
    let global_explicit = request.verification.is_set() || plan.verification.is_set();
    plan.verification = plan.verification.with_overrides(request.verification);
    let own_step_policy = plan
        .steps
        .iter()
        .try_fold(false, |found, step| -> Result<bool> {
            Ok(found || verification_controls(&step.args)?.is_set())
        })?;
    let root_budget = plan.verification.resolve()?;
    let verification_scope = plan.verification_scope.unwrap_or(
        if own_step_policy && (!global_explicit || root_budget.is_none()) {
            VerificationScope::Steps
        } else {
            VerificationScope::Response
        },
    );
    plan.verification_scope = Some(verification_scope);
    plan.verification = VerificationControls::resolved(root_budget);
    let response_verification = if verification_scope == VerificationScope::Response
        && plan.steps.iter().any(|step| is_evidence(&step.tool))
    {
        root_budget
    } else {
        None
    };
    ensure!(
        response_verification.is_none() || !own_step_policy,
        "choose response verification or explicit verification_scope:steps for per-step choices"
    );
    ensure!(
        verification_scope != VerificationScope::Steps || plan.output.view == "raw",
        "per-step verification requires output.view:raw to retain complete receipts"
    );
    let mut ids: BTreeMap<String, String> = BTreeMap::new();
    for step in &mut plan.steps {
        ensure!(
            !step.id.is_empty()
                && step.id.len() <= 32
                && step
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-'),
            "invalid step ID"
        );
        ensure!(step.bind.len() <= 1, "one typed location binding per step");
        for binding in &step.bind {
            let source = ids
                .get(&binding.from)
                .context("binding must name an earlier step")?;
            ensure!(
                (binding.select == "unique_definition_path"
                    && binding.arg == "path"
                    && source == "symbols"
                    && matches!(
                        step.tool.as_str(),
                        "outline" | "file_info" | "symbols" | "code_facts"
                    ))
                    || (binding.select == "hit_locations"
                        && binding.arg == "location"
                        && step.tool == "read_file")
                    || (binding.select == "selected_symbol_ids"
                        && binding.arg == "symbol"
                        && matches!(source.as_str(), "symbols" | "code_facts")
                        && step.tool == "relationships"),
                "unsupported selector or destination"
            );
        }
        ensure!(
            ids.insert(step.id.clone(), step.tool.clone()).is_none(),
            "duplicate step ID"
        );
        if let Some(detail) = request.reply_detail
            && step.tool != "query_scope"
            && step.args.get("reply_detail").is_none_or(Value::is_null)
        {
            step.args.insert("reply_detail".into(), json!(detail));
        }
        let own_selection = selected_args(&step.args, &selection)?;
        if step.tool != "query_scope" {
            install_selection(&mut step.args, &own_selection)?;
        }
        let key = if step.tool == "list_files" {
            "prefix"
        } else {
            "path_prefix"
        };
        if fields(&step.tool).contains(&key) {
            step.args.entry(key).or_insert(json!(plan.path_prefix));
        }
        if verification_scope == VerificationScope::Steps
            && !verification_fields(&step.tool).is_empty()
        {
            let own = verification_controls(&step.args)?;
            let effective =
                if own.verification == Some(airk_core::verification::VerificationMode::Off) {
                    own.resolve()?
                } else {
                    plan.verification.with_overrides(own).resolve()?
                };
            for key in verification_fields(&step.tool) {
                step.args.remove(*key);
            }
            step.args.extend(
                serde_json::to_value(VerificationControls::resolved(effective))?
                    .as_object()
                    .unwrap()
                    .clone(),
            );
        }
        validate_combined_step(step, &plan.path_prefix, step.bind.is_empty(), &selection)?;
        if needs_origin(&step.tool)
            && step.bind.is_empty()
            && own_selection
                .index_ids
                .as_ref()
                .is_none_or(|indexes| indexes.len() != 1)
        {
            needs.push(format!(
                "{} requires one originating index in index_ids; select it explicitly.",
                step.id
            ));
        }
        if ["name", "symbol"].iter().any(|key| {
            step.args
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|id| {
                    ["syntax:", "scip:", "ctags:", "lsp:"]
                        .iter()
                        .any(|prefix| id.starts_with(prefix))
                })
        }) && !step.args.contains_key("revision")
            && !step.args.contains_key("origin_publication")
            && step.bind.is_empty()
        {
            needs.push(format!(
                "{} needs the original member revision for its literal symbol ID.",
                step.id
            ));
        }
    }
    if let Some(expected) = expected {
        ensure!(
            expected == digest(&plan)?,
            "prepared plan controls changed; prepare again"
        );
    }
    plan.digest = digest(&plan)?;
    let mut result = ResultEnvelope {
        query_scope: Some(selection.clone()),
        scope_identity: Some(scope_identity),
        member_verification: None,
        scope_observation: None,
        source_verification: None,
        schema: RESULT_SCHEMA.into(),
        reply_detail: request.reply_detail,
        collection: plan.collection.clone(),
        publication: None,
        status: if needs.is_empty() {
            "prepared"
        } else {
            "needs_input"
        }
        .into(),
        executed: false,
        plan: plan.clone(),
        requires_input: needs
            .into_iter()
            .map(|reason| RequiredInput {
                field: "index_ids".into(),
                step: None,
                reason,
                choices: vec![],
            })
            .collect(),
        steps: vec![],
        evidence: vec![],
        warnings: vec![
            "Combined member publications are separate observations, never one atomic publication."
                .into(),
        ],
        truncated: false,
        completeness: vec![],
        calls: 0,
        payload_bytes: 0,
    };
    if request.mode == Mode::Prepare || !result.requires_input.is_empty() {
        return bounded_result(result);
    }
    let mut outputs = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut fanout = 0;
    let read_calls = plan
        .budgets
        .calls
        .saturating_sub(usize::from(response_verification.is_some()));
    for step in &plan.steps {
        let (args_list, omitted) = match expand_combined(
            step,
            &outputs,
            plan.budgets.fanout.saturating_sub(fanout),
            &selection,
        ) {
            Ok(expanded) => expanded,
            Err(error) => {
                if let Some(choice) = error.downcast_ref::<DefinitionChoice>() {
                    result.requires_input.push(RequiredInput {
                        field: "index_ids/path".into(),
                        step: Some(step.id.clone()),
                        reason: error.to_string(),
                        choices: choice.0.clone(),
                    });
                }
                result.steps.push(StepReport {
                    id: step.id.clone(),
                    tool: step.tool.clone(),
                    arguments: json!(step.args),
                    status: "skipped".into(),
                    elapsed_ms: 0,
                    result: None,
                    error: Some(error.to_string()),
                });
                result.completeness.push(format!(
                    "{}: dependency needs explicit qualified selection",
                    step.id
                ));
                continue;
            }
        };
        if omitted {
            result.truncated = true;
            result
                .completeness
                .push(format!("{}: fanout omitted qualified locations", step.id));
        }
        if !step.bind.is_empty() {
            fanout += args_list.len();
        }
        let mut collected = Vec::new();
        for mut args in args_list {
            if result.calls >= read_calls
                || started.elapsed().as_secs() >= plan.budgets.seconds
                || result.payload_bytes + 1024 > plan.budgets.max_bytes
            {
                result.truncated = true;
                result
                    .completeness
                    .push(format!("{}: run budget exhausted", step.id));
                break;
            }
            let remaining = plan.budgets.max_bytes - result.payload_bytes;
            if fields(&step.tool).contains(&"max_bytes") {
                let cap = number(&args, "max_bytes", 8192)
                    .min(remaining.saturating_sub(512))
                    .min(65536);
                args.insert("max_bytes".into(), json!(cap));
            }
            if verification_scope == VerificationScope::Response
                && !verification_fields(&step.tool).is_empty()
            {
                args.insert("verification".into(), json!("off"));
            }
            validate_combined_step(
                &Step {
                    args: args.clone(),
                    ..step.clone()
                },
                &plan.path_prefix,
                true,
                &selection,
            )?;
            let at = Instant::now();
            result.calls += 1;
            result.executed = true;
            let public_args = args.clone();
            let remaining_ms = std::time::Duration::from_secs(plan.budgets.seconds)
                .saturating_sub(started.elapsed())
                .as_millis()
                .min(u128::from(u64::MAX)) as u64;
            args.insert("__airk_plan_remaining_ms".into(), json!(remaining_ms));
            let answer = reader(step.tool.clone(), args.clone()).await;
            let mut report = StepReport {
                id: step.id.clone(),
                tool: step.tool.clone(),
                arguments: json!(public_args),
                status: "complete".into(),
                elapsed_ms: at.elapsed().as_millis() as u64,
                result: None,
                error: None,
            };
            match answer {
                Err(error) => {
                    report.status = "failed".into();
                    report.error = Some("combined read failed; no automatic replay".into());
                    if let Some(receipt) = error.downcast_ref::<ReadFailure>()
                        && serde_json::to_vec(&receipt.0)?.len() <= remaining
                    {
                        report.result = Some(receipt.0.clone());
                    }
                    result
                        .completeness
                        .push(format!("{}: read failed", step.id));
                }
                Ok(value) => {
                    let bytes = serde_json::to_vec(&value)?.len();
                    if bytes > remaining {
                        report.status = "failed".into();
                        report.error =
                            Some("complete member receipt exceeds payload budget".into());
                        result.truncated = true;
                        result
                            .completeness
                            .push(format!("{}: payload budget exceeded", step.id));
                    } else {
                        let records = locations(&value)?;
                        let selected = selected_args(&args, &selection)?;
                        for record in &records {
                            let origin: crate::combined_reads::QualifiedOrigin =
                                serde_json::from_value(record["origin"].clone())
                                    .context("combined evidence requires a qualified origin")?;
                            ensure!(
                                selected
                                    .index_ids
                                    .as_ref()
                                    .is_none_or(|indexes| indexes.contains(&origin.index_id))
                                    && origin
                                        .project_ids
                                        .iter()
                                        .all(|project| selected.project_ids.contains(project)),
                                "read result escapes selected Project/index scope"
                            );
                            if let Some(publication) = args
                                .get("origin_publication")
                                .or_else(|| args.get("revision"))
                                .and_then(Value::as_str)
                            {
                                ensure!(
                                    origin.publication.token.to_string() == publication,
                                    "origin publication changed; bound evidence not admitted"
                                );
                            }
                        }
                        ensure!(
                            records.iter().all(|record| record["citation"]["path"]
                                .as_str()
                                .is_none_or(|path| path.starts_with(&plan.path_prefix))),
                            "read result escapes plan scope"
                        );
                        result.payload_bytes += bytes;
                        if failed(&value) {
                            report.status = "failed".into();
                            result
                                .completeness
                                .push(format!("{}: complete member failure retained", step.id));
                        } else if member_partial(&value) {
                            result.completeness.push(format!(
                                "{}: partial member availability; complete outcomes retained",
                                step.id
                            ));
                        }
                        if clipped(&value) {
                            result.truncated = true;
                            result
                                .completeness
                                .push(format!("{}: upstream truncation or continuation", step.id));
                        }
                        for mut record in records {
                            if seen.insert(
                                json!([record.get("origin"), record["citation"]]).to_string(),
                            ) {
                                record["step"] = json!(step.id);
                                result.evidence.push(record);
                            }
                        }
                        collected.push(value.clone());
                        report.result = Some(value);
                    }
                }
            }
            result.steps.push(report);
        }
        if !collected.is_empty() {
            outputs.insert(step.id.clone(), json!(collected));
        }
    }
    for step in &plan.steps {
        if !result.steps.iter().any(|report| report.id == step.id) {
            result.steps.push(StepReport {
                id: step.id.clone(),
                tool: step.tool.clone(),
                arguments: json!(step.args),
                status: "skipped".into(),
                elapsed_ms: 0,
                result: None,
                error: Some("Run stopped before this step was admitted.".into()),
            });
        }
    }
    result.status = if !result.requires_input.is_empty() {
        "needs_input"
    } else if result.truncated || !result.completeness.is_empty() {
        if result
            .steps
            .iter()
            .any(|report| report.status == "complete")
        {
            "partial"
        } else {
            "failed"
        }
    } else {
        "complete"
    }
    .into();
    // Final projection is fixed before the source-check callback. Complete raw
    // member outcomes remain required for per-step receipt semantics.
    if plan.output.view == "evidence" {
        for report in &mut result.steps {
            report.result = None;
        }
    }
    result = if response_verification.is_some() {
        fit_verification_frame(result, 4096)?
    } else {
        bounded_result(result)?
    };
    if let Some(budget) = response_verification {
        ensure!(
            result.calls < plan.budgets.calls,
            "response verification requires one remaining plan call"
        );
        let mut args = serde_json::to_value(VerificationControls::resolved(Some(budget)))?
            .as_object()
            .unwrap()
            .clone();
        let remaining_ms = std::time::Duration::from_secs(plan.budgets.seconds)
            .saturating_sub(started.elapsed())
            .as_millis()
            .min(u128::from(u64::MAX)) as u64;
        // Zero is an explicit exhausted allocation handled by the native owner,
        // never a fresh verification deadline after the plan deadline.
        args.insert(
            "remaining_deadline_ms".into(),
            json!(budget.deadline_ms.min(remaining_ms)),
        );
        let projection = serde_json::to_value(&result)?;
        let receipt_bytes_budget = result
            .plan
            .output
            .max_bytes
            .saturating_sub(serde_json::to_vec(&projection)?.len() + 2048)
            .min(airk_core::verification::receipt_budget_for_reply(
                &projection,
            )?);
        args.insert("receipt_bytes_budget".into(), json!(receipt_bytes_budget));
        args.insert("projection".into(), projection);
        install_selection(&mut args, &selection)?;
        result.calls += 1;
        let verification = reader("__airk_verify_projection".into(), args).await?;
        let failed = failed(&verification);
        let partial = member_partial(&verification);
        result.member_verification = Some(verification);
        if failed && budget.mode == airk_core::verification::VerificationMode::Require {
            result.status = "failed".into();
            result.completeness.push("Response source-verification requirements were not satisfied; complete native observation retained.".into());
        } else if failed {
            result.status = "partial".into();
            result.completeness.push("Response source-verification is unavailable; complete native failure retained without a verification claim.".into());
        } else if partial {
            result.status = "partial".into();
            result.completeness.push("Response source-verification observations are incomplete; complete member outcomes retained.".into());
        }
        ensure!(
            serde_json::to_vec(&result)?.len() <= plan.output.max_bytes,
            "complete combined verification receipt exceeds output budget"
        );
        airk_core::verification::validate_projection(&serde_json::to_value(&result)?)?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{
        combined_reads::MemberAvailability, identity::Incarnation, index_catalog::SourceRootId,
    };
    use std::{
        cell::RefCell,
        task::{Context as TaskContext, Poll, Waker},
    };
    fn run<T>(future: impl Future<Output = T>) -> T {
        let mut context = TaskContext::from_waker(Waker::noop());
        match std::pin::pin!(future).as_mut().poll(&mut context) {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("original bounded fixture must complete without scheduling"),
        }
    }
    fn project() -> QueryProjectId {
        QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]))
    }
    fn index(byte: u8) -> IndexId {
        IndexId::from_identity(Incarnation::from_bytes([byte; 16]))
    }
    fn selection() -> QuerySelection {
        QuerySelection {
            project_ids: vec![project()],
            index_ids: Some(vec![index(2), index(3)]),
            member_availability: MemberAvailability::Report,
        }
    }
    fn identity() -> Sha256Digest {
        Sha256Digest::from_bytes([4; 32])
    }
    fn fixture(byte: u8, path: &str) -> Value {
        let token = airk_core::identity::RevisionToken {
            incarnation: Incarnation::from_bytes([5; 16]),
            generation: 1,
            revision: Sha256Digest::from_bytes([6; 32]),
        };
        let publication = airk_core::evidence::RevisionEnvelope::new("fixture".into(), token);
        json!({"schema":"airk.combined-read/1","members":[{"index_id":index(byte),"result":{"structuredContent":{"hits":[{"citation":{"path":path,"content_hash":"ab".repeat(32),"start_line":1,"end_line":1,"location":{"Lines":{"start":1,"end":1}}},"text":"Queue"}]}}}],"ranking":[{"index_id":index(byte),"result_pointer":"/members/0/result/structuredContent/hits/0","origin":{"index_id":index(byte),"source_root_id":SourceRootId::from_identity(Incarnation::from_bytes([7;16])),"project_ids":[project()],"source_relative_path":path,"publication":publication}}]})
    }
    fn request(value: Value) -> Request {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn prepare_freezes_scope_and_never_dispatches_reads_or_verification() -> Result<()> {
        let prepared = run(execute(
            request(json!({"schema":REQUEST_SCHEMA,"text":"/search Queue"})),
            selection(),
            identity(),
            |_, _| async { panic!("preparation must be inert") },
        ))?;
        assert!(!prepared.executed && prepared.steps.is_empty());
        assert_eq!(prepared.plan.query_scope, Some(selection()));
        assert_eq!(prepared.plan.scope_identity, Some(identity()));
        let error = run(execute(
            request(json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":prepared.plan})),
            selection(),
            Sha256Digest::from_bytes([8; 32]),
            |_, _| async { panic!("changed authority must not dispatch") },
        ))
        .unwrap_err();
        assert!(error.to_string().contains("authority changed"));
        Ok(())
    }

    #[test]
    fn prepared_controls_cannot_change_through_outer_overrides() -> Result<()> {
        let prepared = run(execute(
            request(json!({"schema":REQUEST_SCHEMA,"text":"/search Queue"})),
            selection(),
            identity(),
            |_, _| async { panic!("prepare must be inert") },
        ))?;
        let error=run(execute(request(json!({"schema":REQUEST_SCHEMA,"mode":"execute","plan":prepared.plan,"budgets":{"calls":11}})),selection(),identity(),|_,_|async{panic!("changed prepared budget must not dispatch")})).unwrap_err();
        assert!(error.to_string().contains("controls changed"));
        Ok(())
    }

    #[test]
    fn slash_selection_uses_closed_json_arrays_and_rejects_duplicates() -> Result<()> {
        let command = format!(
            "/search 'Queue policy' --project-ids '[\"{}\"]' --index-ids '[\"{}\"]'",
            project(),
            index(2)
        );
        let prepared = run(execute(
            request(json!({"schema":REQUEST_SCHEMA,"text":command})),
            selection(),
            identity(),
            |_, _| async { panic!("prepare must be inert") },
        ))?;
        assert_eq!(prepared.plan.steps[0].args["query"], "Queue policy");
        assert_eq!(prepared.plan.steps[0].args["index_ids"], json!([index(2)]));
        let error = parse_combined_text(
            "/search Queue --member-availability report --member-availability require",
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicate"));
        Ok(())
    }

    #[test]
    fn discovery_keeps_its_exact_public_inputs_and_freezes_scope_in_the_plan() -> Result<()> {
        let prepared = run(execute(
            request(json!({"schema":REQUEST_SCHEMA,"text":"/query-scope projects"})),
            selection(),
            identity(),
            |_, _| async { panic!("prepare must be inert") },
        ))?;
        assert_eq!(
            prepared.plan.steps[0].args,
            json!({"view":"projects"}).as_object().unwrap().clone()
        );
        assert_eq!(prepared.plan.query_scope, Some(selection()));
        assert_eq!(
            operation_catalog()["operations"]["query_scope"],
            airk_core::combined_reads::query_scope::schema()
        );
        Ok(())
    }

    #[test]
    fn qualified_bound_read_keeps_its_own_index_project_and_publication() -> Result<()> {
        let calls = RefCell::new(Vec::new());
        let plan = json!({"steps":[{"id":"find","tool":"search","args":{"query":"Queue"}},{"id":"read","tool":"read_file","bind":[{"arg":"location","from":"find","select":"hit_locations"}]}]});
        let result = run(execute(
            request(
                json!({"schema":REQUEST_SCHEMA,"mode":"execute","verification":"off","plan":plan}),
            ),
            selection(),
            identity(),
            |tool, args| {
                calls.borrow_mut().push((tool.clone(), args.clone()));
                std::future::ready(Ok(fixture(2, "queue.rs")))
            },
        ))?;
        assert_eq!(result.status, "complete");
        assert_eq!(calls.borrow().len(), 2);
        let bound = &calls.borrow()[1].1;
        assert_eq!(bound["index_ids"], json!([index(2)]));
        assert_eq!(bound["project_ids"], json!([project()]));
        assert!(
            bound["origin_publication"]
                .as_str()
                .unwrap()
                .starts_with("airk-r1:")
        );
        assert_eq!(result.evidence[0]["origin"]["index_id"], json!(index(2)));
        Ok(())
    }

    #[test]
    fn same_path_in_independent_indexes_is_ambiguous_and_not_deduplicated() -> Result<()> {
        let calls = RefCell::new(0);
        let value = json!([fixture(2, "same.rs"), fixture(3, "same.rs")]);
        assert_eq!(locations(&value)?.len(), 2);
        let result = run(execute(
            request(
                json!({"schema":REQUEST_SCHEMA,"mode":"execute","verification":"off","recipe":{"id":"definition","parameters":{"name":"Queue"}}}),
            ),
            selection(),
            identity(),
            |tool, args| {
                *calls.borrow_mut() += 1;
                std::future::ready(Ok(if tool == "read_file" {
                    fixture(
                        if args["index_ids"][0] == json!(index(2)) {
                            2
                        } else {
                            3
                        },
                        "same.rs",
                    )
                } else {
                    value.clone()
                }))
            },
        ))?;
        assert_eq!(result.status, "needs_input");
        assert_eq!(result.requires_input[0].choices.len(), 2);
        assert_eq!(result.evidence.len(), 2);
        // The independently qualified excerpt step may read each origin;
        // the ambiguous definition structure step never guesses a member.
        assert_eq!(
            result
                .steps
                .iter()
                .filter(|step| step.tool == "outline" && step.status == "skipped")
                .count(),
            1
        );
        Ok(())
    }

    #[test]
    fn unqualified_followup_and_mixed_legacy_scope_do_not_dispatch() -> Result<()> {
        let result = run(execute(
            request(
                json!({"schema":REQUEST_SCHEMA,"mode":"execute","verification":"off","text":"/info queue.rs"}),
            ),
            selection(),
            identity(),
            |_, _| async { panic!("ambiguous origin must not dispatch") },
        ))?;
        assert_eq!(result.status, "needs_input");
        let error=run(execute(request(json!({"schema":REQUEST_SCHEMA,"plan":{"steps":[{"id":"read","tool":"search","args":{"query":"Queue","project":"legacy"}}]}})),selection(),identity(),|_,_|async{panic!("mixed selectors must not dispatch")})).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("unknown search argument project")
        );
        Ok(())
    }

    #[test]
    fn response_verifier_runs_once_after_projection_with_inner_checks_disabled() -> Result<()> {
        let calls = RefCell::new(Vec::new());
        let result = run(execute(
            request(json!({"schema":REQUEST_SCHEMA,"mode":"execute","text":"/search Queue"})),
            selection(),
            identity(),
            |tool, args| {
                calls.borrow_mut().push((tool.clone(), args.clone()));
                std::future::ready(Ok(if tool == "__airk_verify_projection" {
                    json!({"schema":"airk.combined-verification/1","scope_identity":identity(),"partial":true,"members":[{"index_id":index(2),"result":{"error":{"code":"original-fixture-observation-unavailable"}}}]})
                } else {
                    fixture(2, "queue.rs")
                }))
            },
        ))?;
        assert_eq!(calls.borrow().len(), 2);
        assert_eq!(calls.borrow()[0].1["verification"], "off");
        assert_eq!(calls.borrow()[1].0, "__airk_verify_projection");
        assert_eq!(
            calls.borrow()[1].1["projection"]["evidence"][0]["origin"]["index_id"],
            json!(index(2))
        );
        assert!(result.member_verification.is_some());
        Ok(())
    }
}
