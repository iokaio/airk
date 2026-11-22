// SPDX-License-Identifier: Apache-2.0
//! Batched Focus settlement against the loop ledger, with the pair loop's
//! outcome injected.
use super::*;
use airk_core::{
    arbiter::answer_loop::{BudgetV3, TokenAttribution},
    identity::{Incarnation, RevisionToken, Sha256Digest},
    index_catalog::IndexId,
    read_attempt::{ReadAttemptBudget, ReadClock},
};

fn attempt() -> ReadAttemptBudget {
    ReadAttemptBudget {
        schema: 1,
        request_id: Incarnation::from_bytes([1; 16]),
        clock: ReadClock::WindowsPerformanceCounter,
        expires_monotonic_ns: 600_000_000_100,
        remaining_ns: 600_000_000_000,
        native_retry_expires_monotonic_ns: None,
    }
}
fn index() -> IndexId {
    IndexId::from_identity(Incarnation::from_bytes([8; 16]))
}
fn charge(part: u8, tokens: usize) -> TokenAttribution {
    TokenAttribution {
        part,
        round: 1,
        tokens,
    }
}
fn reservation() -> Vec<TokenAttribution> {
    vec![charge(1, 1_024), charge(2, 1_024)]
}
fn budget() -> BudgetV3 {
    BudgetV3 {
        round_input_tokens: 4_096,
        ..BudgetV3::default()
    }
}
fn reserved() -> Result<(Mutex<loop_ledger::Ledger>, loop_ledger::Ticket)> {
    let ledger = Mutex::new(loop_ledger::Ledger::default());
    let ticket = ledger.lock().unwrap().reserve_focus(
        attempt(),
        &budget(),
        loop_ledger::FocusCall {
            index_id: index(),
            ordinal: 1,
            pairs: 2,
        },
        &reservation(),
        None,
        10,
    )?;
    Ok((ledger, ticket))
}
/// Whether `part`, round 1 can still take `tokens` in Focus ordinal 2.
fn room(ledger: &Mutex<loop_ledger::Ledger>, part: u8, tokens: usize) -> bool {
    ledger
        .lock()
        .unwrap()
        .reserve_focus(
            attempt(),
            &budget(),
            loop_ledger::FocusCall {
                index_id: index(),
                ordinal: 2,
                pairs: 2,
            },
            &[charge(part, tokens)],
            None,
            11,
        )
        .is_ok()
}
fn receipt(count: usize) -> Result<FocusBatchReceipt> {
    let scope = ScopeBinding {
        installation: Sha256Digest::from_bytes([6; 32]),
        catalog: Incarnation::from_bytes([7; 16]),
        index_id: index(),
        authorization: Sha256Digest::from_bytes([9; 32]),
        configuration_revision: 2,
        publication: airk_core::evidence::RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([10; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([11; 32]),
            },
        ),
    };
    Ok(FocusBatchReceipt {
        schema: FOCUS_BATCH_SCHEMA.into(),
        scope_digest: airk_core::arbiter::answer_loop::scope_digest(&scope)?,
        identity: None,
        availability: Availability::Failed,
        reason: UNKNOWN.into(),
        applied: false,
        input_digest: Sha256Digest::from_bytes([12; 32]),
        order_digest: Sha256Digest::from_bytes([13; 32]),
        questions_digest: Sha256Digest::from_bytes([14; 32]),
        question_count: 2,
        candidate_count: count,
        ranked: (0..count).map(|index| index as u16).collect(),
        pairs: vec![],
        reserved_input_tokens: 2_048,
        observed_input_tokens: None,
        attribution: reservation(),
        work: Work::default(),
    })
}
fn pair(candidate: u16, yes: f32) -> FocusPairScore {
    FocusPairScore {
        candidate,
        question: (candidate % 2) as u8,
        no_logit: 0.0,
        yes_logit: yes,
    }
}
fn ids(count: usize) -> Vec<Sha256Digest> {
    (0..count)
        .map(|index| Sha256Digest::from_bytes([index as u8 + 40; 32]))
        .collect()
}
fn settle(
    ledger: &Mutex<loop_ledger::Ledger>,
    ticket: loop_ledger::Ticket,
    receipt: &mut FocusBatchReceipt,
    tokens: usize,
    refused: bool,
    still_live: bool,
) -> Result<()> {
    let reserved = reservation();
    settle_focus_batch(
        FocusSettle {
            ledger,
            ticket,
            reserved: &reserved,
            ids: ids(receipt.candidate_count),
            score_count: 2,
            tokens,
            settled: true,
            failed: false,
            refused,
            still_live,
        },
        receipt,
    )
}

