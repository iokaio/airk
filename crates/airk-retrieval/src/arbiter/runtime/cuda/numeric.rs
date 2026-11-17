// SPDX-License-Identifier: Apache-2.0
//! Shared native raw/mass/argmax replay, moved from software_v2 qualification.
use anyhow::{Result, ensure};
fn softmax(values: &[f32]) -> Result<Vec<f64>> {
    ensure!(
        !values.is_empty() && values.iter().all(|value| value.is_finite()),
        "finite native numeric scores required"
    );
    let peak = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let values = values
        .iter()
        .map(|value| (f64::from(*value) - f64::from(peak)).exp())
        .collect::<Vec<_>>();
    let total: f64 = values.iter().sum();
    Ok(values.into_iter().map(|value| value / total).collect())
}
pub fn compare_reference_scores(actual: &[f32], expected: &[f32]) -> Result<(f64, f64, bool)> {
    ensure!(
        actual.len() == expected.len(),
        "native numeric reference cardinality differs"
    );
    let a = softmax(actual)?;
    let b = softmax(expected)?;
    let winner = |scores: &[f32]| {
        scores.iter().enumerate().fold(
            0,
            |best, (index, value)| if *value > scores[best] { index } else { best },
        )
    };
    Ok((
        actual
            .iter()
            .zip(expected)
            .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
            .fold(0f64, f64::max),
        a.iter()
            .zip(b)
            .map(|(a, b)| (a - b).abs())
            .fold(0f64, f64::max),
        winner(actual) == winner(expected),
    ))
}
