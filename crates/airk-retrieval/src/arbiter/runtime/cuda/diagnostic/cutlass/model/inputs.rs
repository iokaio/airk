// SPDX-License-Identifier: Apache-2.0
//! Frozen engineering and exposed development inputs for private scientific measurements.
use super::{CompiledInput, Path, Result, Value, ensure, fixed_input};

const Q543: &str = "af125f4c7ef1cd3776ea0cf7308743724a958744a921559746d907d0278868a4";
const CONTROL32: &str = "ec7b4845c37fb7cbe2a64a866f8b6cdc03479d65593cdb0a1d01102fe56fc335";
const WIDER: [(usize, &str); 3] = [
    (
        128,
        "791067affc1eeba53660a0044369f22f1f463535656419c5da9a486517222c7a",
    ),
    (
        1024,
        "948a46156a129e61b5beadce977e6314b6e96c9570a439724813fe9592fa6357",
    ),
    (
        8192,
        "ca9d34d962e308e6e53af8e14101058e8f02fbb4151bcc65eda2ad88be3de1f0",
    ),
];

fn checked(value: &Value, width: usize, cards: usize, digest: &str) -> Result<CompiledInput> {
    let input: CompiledInput = serde_json::from_value(value.clone())?;
    ensure!(
        input.input_digest.to_string() == digest
            && input.input_ids.len() == width
            && input.decision_position == i64::try_from(width)? - 1
            && input.attention_mask == vec![1; width]
            && input
                .position_ids
                .iter()
                .copied()
                .eq(0..i64::try_from(width)?)
            && input.option_mask == vec![true; cards]
            && input
                .candidate_positions
                .iter()
                .copied()
                .eq(32..32 + i64::try_from(cards)?),
        "frozen compiled tensor shape, token order or row identity changed"
    );
    Ok(input)
}

#[test]
fn protected_control_rejects_mask_position_and_card_drift() -> Result<()> {
    let value = serde_json::json!({"prompt":"control","input_ids":[19,23],"attention_mask":[1,1],
        "position_ids":[0,1],"decision_position":1,"candidate_positions":[32],"option_mask":[true],"input_digest":Q543});
    let mut input = checked(&value, 2, 1, Q543)?;
    super::validate_cal_input(&input)?;
    input.input_ids[0] = 151_936;
    assert!(super::validate_cal_input(&input).is_err());
    input.input_ids[0] = 0;
    input.attention_mask[0] = 0;
    assert!(super::validate_cal_input(&input).is_err());
    for (field, wrong) in [
        ("decision_position", serde_json::json!(0)),
        ("attention_mask", serde_json::json!([1, 0])),
        ("option_mask", serde_json::json!([false])),
        ("candidate_positions", serde_json::json!([33])),
        ("position_ids", serde_json::json!([1, 0])),
        ("input_digest", serde_json::json!(CONTROL32)),
    ] {
        let mut altered = value.clone();
        altered[field] = wrong;
        assert!(checked(&altered, 2, 1, Q543).is_err(), "{field}");
    }
    Ok(())
}

pub(super) fn load(campaign: &Path, name: &str) -> Result<CompiledInput> {
    let read = |path: &str, size, hash| -> Result<Value> {
        Ok(serde_json::from_slice(&super::super::pinned(
            &campaign.join(path),
            size,
            hash,
        )?)?)
    };
    match name {
        "fixed" => fixed_input(campaign),
        "q543" => {
            let record = read(
                "compact-development-TRAIN_Q543-focus1024-run-r1/fixture/context-cal-admissions/record-0001.json",
                204733,
                "50ed40cec62a39b0067515b9aab2cd97c2849ef547dad149c49d86a32a863912",
            )?;
            let rows = record["compiler_census"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing compiler census"))?;
            let mut compact = rows.iter().filter(|r| {
                r["profile"] == "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f"
            });
            let row = compact
                .next()
                .ok_or_else(|| anyhow::anyhow!("missing original Compact input"))?;
            ensure!(
                compact.next().is_none()
                    && row["fits_remaining_allowance"] == true
                    && row["NN_invocations"] == 0,
                "original Compact compiler row must be unique and fit"
            );
            checked(&row["compiled"], 1588, 7, Q543)
        }
        "32" => {
            let reference = read(
                "compact-fp16-qkv-fp32-reference-r1/compact-qkv-fp32-t32-reference.json",
                615133,
                "304d9b16922ce88f9db181c1e1956d9c0aa99ab29c5ef4dff26f2c37f5b0fa28",
            )?;
            checked(&reference["reference"]["input"], 32, 8, CONTROL32)
        }
        _ => {
            let (ordinal, (width, digest)) = WIDER
                .iter()
                .enumerate()
                .find(|(_, row)| row.0.to_string() == name)
                .ok_or_else(|| anyhow::anyhow!("unknown scientific input"))?;
            let reference = read(
                "compact-fp16-qkv-fp32-wider-reference-r1/compact-qkv-fp32-wider-reference.json",
                790303,
                "f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14",
            )?;
            checked(
                &reference["references"][ordinal]["input"],
                *width,
                8,
                digest,
            )
        }
    }
}
