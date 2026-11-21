// SPDX-License-Identifier: Apache-2.0
//! Private arithmetic for verified R5 or R8 study inputs. No profile or calibration artifact.
use super::*;
use airk_core::arbiter::context_action::calibration::terminal::raw_winner;

const GRAPH: &str = "147be075a73ff81ca17b6c2fb965ec8f887e10908fbd33cf47a919d1694aa3d9";
const WEIGHTS: &str = "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3";
const RECIPE: &str = "7efd4fc72f11de95131c532467803a6a2c82c71cf84ac0bd23cabbcf3662e394";
// Completed CAL100 lifetime plans whose verified inputs the study accepts: R5, then R8.
const PLANS: [&str; 2] = [
    "55a89e5219cf6de0c3b59cbaa8a4a8a43dc0d097276368cfb5a28ee2fe6536bf",
    "52e2fb170c4074da74c4f08a04b02364c5dfde7dd612d0f2c964e0fbd157242c",
];

// One accepted plan must name both the input and its independent preflight.
fn study_plan(input: &Value) -> Option<&'static str> {
    PLANS
        .into_iter()
        .find(|plan| input["plan"] == *plan && input["preflight"]["lifetime_plan_sha256"] == *plan)
}

fn supported_winner(case: &Case) -> Result<Option<(Sha256Digest, f32, bool)>> {
    let Some(scores) = &case.scores else {
        return Ok(None);
    };
    ensure!(
        scores.len() == case.roster.cards.len(),
        "incomplete native roster"
    );
    let Some((slot, margin)) = raw_winner(scores)? else {
        return Ok(None);
    };
    let card = &case.roster.cards[slot];
    Ok(generic_rule(0.0)
        .supports(card)
        .then(|| (card.id, margin, case.certified.contains(&card.id))))
}

// Any feasible positive coverage has a largest admitted cutoff equal to the
// smallest selected winner margin. The frozen fit enumerates all these exact
// f32 differences (and additional redundant boundaries).
fn cutoff(winners: &[(f32, bool)]) -> Option<f32> {
    let mut best = None;
    for &(margin, positive) in winners {
        if !positive {
            continue;
        }
        let mut admitted = 0;
        let safe = winners
            .iter()
            .filter(|(other, _)| *other >= margin)
            .all(|(_, certified)| {
                admitted += 1;
                *certified
            });
        if safe
            && best.is_none_or(|(count, threshold)| {
                admitted > count || admitted == count && margin > threshold
            })
        {
            best = Some((admitted, margin));
        }
    }
    best.map(|(_, threshold)| threshold)
}

#[test]
#[ignore = "explicit verified full-R5 or full-R8 private graph study; no scorer, asset calibration or product activation"]
fn study_cutlass_generic_read() -> Result<()> {
    let bytes = read_pinned(
        Path::new(&std::env::var("AIRK_CUTLASS_STUDY_INPUT")?),
        std::env::var("AIRK_CUTLASS_STUDY_INPUT_SHA256")?.parse()?,
    )?;
    let input: Value = serde_json::from_slice(&bytes)?;
    let plan = study_plan(&input).context("different study epoch")?;
    ensure!(
        input["schema"] == "airk.private.cutlass-study-verified-input/1"
            && input["graph"] == GRAPH
            && input["weights"] == WEIGHTS
            && input["recipe"] == RECIPE
            && input["preflight"]["focus_tokens"] == 1024,
        "different study epoch"
    );
    let original = rows(&input["preflight"], "rows")?;
    let raw = rows(&input["raw"], "rows")?;
    ensure!(
        original.len() == 100
            && raw.len() == 100
            && original.iter().zip(raw).all(|(a, b)| a["id"] == b["id"]),
        "original CAL100 order required"
    );
    let (cases, _) = selected_cases(&input["preflight"], &input["raw"])?;
    let mut winners = BTreeMap::new();
    let mut margins = Vec::new();
    for case in cases {
        if let Some((card, margin, witnessed)) = supported_winner(&case)? {
            winners.insert(case.id, (card, margin, witnessed));
            margins.push((margin, witnessed));
        }
    }
    let threshold = cutoff(&margins);
    let mut outcomes = Vec::new();
    for row in raw {
        let id = field(row, "id")?;
        let admitted = winners
            .get(id)
            .filter(|(_, margin, _)| threshold.is_some_and(|t| *margin >= t));
        outcomes.push(
            json!({"id":id,"status":row["status"],"card":admitted.map(|w|w.0),
            "witnessed_original_gain":admitted.map(|w|w.2),"semantic_negative":false}),
        );
    }
    println!(
        "AIRK_CUTLASS_READ_STUDY={}",
        json!({"input_sha256":sha(&bytes),"graph":GRAPH,"weights":WEIGHTS,
        "recipe":RECIPE,"plan":plan,"minimum_raw_margin":threshold,"margin_bits":threshold.map(f32::to_bits),
        "outcomes":outcomes,"release_qualified":false,"registered_profile":false})
    );
    Ok(())
}

#[test]
fn study_requires_unique_finite_supported_winner_and_zero_unknowns() -> Result<()> {
    let fixture: Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/split/arbiter/context_action_cases.json"
    )))?;
    let terminal = serde_json::from_value(
        fixture["cases"][0]["input"]["assessment_history"][0]["result"]["card"].clone(),
    )?;
    assert!(!generic_rule(0.0).supports(&terminal));
    assert_eq!(raw_winner(&[1., 1.])?, None);
    assert_eq!(raw_winner(&[f32::MAX, -f32::MAX])?, None);
    assert_eq!(cutoff(&[(1., true), (1., false)]), None);
    assert_eq!(cutoff(&[(2., true), (1., false), (0.5, true)]), Some(2.));
    Ok(())
}

#[test]
fn study_accepts_only_a_completed_plan_that_also_names_its_preflight() {
    let r5 = "55a89e5219cf6de0c3b59cbaa8a4a8a43dc0d097276368cfb5a28ee2fe6536bf";
    let r8 = "52e2fb170c4074da74c4f08a04b02364c5dfde7dd612d0f2c964e0fbd157242c";
    let bound = |plan: &str, preflight: &str| {
        study_plan(&json!({"plan":plan,"preflight":{"lifetime_plan_sha256":preflight}}))
    };
    assert_eq!(bound(r5, r5), Some(r5));
    assert_eq!(bound(r8, r8), Some(r8));
    assert_eq!(bound(r8, r5), None, "R8 input with the R5 certificate");
    assert_eq!(bound(r5, r8), None, "R5 input with the R8 certificate");
    assert_eq!(study_plan(&json!({"plan":r8,"preflight":{}})), None);
    // R7 never closed its full population, so its plan is not accepted.
    let r7 = "c04be2262a32d8318f9aa222e73ea9bf078ad80d7975d907eebefe486895a2eb";
    assert_eq!(bound(r7, r7), None);
}
