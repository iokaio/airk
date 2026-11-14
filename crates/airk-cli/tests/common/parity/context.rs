// SPDX-License-Identifier: Apache-2.0
//! Strict /2 unassessed fallback comparison, after owning raw validation.
use airk_core::{arbiter::{Budget,context_action::{AssessmentResult,ContextResponseV2}},identity::{Incarnation,Sha256Digest,framed_digest}};
use anyhow::{Context,Result,ensure};
use serde_json::{Value,json};
use std::collections::BTreeMap;

fn primitive(raw:&Value)->Result<Value>{
    let mut rules=super::strict::Normalization::default();
    if let Some(id)=raw.get("request_id") {
        let _:Incarnation=serde_json::from_value(id.clone())?;
        rules.identities.insert("/request_id".into(),id.as_str().context("primitive request ID")?.into());
    }
    let mut result=super::strict::normalize(raw,&rules).map_err(anyhow::Error::msg)?;
    if let Some(freshness)=raw.get("freshness") {
        let _:airk_core::changes::FreshnessObservation=serde_json::from_value(freshness.clone())?;
        let mut timing=super::strict::Normalization::default();timing.timing.push("/observed_unix_ms".into());
        result["freshness"]=super::strict::normalize(freshness,&timing).map_err(anyhow::Error::msg)?;
    }
    if let Some(receipt)=raw.get("source_verification") {
        let typed:airk_core::verification::VerificationReceipt=serde_json::from_value(receipt.clone())?;
        ensure!(typed.observed_to_unix_ms>=typed.observed_from_unix_ms,"crossed source verification interval");
        let mut normalized=receipt.clone();
        normalized["observed_from_unix_ms"]=json!("<verified-source-observation-start>");
        normalized["observed_to_unix_ms"]=json!("<verified-source-observation-end>");
        result["source_verification"]=normalized;
    }
    Ok(result)
}

fn authenticated(raw:&Value,budget:&Budget)->Result<ContextResponseV2>{
    let typed=ContextResponseV2::from_canonical(raw,budget)?;
    ensure!(typed.omitted_count==0,"omitted/non-prefix records require a separate owning replay proof");
    ensure!(typed.assessment_history.iter().all(|event|matches!(&event.result,AssessmentResult::Unavailable{..})),
        "learned decisions require explicit prompt/decision parity, not fallback rebinding");
    let scope=&typed.assessment.scope;
    let application=typed.applications.iter().find(|application|application.scope==*scope)
        .context("fallback replay requires the actual same-scope application")?.clone();
    let now=std::time::Instant::now();
    let request=airk_core::arbiter::ContextRequest{schema:airk_core::arbiter::REQUEST_SCHEMA.into(),question:typed.question.clone(),budget:budget.clone()};
    let mut gathering=airk_kernel::arbiter::Gathering::new(request,scope.clone(),application,now)?;
    for record in &typed.records {
        let dispatch=gathering.next(now)?.context("record is not in the owning fallback dispatch prefix")?;
        ensure!(dispatch.choice.operation==record.operation&&dispatch.arguments.clone().into_iter().collect::<BTreeMap<_,_>>()==record.arguments,
            "raw record operation/arguments differ from owning dispatch");
        let observed=if record.canonical_result["isError"]==true {
            Err(airk_kernel::workbench::ReadFailure(record.canonical_result.clone()).into())
        } else {Ok(record.canonical_result.clone())};
        gathering.observe(observed,now)?;
        let generated=gathering.records().last().context("raw record was not retained by owning recipe")?;
        ensure!(serde_json::to_value(generated)?==serde_json::to_value(record)?,
            "raw record digest/scope/body do not match owning canonical dispatch");
    }
    Ok(typed)
}
fn packet(raw:&Value,budget:&Budget)->Result<Value>{
    let typed=authenticated(raw,budget)?;
    let mut result=primitive(raw)?;
    let mut records=BTreeMap::new();
    for (position,record) in typed.records.iter().enumerate(){
        let ordinal=u64::try_from(position)?;
        let logical=framed_digest(b"airk.test.context-record-position/1",&[&ordinal.to_le_bytes()]);
        ensure!(records.insert(record.id,logical).is_none(),"duplicate raw record identity");
        result["records"][position]["id"]=serde_json::to_value(logical)?;
        result["records"][position]["canonical_result"]=primitive(&record.canonical_result)?;
    }
    let rebound=|id:Sha256Digest|->Result<Value>{
        serde_json::to_value(records.get(&id).context("orphaned raw evidence reference")?).map_err(Into::into)
    };
    for (position,paragraph) in typed.paragraphs.iter().enumerate(){
        for (reference,id) in paragraph.evidence.iter().enumerate(){
            result["paragraphs"][position]["evidence"][reference]=rebound(*id)?;
        }
    }
    for (position,event) in typed.assessment_history.iter().enumerate(){
        for (reference,unit) in event.basis.source_units_presented.iter().enumerate(){
            result["assessment_history"][position]["basis"]["source_units_presented"][reference]["record"]=rebound(unit.record)?;
        }
    }
    Ok(result)
}

/// Both full raw graphs are authenticated by the owning native recipe FIRST.
pub fn pair(left:&Value,right:&Value,budget:&Budget)->Result<(Value,Value)>{
    Ok((packet(left,budget)?,packet(right,budget)?))
}

/// Only the actually negotiated older MCP framing omits this SDK constant.
pub fn negotiated_mcp_pair(left:&Value,right:&Value,protocol:&str)->Result<(Value,Value)>{
    ensure!(matches!(protocol,"2024-11-05"|"2025-03-26"|"2025-06-18"|"2025-11-25"|"2026-07-28"),"unknown actual negotiated MCP protocol");
    let mut left=left.clone();
    if protocol!="2026-07-28" {
        ensure!(right.get("resultType").is_none(),"negotiated older MCP wire retained resultType");
        if let Some(kind)=left.as_object_mut().context("native envelope")?.remove("resultType") {
            ensure!(kind=="complete","unknown native result discriminator");
        }
    }
    Ok((left,right.clone()))
}

fn envelope_body(raw:&Value)->Result<Value>{
    ensure!(raw.get("isError")!=Some(&Value::Bool(true)),"finite Answer failed; no replay permitted");
    let text=raw.pointer("/content/0/text").and_then(Value::as_str).context("Answer text body")?;
    let body:Value=serde_json::from_str(text)?;
    if let Some(structured)=raw.get("structuredContent"){ensure!(body==*structured,"Answer text/structured duplicate mismatch");}
    Ok(body)
}
pub fn envelopes(left:&Value,right:&Value,budget:&Budget)->Result<(Value,Value)>{
    let (a,b)=pair(&envelope_body(left)?,&envelope_body(right)?,budget)?;
    Ok((replace_body(left,a)?,replace_body(right,b)?))
}

pub fn replace_body(raw:&Value,body:Value)->Result<Value>{
    envelope_body(raw)?;
    let mut result=raw.clone();result["content"][0]["text"]=serde_json::to_value(serde_json::to_string(&body)?)?;
    if raw.get("structuredContent").is_some(){result["structuredContent"]=body;}
    Ok(result)
}
