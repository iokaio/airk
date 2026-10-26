// SPDX-License-Identifier: Apache-2.0
//! Test clients repeat only the public, typed read-rejection contract.
#![allow(dead_code)]
use anyhow::Result;
use serde_json::{Map, Value};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retry {
    pub delay: Duration,
    pub fresh_brokerage: bool,
    pub publication: bool,
}

pub fn classify(envelope: &Value) -> Option<Retry> {
    if envelope.get("isError")? != true {
        return None;
    }
    let content = envelope.get("content")?.as_array()?;
    if content.len() != 1 || content[0].get("type")? != "text" {
        return None;
    }
    let hint: Value = serde_json::from_str(content[0].get("text")?.as_str()?).ok()?;
    let (delay, fresh_brokerage, publication) = if hint.get("status") == Some(&Value::from("retry"))
    {
        if hint.get("retryable")? != true
            || hint.get("action")? != "repeat_same_request"
            || !matches!(
                hint.get("code")?.as_str()?,
                "publication_busy" | "selection_busy"
            )
        {
            return None;
        }
        (hint.get("retry_after_ms")?.as_u64()?, false, true)
    } else {
        let error = hint.get("error")?;
        let fresh = match error.get("code")?.as_str()? {
            "direct_query_admission" => false,
            "direct_query_setup" => true,
            _ => return None,
        };
        if error.get("reason")? != "busy"
            || error.get("query_submitted")? != false
            || error.get("retryable")? != true
            || error.get("fresh_brokerage_required")?.as_bool()? != fresh
        {
            return None;
        }
        (error.get("retry_after_ms")?.as_u64()?, fresh, false)
    };
    (1..=1000).contains(&delay).then_some(Retry {
        delay: Duration::from_millis(delay),
        fresh_brokerage,
        publication,
    })
}

pub fn within(envelope: &Value, deadline: Instant) -> Option<Retry> {
    classify(envelope).filter(|retry| Instant::now() + retry.delay < deadline)
}

pub fn from_output(output: &std::process::Output, deadline: Instant) -> Option<Retry> {
    // Tool rejection exits 1. A killed or otherwise failed process is not a reply.
    if !output.status.success() && output.status.code() != Some(1) {
        return None;
    }
    serde_json::from_slice::<Value>(&output.stdout)
        .ok()
        .and_then(|value| within(&value, deadline))
}

pub fn output(command: &mut std::process::Command) -> std::io::Result<std::process::Output> {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let output = command.output()?;
        let retry = from_output(&output, deadline);
        let Some(retry) = retry else {
            return Ok(output);
        };
        std::thread::sleep(retry.delay);
    }
}

pub(crate) trait ReadRetry {
    async fn read_retry(&mut self, name: String, arguments: Map<String, Value>) -> Result<Value>;
}
impl ReadRetry for airk_cli::project_client::Connection {
    async fn read_retry(&mut self, name: String, arguments: Map<String, Value>) -> Result<Value> {
        direct(
            self,
            &name,
            &arguments,
            Instant::now() + Duration::from_secs(5),
            |_| {},
        )
        .await
    }
}

pub async fn direct(
    connection: &mut airk_cli::project_client::Connection,
    name: &str,
    arguments: &Map<String, Value>,
    deadline: Instant,
    mut rejected: impl FnMut(&Value),
) -> Result<Value> {
    loop {
        let receipt = connection.status_receipt();
        // Neither a transport error nor a successful query authorizes replay.
        let reply = connection.call(name.into(), arguments.clone()).await?;
        let Some(retry) = within(&reply, deadline) else {
            return Ok(reply);
        };
        // A raw connection cannot satisfy a request for fresh brokerage.
        if retry.fresh_brokerage {
            return Ok(reply);
        }
        if retry.publication {
            assert_eq!(
                connection.status_receipt(),
                receipt,
                "rejected publication read changed the client's observation receipt"
            );
        }
        rejected(&reply);
        tokio::time::sleep(retry.delay).await;
    }
}

pub fn evidence_for_comparison(value: &mut Value) -> Result<()> {
    let Some(fields) = value.as_object_mut() else {
        anyhow::bail!("expected evidence object")
    };
    fields.remove("request_id");
    if let Some(freshness) = fields.remove("freshness") {
        let observation: airk_core::changes::FreshnessObservation =
            serde_json::from_value(freshness)?;
        assert!(observation.observed_unix_ms > 0);
        assert!(!observation.live_source_verified);
        if observation.state == airk_core::changes::FreshnessState::Unavailable {
            assert!(observation.error.is_some());
        }
    }
    if let Some(readiness) = fields.remove("semantic_readiness") {
        let _: airk_core::readiness::SemanticReadiness = serde_json::from_value(readiness)?;
    }
    if let Some(readiness) = fields.remove("discovery_readiness") {
        let _: airk_core::readiness::DiscoveryReadiness = serde_json::from_value(readiness)?;
    }
    if let Some(observation) = fields.remove("discovery_observation") {
        assert_eq!(observation, "unavailable_after_read");
    }
    Ok(())
}