#[test]
fn a_batch_with_no_queued_pair_is_released_as_known_zero_work() -> Result<()> {
    let (ledger, ticket) = reserved()?;
    let mut receipt = receipt(3)?;
    settle(&ledger, ticket, &mut receipt, 0, true, true)?;
    receipt.validate(&receipt_scope()?, &budget())?;
    assert_eq!(receipt.observed_input_tokens, Some(0));
    assert_eq!(receipt.attribution, vec![charge(1, 0), charge(2, 0)]);
    assert_eq!(receipt.reason, NOT_QUEUED);
    assert!(!receipt.applied);
    assert!(room(&ledger, 2, 4_096), "part 2's allowance is free again");
    Ok(())
}

#[test]
fn a_complete_batch_ranks_by_relevance_and_settles_each_part() -> Result<()> {
    let (ledger, ticket) = reserved()?;
    let mut receipt = receipt(3)?;
    receipt.pairs = vec![pair(0, 0.1), pair(1, 2.0)];
    receipt.work.candidates = 2;
    settle(&ledger, ticket, &mut receipt, 1_000, false, true)?;
    assert!(receipt.applied);
    assert_eq!(
        receipt.ranked,
        vec![1, 0, 2],
        "the unscored tail keeps its place"
    );
    assert_eq!(receipt.observed_input_tokens, Some(1_000));
    assert_eq!(receipt.attribution, vec![charge(1, 500), charge(2, 500)]);
    assert!(room(&ledger, 2, 4_096 - 500));
    assert!(!room(&ledger, 2, 4_096 - 499));
    Ok(())
}

#[test]
fn a_batch_cut_short_keeps_canonical_order_and_charges_known_work() -> Result<()> {
    let (ledger, ticket) = reserved()?;
    let mut receipt = receipt(3)?;
    receipt.pairs = vec![pair(0, 0.1)];
    receipt.work.candidates = 1;
    settle(&ledger, ticket, &mut receipt, 512, true, true)?;
    assert!(!receipt.applied);
    assert_eq!(receipt.ranked, vec![0, 1, 2]);
    assert_eq!(receipt.observed_input_tokens, Some(512));
    // Review L1, recorded: without a candidate-to-part map the known work is
    // charged to every reserved part-round in proportion.
    assert_eq!(receipt.attribution, vec![charge(1, 256), charge(2, 256)]);
    Ok(())
}

#[test]
fn a_complete_batch_after_authority_changed_is_not_applied() -> Result<()> {
    let (ledger, ticket) = reserved()?;
    let mut receipt = receipt(2)?;
    receipt.pairs = vec![pair(0, 0.1), pair(1, 2.0)];
    receipt.work.candidates = 2;
    settle(&ledger, ticket, &mut receipt, 900, false, false)?;
    assert!(!receipt.applied);
    assert_eq!(receipt.ranked, vec![0, 1]);
    assert_eq!(receipt.observed_input_tokens, Some(900));
    Ok(())
}

fn receipt_scope() -> Result<ScopeBinding> {
    Ok(ScopeBinding {
        installation: Sha256Digest::from_bytes([6; 32]),
        catalog: Incarnation::from_bytes([7; 16]),
        index_id: index(),
        authorization: Sha256Digest::from_bytes([9; 32]),
        configuration_revision: 2,
        publication: airk_core::evidence::RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([10; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([11; 32]),
            },
        ),
    })
}
