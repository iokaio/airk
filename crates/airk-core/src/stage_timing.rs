// SPDX-License-Identifier: Apache-2.0
//! Opt-in wall-clock accumulators for indexing measurements.
//!
//! Call sites cost nothing unless the `stage-timing` feature is enabled: the
//! guard is then a zero-sized value and [`snapshot`] returns nothing. The
//! feature is for measurement builds only and adds no persisted or wire shape.
//! Nested scopes are each counted in full, so a parent stage includes its
//! children; subtract them when an exclusive figure is needed.

/// Adds the time between its creation and its drop to one named stage.
#[must_use = "a stage scope measures until it is dropped"]
pub struct Scope {
    #[cfg(feature = "stage-timing")]
    stage: &'static str,
    #[cfg(feature = "stage-timing")]
    started: std::time::Instant,
}

/// Start measuring `stage`.
#[inline]
pub fn scope(stage: &'static str) -> Scope {
    #[cfg(feature = "stage-timing")]
    {
        Scope {
            stage,
            started: std::time::Instant::now(),
        }
    }
    #[cfg(not(feature = "stage-timing"))]
    {
        let _ = stage;
        Scope {}
    }
}

/// One stage's totals since the last [`reset`].
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct StageTotal {
    pub stage: String,
    pub calls: u64,
    pub nanoseconds: u64,
}

#[cfg(feature = "stage-timing")]
static TOTALS: std::sync::Mutex<std::collections::BTreeMap<&'static str, (u64, u64)>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

#[cfg(feature = "stage-timing")]
impl Drop for Scope {
    fn drop(&mut self) {
        let elapsed = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        // A poisoned table only loses measurements; it never fails measured work.
        if let Ok(mut totals) = TOTALS.lock() {
            let entry = totals.entry(self.stage).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(1);
            entry.1 = entry.1.saturating_add(elapsed);
        }
    }
}

/// Totals for every stage seen, ordered by name. Empty without the feature.
pub fn snapshot() -> Vec<StageTotal> {
    #[cfg(feature = "stage-timing")]
    {
        TOTALS
            .lock()
            .map(|totals| {
                totals
                    .iter()
                    .map(|(stage, (calls, nanoseconds))| StageTotal {
                        stage: (*stage).to_owned(),
                        calls: *calls,
                        nanoseconds: *nanoseconds,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(not(feature = "stage-timing"))]
    {
        Vec::new()
    }
}

/// Forget all totals.
pub fn reset() {
    #[cfg(feature = "stage-timing")]
    if let Ok(mut totals) = TOTALS.lock() {
        totals.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_accumulate_only_with_the_feature() {
        drop(scope("stage-timing-self-test"));
        let seen = snapshot()
            .into_iter()
            .find(|total| total.stage == "stage-timing-self-test");
        #[cfg(feature = "stage-timing")]
        assert!(seen.is_some_and(|total| total.calls >= 1));
        #[cfg(not(feature = "stage-timing"))]
        assert!(seen.is_none() && snapshot().is_empty());
    }
}
