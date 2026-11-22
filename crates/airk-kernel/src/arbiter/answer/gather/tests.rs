// SPDX-License-Identifier: Apache-2.0
//! The shared read cache with synthetic canonical replies.
use super::*;
use crate::arbiter::answer::tests::{hit, reply, scope};

fn search(gather: &mut Gathering, part: u8, need: Option<u8>, query: &str) -> Result<Dispatch> {
    gather.dispatch(
        ("search", v3::LoopRead::Search),
        json!({"query":query,"limit":6,"max_bytes":4096,"full_chunks":false}),
        (part, need, 1),
        if need.is_some() {
            Cause::NeedSearch
        } else {
            Cause::PartSearch
        },
        software::FollowUp::Initial,
        Step::Search,
        query,
    )
}

#[test]
fn two_parts_needing_one_file_read_it_once_and_both_keep_provenance() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let first = search(&mut gather, 1, Some(1), "restart guard")?;
    let second = search(&mut gather, 2, Some(3), "restart guard")?;
    assert_eq!(first.key, second.key);
    assert_eq!(gather.offer(first), Offer::Queued);
    assert_eq!(gather.offer(second), Offer::Waiting);
    assert_eq!(
        gather.take(2, 4).len(),
        0,
        "the read belongs to part 1's queue"
    );
    let taken = gather.take(1, 4);
    assert_eq!(taken.len(), 1);
    let body = reply(
        &scope,
        vec![hit("src/guard.rs", 1, 9, "fn restart_guard() {}")],
    );
    assert_eq!(
        gather.observe(taken[0].seq, Ok(body))?,
        Observed::Retained(0)
    );
    assert_eq!(gather.calls, 1);
    assert_eq!(gather.records.len(), 1);
    let uses = &gather.records[0].uses;
    assert_eq!(uses.len(), 2);
    assert_eq!(uses[0].cause, v3::UseCause::NeedSearch);
    assert_eq!(
        (uses[1].part, uses[1].need, uses[1].cause),
        (2, Some(3), v3::UseCause::SharedRead)
    );
    // A later round asking for the same read is served from the cache.
    let mut again = search(&mut gather, 2, Some(1), "restart guard")?;
    again.round = 2;
    assert_eq!(gather.offer(again), Offer::Cached(0));
    assert_eq!(gather.calls, 1);
    assert_eq!(gather.records[0].uses.len(), 3);
    Ok(())
}

#[test]
fn pending_read_waiters_attach_without_second_dispatch() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let first = search(&mut gather, 1, None, "queue policy")?;
    gather.offer(first);
    let taken = gather.take(1, 4);
    assert_eq!(gather.pending(), 1);
    let waiter = search(&mut gather, 1, Some(2), "queue policy")?;
    assert_eq!(gather.offer(waiter), Offer::Waiting);
    assert!(gather.take(1, 4).is_empty());
    gather.observe(
        taken[0].seq,
        Ok(reply(&scope, vec![hit("a.md", 1, 2, "policy")])),
    )?;
    assert_eq!(gather.records[0].uses.len(), 2);
    assert_eq!(gather.calls, 1);
    Ok(())
}

#[test]
fn failed_follow_up_keeps_receipt_and_queue() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let part = search(&mut gather, 1, None, "part")?;
    let follow = gather.dispatch(
        ("symbols", v3::LoopRead::Symbols),
        json!({"name":"restart_guard","limit":8,"max_bytes":4096}),
        (1, Some(1), 2),
        Cause::FollowUp,
        software::FollowUp::Definition,
        Step::Definitions,
        "restart_guard",
    )?;
    let later = search(&mut gather, 1, Some(2), "later")?;
    gather.offer(part);
    gather.offer(follow);
    gather.offer(later);
    let taken = gather.take(1, 2);
    gather.observe(taken[0].seq, Ok(reply(&scope, vec![])))?;
    let receipt = json!({"isError":true,"content":[{"type":"text","text":"ambiguous definition"}]});
    let failed = gather.observe(
        taken[1].seq,
        Err(anyhow::Error::new(crate::workbench::ReadFailure(
            receipt.clone(),
        ))),
    )?;
    assert_eq!(
        failed,
        Observed::Failed {
            record: Some(1),
            refused: vec![]
        }
    );
    assert!(gather.records[1].failure);
    assert_eq!(gather.records[1].body, receipt);
    assert_eq!(gather.queued_for(1), 1, "the remaining queue survives");
    assert!(gather.stop.is_none());
    Ok(())
}

#[test]
fn a_failed_part_search_refuses_and_a_foreign_publication_stops_every_part() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let part = search(&mut gather, 1, None, "part")?;
    gather.offer(part);
    let taken = gather.take(1, 1);
    let refused = gather.observe(taken[0].seq, Err(anyhow::anyhow!("index unavailable")))?;
    assert_eq!(
        refused,
        Observed::Failed {
            record: None,
            refused: vec![1]
        }
    );
    let other = search(&mut gather, 2, None, "other")?;
    gather.offer(other);
    let taken = gather.take(2, 1);
    let mut body = reply(&scope, vec![]);
    body["publication"]["token"] = json!("another-revision");
    assert_eq!(gather.observe(taken[0].seq, Ok(body))?, Observed::Stale);
    assert_eq!(gather.stop, Some(GlobalStop::Stale));
    Ok(())
}

