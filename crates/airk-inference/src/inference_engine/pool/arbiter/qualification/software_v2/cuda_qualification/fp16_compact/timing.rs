// SPDX-License-Identifier: Apache-2.0
//! Frozen unprofiled Compact development timing populations and score custody.
use super::*;

const T32_NATIVE: &str = "217431438d1bf79bbffa071c9d69d62f4dcb73c44cdee458c19cc50a544d774f";
const WIDER_NATIVE: &str = "4da43ecdfaeba452cc65009bcadfb82de942253c89a7d544c6285ad90855993a";

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Population {
    Fit,
    Fixed,
    Mixed,
}

impl Population {
    pub(super) fn journal_operation(self, index: usize) -> (operation::Kind, usize) {
        match (self, index) {
            (Self::Fixed, 0) => (operation::Kind::Warmup, 0),
            (Self::Fixed, i) => (operation::Kind::Sample, i),
            (Self::Mixed | Self::Fit, i) => (operation::Kind::Sample, i + 1),
        }
    }

    pub(super) fn schedule(self) -> Vec<(usize, usize, &'static str)> {
        match self {
            Self::Fit => vec![(3, 0, "sample")],
            Self::Fixed => std::iter::once((3, 0, "warmup"))
                .chain(std::iter::repeat_n((3, 0, "sample"), 20))
                .collect(),
            Self::Mixed => (1..=3)
                .flat_map(|cycle| [0, 1, 2, 3, 0].map(|index| (index, cycle, "sample")))
                .collect(),
        }
    }
}

pub(super) struct Sealed {
    pub(super) pins: [ResourceFilePin; 2],
    pub(super) scores: [[f32; 8]; 4],
}

impl Sealed {
    pub(super) fn load() -> Result<Self> {
        let short = env_pin(
            "AIRK_CUDA_FP16_TIMING_T32",
            "AIRK_CUDA_FP16_TIMING_T32_SHA256",
        )?;
        let wider = env_pin(
            "AIRK_CUDA_FP16_TIMING_WIDER",
            "AIRK_CUDA_FP16_TIMING_WIDER_SHA256",
        )?;
        ensure!(
            short.sha256 == T32_NATIVE.parse()? && wider.sha256 == WIDER_NATIVE.parse()?,
            "exact sealed native score receipts required"
        );
        let mut scores = Vec::new();
        for pin in [&short, &wider] {
            let source: Value = serde_json::from_slice(&pinned(pin, PROOF_BYTES)?)?;
            for row in source["rows"]
                .as_array()
                .context("missing sealed score rows")?
            {
                let (tokens, digest) = [
                    (32, T32_INPUT),
                    (128, DEVELOPMENT_CASES[0].2),
                    (1024, DEVELOPMENT_CASES[1].2),
                    (8192, DEVELOPMENT_CASES[2].2),
                ]
                .get(scores.len())
                .copied()
                .context("extra sealed score row")?;
                ensure!(
                    row["tokens"] == tokens && row["input_digest"] == digest,
                    "sealed input differs"
                );
                let raw: [f32; 8] = serde_json::from_value(row["raw_scores"].clone())?;
                ensure!(
                    raw.iter().all(|s| s.is_finite()),
                    "invalid sealed raw scores"
                );
                scores.push(raw);
            }
        }
        Ok(Self {
            pins: [short, wider],
            scores: scores
                .try_into()
                .map_err(|_| anyhow::anyhow!("missing sealed score row"))?,
        })
    }
}

pub(super) struct Run<'a> {
    pub(super) references: &'a DevelopmentReferences,
    pub(super) short: &'a ReferenceProof,
    pub(super) population: Option<(Population, &'a Sealed)>,
}

pub(super) struct Sample<'a> {
    pub(super) reference: &'a ReferenceProof,
    pub(super) sealed: Option<&'a [f32; 8]>,
    pub(super) cycle: usize,
    pub(super) kind: &'static str,
}

impl<'a> Run<'a> {
    pub(super) fn schedule(&self) -> Vec<Sample<'a>> {
        match self.population {
            None => self
                .references
                .rows
                .iter()
                .map(|reference| Sample {
                    reference,
                    sealed: None,
                    cycle: 0,
                    kind: "development",
                })
                .collect(),
            Some((timing, sealed)) => {
                let [a, b, c] = &self.references.rows;
                let inputs = [self.short, a, b, c];
                timing
                    .schedule()
                    .into_iter()
                    .map(|(index, cycle, kind)| Sample {
                        reference: inputs[index],
                        sealed: Some(&sealed.scores[index]),
                        cycle,
                        kind,
                    })
                    .collect()
            }
        }
    }
}

#[test]
fn compact_fp16_timing_fixes_warm_population_and_mixed_cycles() {
    assert_eq!(Population::Fit.schedule(), [(3, 0, "sample")]);
    assert_eq!(
        Population::Fit.journal_operation(0),
        (operation::Kind::Sample, 1)
    );
    let fixed = Population::Fixed.schedule();
    assert_eq!(fixed[0], (3, 0, "warmup"));
    assert_eq!(fixed[1..], [(3, 0, "sample"); 20]);
    assert_eq!(
        Population::Fixed.journal_operation(0),
        (operation::Kind::Warmup, 0)
    );
    for i in 1..fixed.len() {
        assert_eq!(
            Population::Fixed.journal_operation(i),
            (operation::Kind::Sample, i)
        );
        assert!(i <= 20);
    }
    let mixed = Population::Mixed.schedule();
    assert_eq!(mixed.len(), 15);
    for i in 0..mixed.len() {
        assert_eq!(
            Population::Mixed.journal_operation(i),
            (operation::Kind::Sample, i + 1)
        );
        assert!(i < 20);
    }
    let (cycles, remainder) = mixed.as_chunks::<5>();
    assert!(remainder.is_empty());
    for (i, cycle) in cycles.iter().enumerate() {
        assert_eq!(
            *cycle,
            [
                (0, i + 1, "sample"),
                (1, i + 1, "sample"),
                (2, i + 1, "sample"),
                (3, i + 1, "sample"),
                (0, i + 1, "sample")
            ]
        );
    }
}
