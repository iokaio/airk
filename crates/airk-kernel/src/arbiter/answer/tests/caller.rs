// SPDX-License-Identifier: Apache-2.0
//! Caller-stated needs and anchors (Core CONTRACT "Optional caller inputs").

use super::*;

/// A request whose question would split into two parts without caller needs.
const COMPOUND: &str = "Please answer both:\n1. explain how `restart_guard` decides to replay\n2. update the README section on the restart guard";

fn same(_: &str, _: &Map<String, Value>, body: Value) -> Result<Value> {
    Ok(body)
}

fn stages(outcome: &AnswerOutcome) -> Vec<v3::LoopStage> {
    outcome
        .custody
        .log
        .entries
        .iter()
        .map(|entry| entry.decision.stage)
        .collect()
}

#[test]
fn caller_needs_are_the_sub_needs_of_one_part_in_order() -> Result<()> {
    let fixture = fixture()?;
    let needs: Vec<String> = [
        "Where the restart guard decides whether to replay",
        "Which condition makes the guard refuse a replay",
        "What state the guard reads before it decides",
        "Which callers invoke the guard",
        "What the README says about the guard today",
        "Which tests cover the guard's refusal",
        "Whether a contract governs the guard",
        "What configuration changes the guard's behaviour",
    ]
    .map(String::from)
    .to_vec();
    let mut asked = request(COMPOUND);
    asked.needs = needs.clone();
    let (outcome, _, reads) = run(&fixture, asked, 10, &same)?;
    let response = &outcome.response;

    assert_eq!(response.parts.len(), 1, "caller needs make one part");
    let part = &response.parts[0];
    let stated: Vec<&str> = part
        .sub_needs
        .iter()
        .map(|need| need.text.as_str())
        .collect();
    assert_eq!(stated, needs.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        part.sub_needs
            .iter()
            .map(|need| need.id)
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5, 6, 7, 8]
    );
    for need in &part.sub_needs {
        assert_eq!(need.origin, v3::SubNeedOrigin::Caller);
        assert_eq!(need.kind, v3::SubNeedKind::Other);
    }
    // No Split and no Plan decision is taken: the caller decided both.
    let taken = stages(&outcome);
    assert!(!taken.contains(&v3::LoopStage::Split), "{taken:?}");
    assert!(!taken.contains(&v3::LoopStage::Plan), "{taken:?}");
    // Every need is searched in its own words.
    for need in &needs {
        let query = format!("\"query\":{}", serde_json::to_string(need)?);
        assert!(
            reads
                .iter()
                .any(|read| read.starts_with("search") && read.contains(&query)),
            "no search for {need}"
        );
    }
    let raw = serde_json::to_value(response)?;
    assert_eq!(raw["parts"][0]["sub_needs"][0]["origin"], "caller");
    response.validate_with_log(&outcome.custody.log, &v3::BudgetV3::default())?;
    Ok(())
}

#[test]
fn without_caller_needs_the_reply_keeps_its_shape() -> Result<()> {
    let fixture = fixture()?;
    let (outcome, _, _) = run(&fixture, request(COMPOUND), 10, &same)?;
    let response = &outcome.response;
    assert_eq!(response.parts.len(), 2, "the kernel still splits");
    let taken = stages(&outcome);
    assert!(taken.contains(&v3::LoopStage::Split) && taken.contains(&v3::LoopStage::Plan));
    let raw = serde_json::to_value(response)?;
    for part in raw["parts"].as_array().context("parts")? {
        for need in part["sub_needs"].as_array().context("sub-needs")? {
            assert!(
                need.get("origin").is_none(),
                "a derived sub-need omits origin"
            );
        }
    }
    for part in &response.parts {
        assert!(part.sub_needs.len() <= 4);
        assert!(
            part.sub_needs
                .iter()
                .all(|need| need.origin == v3::SubNeedOrigin::Derived)
        );
    }
    Ok(())
}

#[test]
fn every_anchor_gets_one_exact_lexical_lookup_in_round_one() -> Result<()> {
    let fixture = fixture()?;
    let anchors = ["session-not-open", "MAX_FILES=500", "a.b/c:d"];
    let mut asked = request("Why does the restart guard refuse a replay?");
    asked.anchors = anchors.map(String::from).to_vec();
    let (outcome, _, reads) = run(&fixture, asked, 10, &same)?;
    for anchor in anchors {
        let query = format!("\"query\":{}", serde_json::to_string(anchor)?);
        let lookups: Vec<&String> = reads
            .iter()
            .filter(|read| read.starts_with("search") && read.contains(&query))
            .collect();
        assert_eq!(lookups.len(), 1, "one lookup for {anchor}: {lookups:?}");
        assert!(
            lookups[0].contains("\"mode\":\"lexical\""),
            "{}",
            lookups[0]
        );
    }
    // Anchors alone leave planning to the kernel.
    assert!(stages(&outcome).contains(&v3::LoopStage::Plan));
    assert!(
        outcome.response.parts[0]
            .sub_needs
            .iter()
            .all(|need| need.origin == v3::SubNeedOrigin::Derived)
    );
    outcome
        .response
        .validate_with_log(&outcome.custody.log, &v3::BudgetV3::default())?;
    Ok(())
}

#[test]
fn a_failed_anchor_lookup_does_not_refuse_the_request() -> Result<()> {
    let fixture = fixture()?;
    let mut asked = request("Why does the restart guard refuse a replay?");
    asked.anchors = vec!["session-not-open".into()];
    let fail = |tool: &str, arguments: &Map<String, Value>, body: Value| {
        if tool == "search"
            && arguments.get("query").and_then(Value::as_str) == Some("session-not-open")
        {
            anyhow::bail!("lexical lookup unavailable");
        }
        Ok(body)
    };
    let (outcome, _, _) = run(&fixture, asked, 10, &fail)?;
    assert!(!outcome.response.parts.is_empty());
    assert!(!outcome.response.records.is_empty());
    Ok(())
}