#[test]
fn retention_and_call_budgets_stop_the_answer_on_budget() -> Result<()> {
    let scope = scope();
    let budget = v3::BudgetV3 {
        calls: 2,
        max_bytes: 4096 + 1024,
        ..v3::BudgetV3::default()
    };
    let mut gather = Gathering::new(budget, scope.clone())?;
    for query in ["one", "two", "three"] {
        let dispatch = search(&mut gather, 1, Some(1), query)?;
        gather.offer(dispatch);
    }
    let taken = gather.take(1, 3);
    assert_eq!(taken.len(), 2, "the call budget admits two reads");
    assert_eq!(gather.stop, Some(GlobalStop::Budget));
    gather.observe(
        taken[0].seq,
        Ok(reply(&scope, vec![hit("a.md", 1, 1, "x")])),
    )?;
    let large = reply(&scope, vec![hit("b.md", 1, 1, &"y".repeat(6000))]);
    assert_eq!(gather.observe(taken[1].seq, Ok(large))?, Observed::Omitted);
    assert_eq!(
        (gather.candidates, gather.omitted, gather.records.len()),
        (2, 1, 1)
    );
    Ok(())
}

#[test]
fn new_evidence_counts_only_excerpts_new_to_the_part() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let first = search(&mut gather, 1, Some(1), "one")?;
    gather.offer(first);
    let taken = gather.take(1, 1);
    let body = reply(&scope, vec![hit("a.md", 1, 1, "same")]);
    gather.observe(taken[0].seq, Ok(body.clone()))?;
    let mut repeat = search(&mut gather, 1, Some(1), "two")?;
    repeat.round = 2;
    gather.offer(repeat);
    let taken = gather.take(1, 1);
    gather.observe(taken[0].seq, Ok(body))?;
    assert_eq!(gather.new_evidence(1, 1), 1);
    assert_eq!(
        gather.new_evidence(1, 2),
        0,
        "a repeated excerpt is no progress"
    );
    Ok(())
}

#[test]
fn a_stopped_part_hands_a_shared_queued_read_to_its_other_waiter() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let first = search(&mut gather, 1, Some(1), "shared")?;
    let second = search(&mut gather, 2, Some(2), "shared")?;
    gather.offer(first);
    gather.offer(second);
    let drained = gather.drain_part(1);
    assert_eq!(drained.len(), 1, "part 1 lists it as unrun");
    assert_eq!(gather.queued_for(1), 0);
    assert_eq!(gather.queued_for(2), 1, "part 2 still runs the read");
    let taken = gather.take(2, 1);
    assert_eq!((taken[0].part, taken[0].need), (2, Some(2)));
    gather.observe(
        taken[0].seq,
        Ok(reply(&scope, vec![hit("a.md", 1, 2, "x")])),
    )?;
    let uses = &gather.records[0].uses;
    assert_eq!(uses.len(), 1);
    assert_eq!((uses[0].part, uses[0].cause), (2, v3::UseCause::NeedSearch));
    Ok(())
}

#[test]
fn reads_are_observed_in_dispatch_order_and_keys_ignore_argument_order() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let one = search(&mut gather, 1, Some(1), "one")?;
    let two = search(&mut gather, 1, Some(2), "two")?;
    gather.offer(one);
    gather.offer(two);
    let taken = gather.take(1, 2);
    assert!(
        gather
            .observe(taken[1].seq, Ok(reply(&scope, vec![])))
            .is_err()
    );
    gather.observe(taken[0].seq, Ok(reply(&scope, vec![])))?;
    gather.observe(taken[1].seq, Ok(reply(&scope, vec![])))?;
    let mut forward = Map::new();
    forward.insert("query".into(), json!("x"));
    forward.insert("limit".into(), json!({"b":1,"a":2}));
    let mut backward = Map::new();
    backward.insert("limit".into(), json!({"a":2,"b":1}));
    backward.insert("query".into(), json!("x"));
    assert_eq!(
        read_key("search", &forward, "r")?,
        read_key("search", &backward, "r")?
    );
    Ok(())
}

#[test]
fn a_failed_part_search_refuses_every_part_waiting_on_it() -> Result<()> {
    let scope = scope();
    let mut gather = Gathering::new(v3::BudgetV3::default(), scope.clone())?;
    let first = search(&mut gather, 1, None, "same")?;
    let second = search(&mut gather, 2, None, "same")?;
    gather.offer(first);
    gather.offer(second);
    let taken = gather.take(1, 1);
    let failure = json!({"isError":true});
    let failed = gather.observe(
        taken[0].seq,
        Err(anyhow::Error::new(crate::workbench::ReadFailure(failure))),
    )?;
    assert_eq!(
        failed,
        Observed::Failed {
            record: Some(0),
            refused: vec![1, 2]
        }
    );
    let third = search(&mut gather, 3, None, "same")?;
    assert_eq!(
        gather.offer(third),
        Offer::Failed(Some(0)),
        "never served as evidence"
    );
    Ok(())
}
