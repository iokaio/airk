// SPDX-License-Identifier: Apache-2.0
//! Execution admission is independent of extraction interpretation and reuse identity.
use airk_ingest::ExtractionExecution;
use anyhow::{Context, Result, ensure};

#[derive(Clone, Copy)]
pub(crate) struct ExtractionAdmission {
    pub requested: usize,
    pub execution: ExtractionExecution,
}

pub(crate) fn configured_extraction_execution() -> Result<ExtractionAdmission> {
    let requested = match std::env::var("AIRK_EXTRACT_WORKERS") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => return Err(error).context("invalid AIRK_EXTRACT_WORKERS"),
    };
    admit(requested.as_deref(), super::machine::memory_bytes())
}

fn admit(requested: Option<&str>, memory: Option<u64>) -> Result<ExtractionAdmission> {
    // Keep serial as the default until workload qualification selects otherwise.
    let requested = requested
        .map(str::parse::<usize>)
        .transpose()
        .context("AIRK_EXTRACT_WORKERS must be an integer from 1 through 4")?
        .unwrap_or(1);
    ensure!(
        (1..=4).contains(&requested),
        "AIRK_EXTRACT_WORKERS must be from 1 through 4"
    );
    // This is an admission heuristic, not an RSS cap. In addition to bounded
    // serialized results and their transient validation encoding, reserve room
    // for input/parser work and the coordinator's
    // one protected read. Existing serial per-file limits still apply on small or
    // unknown machines; native expansion is always dispatched serially.
    let mib = 1024 * 1024;
    let available = memory
        .filter(|bytes| *bytes > 0)
        .map(|bytes| (bytes / 8).min(4 * 1024 * mib))
        .unwrap_or(512 * mib);
    let worker = 2 * ExtractionExecution::MAX_RESULT_BYTES as u64
        + ExtractionExecution::MAX_PARALLEL_SOURCE_BYTES as u64
        + 64 * mib;
    let admitted = (available.saturating_sub(airk_ingest::MAX_FILE_BYTES as u64) / worker)
        .clamp(1, 4) as usize;
    let workers = requested.min(admitted);
    let execution = ExtractionExecution {
        workers,
        max_in_flight_source_bytes: workers * ExtractionExecution::MAX_PARALLEL_SOURCE_BYTES,
    };
    execution.validate()?;
    Ok(ExtractionAdmission {
        requested,
        execution,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_parallelism_is_capped_by_memory_and_never_changes_interpretation() -> Result<()> {
        let gib = 1024 * 1024 * 1024;
        assert_eq!(admit(None, Some(64 * gib))?.execution.workers, 1);
        for memory in [None, Some(0), Some(2 * gib)] {
            let plan = admit(Some("4"), memory)?;
            assert_eq!((plan.requested, plan.execution.workers), (4, 1));
        }
        assert_eq!(admit(Some("4"), Some(8 * gib))?.execution.workers, 1);
        let plan = admit(Some("4"), Some(16 * gib))?;
        assert_eq!(plan.execution.workers, 3);
        assert_eq!(plan.execution.max_in_flight_source_bytes, 12 * 1024 * 1024);
        assert_eq!(admit(Some("4"), Some(32 * gib))?.execution.workers, 4);
        assert_eq!(admit(Some("4"), Some(64 * gib))?.execution.workers, 4);
        assert_eq!(admit(Some("2"), Some(64 * gib))?.execution.workers, 2);
        for invalid in ["", "0", "5", "-1", "several", "2.5"] {
            assert!(admit(Some(invalid), Some(64 * gib)).is_err());
        }
        Ok(())
    }
}
