// SPDX-License-Identifier: Apache-2.0
//! Scoped scoring callback for one authenticated Large CAL phase.
use super::super::model::{CalSession, validate_cal_input};
use super::{CompiledInput, Instant, Result, Value, ensure, json};

pub(super) struct Phase<'a, 'callback> {
    pub(super) graph: &'a str,
    pub(super) collect: &'a mut CalSession<'callback>,
}

impl Phase<'_, '_> {
    pub(super) fn run(
        self,
        deadline: Instant,
        mut score: impl FnMut(&CompiledInput) -> Result<Vec<f32>>,
    ) -> Result<Value> {
        let mut settled = true;
        let mut native_forwards = 0;
        let observation = (self.collect)(&mut |input, expires| {
            let ready = std::mem::replace(&mut settled, false);
            ensure!(
                ready && native_forwards < 8 && expires <= deadline && Instant::now() < expires,
                "Large CAL original phase or request window exhausted"
            );
            validate_cal_input(input)?;
            let scores = score(input)?;
            native_forwards += 1;
            ensure!(
                Instant::now() < expires,
                "original Large CAL forward expired"
            );
            settled = true;
            Ok(scores)
        })?;
        ensure!(
            settled && native_forwards > 0,
            "Large CAL has no complete synchronized forwards"
        );
        Ok(json!({"observation":observation,"native_forwards":native_forwards}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::identity::Sha256Digest;
    use std::time::Duration;

    #[test]
    fn large_cal_refuses_more_than_one_phase_even_if_collector_ignores_error() -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let input = CompiledInput {
            prompt: String::new(),
            input_ids: vec![1],
            attention_mask: vec![1],
            position_ids: vec![0],
            decision_position: 0,
            candidate_positions: vec![32],
            option_mask: vec![true],
            input_digest: Sha256Digest::from_bytes([0; 32]),
        };
        let mut collect = |score: &mut dyn FnMut(&CompiledInput, Instant) -> Result<Vec<f32>>| {
            for _ in 0..8 {
                assert_eq!(score(&input, deadline)?, vec![1.0]);
            }
            assert!(score(&input, deadline).is_err());
            Ok(Value::Null)
        };
        let mut forwards = 0;
        let result = Phase {
            graph: "",
            collect: &mut collect,
        }
        .run(deadline, |_| {
            forwards += 1;
            Ok(vec![1.0])
        });
        assert!(result.is_err());
        assert_eq!(forwards, 8);
        Ok(())
    }
}
