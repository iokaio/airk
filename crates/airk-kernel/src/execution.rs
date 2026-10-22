// SPDX-License-Identifier: Apache-2.0
//! Bounded storage-root execution slots shared by indexing processes.
use airk_core::execution::{ExecutionMode, ExecutionPlan};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

#[derive(Debug)]
pub(crate) struct CapacityBusy;
impl std::fmt::Display for CapacityBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("waiting for indexing capacity")
    }
}
impl std::error::Error for CapacityBusy {}

pub(crate) struct ExecutionLease {
    pub plan: ExecutionPlan,
    _files: Vec<File>,
    _lifecycle: File,
}

fn mode() -> Result<ExecutionMode> {
    Ok(match std::env::var("AIRK_EXECUTION_MODE").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("automatic") => ExecutionMode::Automatic,
        Ok("interactive") => ExecutionMode::Interactive,
        Ok("throughput") => ExecutionMode::Throughput,
        _ => anyhow::bail!("AIRK_EXECUTION_MODE must be automatic, interactive or throughput"),
    })
}

pub(crate) fn plan(slots: usize) -> Result<ExecutionPlan> {
    let mode = mode()?;
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    let memory = crate::limits::machine::memory_bytes();
    let cpu_share = cpus
        .saturating_sub(1)
        .saturating_mul(slots)
        .div_ceil(4)
        .max(1);
    let budget = working_budget(mode, memory, slots);
    let requested = |name, default: usize, maximum| -> Result<usize> {
        let value = match std::env::var(name) {
            Ok(value) => value.parse().with_context(|| format!("invalid {name}"))?,
            Err(std::env::VarError::NotPresent) => default,
            Err(error) => return Err(error.into()),
        };
        ensure!(
            (1..=maximum).contains(&value),
            "{name} must be 1..{maximum}"
        );
        Ok(value)
    };
    let interactive = mode == ExecutionMode::Interactive;
    let extraction = requested(
        "AIRK_EXTRACT_WORKERS",
        if interactive { 1 } else { cpu_share.min(4) },
        4,
    )?;
    let inference = requested(
        "AIRK_INDEX_WORKERS",
        if interactive {
            1
        } else {
            cpu_share.min(if mode == ExecutionMode::Throughput {
                8
            } else {
                4
            })
        },
        8,
    )?;
    let extraction_memory = (budget.saturating_sub(airk_ingest::MAX_FILE_BYTES)
        / (2 * airk_ingest::ExtractionExecution::MAX_RESULT_BYTES
            + airk_ingest::ExtractionExecution::MAX_PARALLEL_SOURCE_BYTES
            + (64 << 20)))
        .max(1);
    Ok(ExecutionPlan {
        mode,
        logical_cpus: cpus,
        detected_memory_bytes: memory,
        requested_extraction_workers: extraction,
        extraction_workers: extraction.min(cpu_share).min(extraction_memory),
        requested_inference_workers: inference,
        inference_workers: inference.min(cpu_share),
        working_memory_bytes: budget,
        resource_slots: slots,
        coordination:
            "four advisory slots per storage root; other applications/roots are not measured".into(),
    })
}

fn working_budget(mode: ExecutionMode, memory: Option<u64>, slots: usize) -> usize {
    let throughput = mode == ExecutionMode::Throughput;
    let root_ceiling = if throughput { 8 << 30 } else { 4 << 30 };
    let process_ceiling = if throughput { 4 << 30 } else { 1 << 30 };
    ((memory.unwrap_or(2 << 30) / 8).min(root_ceiling) / 4 * slots as u64)
        .clamp(128 << 20, process_ceiling) as usize
}

pub(crate) fn acquire(root: &Path) -> Result<ExecutionLease> {
    airk_store::quota::ensure_current_root(root)?;
    std::fs::create_dir_all(root)?;
    let lifecycle = airk_store::quota::indexing_lifecycle_lease(root)?;
    airk_store::quota::ensure_current_root(root)?;
    let desired = if mode()? == ExecutionMode::Throughput {
        2
    } else {
        1
    };
    let mut files = Vec::new();
    for slot in 0..4 {
        let path = root.join(format!("execution-1.2-{slot}.lock"));
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "linked execution lock"
            );
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock() {
            Ok(()) => files.push(file),
            Err(std::fs::TryLockError::WouldBlock) => continue,
            Err(error) => return Err(error.into()),
        }
        if files.len() == desired {
            break;
        }
    }
    if files.is_empty() {
        return Err(CapacityBusy.into());
    }
    Ok(ExecutionLease {
        plan: plan(files.len())?,
        _files: files,
        _lifecycle: lifecycle,
    })
}

impl crate::Kernel {
    pub fn execution_status(&self) -> Result<ExecutionPlan> {
        plan(if mode()? == ExecutionMode::Throughput {
            2
        } else {
            1
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throughput_can_admit_parallel_extraction_without_giving_each_process_the_host() {
        let gib = 1u64 << 30;
        assert_eq!(
            working_budget(ExecutionMode::Automatic, Some(128 * gib), 1),
            gib as usize
        );
        assert_eq!(
            working_budget(ExecutionMode::Throughput, Some(128 * gib), 2),
            (4 * gib) as usize
        );
        assert_eq!(
            working_budget(ExecutionMode::Throughput, Some(16 * gib), 2),
            gib as usize
        );
        assert_eq!(
            working_budget(ExecutionMode::Throughput, None, 2),
            (gib / 8) as usize
        );
        let worker = 2 * airk_ingest::ExtractionExecution::MAX_RESULT_BYTES
            + airk_ingest::ExtractionExecution::MAX_PARALLEL_SOURCE_BYTES
            + (64 << 20);
        let budget = working_budget(ExecutionMode::Throughput, Some(128 * gib), 2);
        assert!((budget - airk_ingest::MAX_FILE_BYTES) / worker >= 4);
    }
}
