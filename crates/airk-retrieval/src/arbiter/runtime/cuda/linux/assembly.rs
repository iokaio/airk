// SPDX-License-Identifier: Apache-2.0
//! Read-only scientific descriptor assembly. No library handle or GPU call.
use super::*;
use airk_core::arbiter::{Asset, AssetRole};
use manifest::linux::{self as schema, Descriptor, ElfFile};

fn validate_input(input: &RuntimeAssemblyInput) -> Result<()> {
    ensure!(
        input.producer_name == "airk-linux-cuda-build-provenance.json"
            && input.producer_sha256 != Sha256Digest::from_bytes([0; 32])
            && input.directory.is_absolute(),
        "invalid Linux CUDA producer input"
    );
    ensure!(
        input.checkpointed_proof.is_none()
            && input.incremental_proof.is_none()
            && input.custody_copies.is_empty()
            && input.historical_tool_copies.is_empty(),
        "Linux assembly cannot consume Windows provenance or custody copies"
    );
    let modules: BTreeSet<_> = input.module_names.iter().collect();
    let ordered: BTreeSet<_> = input.load_order.iter().collect();
    ensure!(
        (2..=64).contains(&modules.len())
            && input.module_names.len() == modules.len()
            && input.load_order.len() == modules.len()
            && ordered == modules
            && input.module_names.iter().all(|n| schema::payload_name(n))
            && (1..=64).contains(&input.system_imports.len())
            && input.system_imports.iter().all(|n| schema::system_name(n)),
        "invalid Linux CUDA assembly module roster or order"
    );
    Ok(())
}

fn inventory(
    input: &RuntimeAssemblyInput,
    host: bool,
    held: &mut Vec<Held>,
    budget: &mut u64,
    deadline: Instant,
) -> Result<Vec<ElfFile>> {
    let names: Vec<_> = if host {
        input.system_imports.iter().collect()
    } else {
        input.module_names.iter().collect()
    };
    let mut files = Vec::with_capacity(names.len());
    for name in names {
        in_time(deadline)?;
        let path = if host {
            Held::system_path(name)?
        } else {
            input.directory.join(name)
        };
        let mut object = Held::open(&path, host, (*budget).min(2_u64 << 30))?;
        let row = object.inspect(name, host, deadline)?;
        *budget = budget
            .checked_sub(row.bytes)
            .context("Linux ELF assembly byte budget")?;
        files.push(row);
        held.push(object);
    }
    Ok(files)
}

pub(in super::super) fn observe_assembly(
    input: RuntimeAssemblyInput,
    deadline: Instant,
) -> Result<Vec<u8>> {
    validate_input(&input)?;
    let mut proof = Held::open(&input.directory.join(&input.producer_name), false, 1 << 20)?;
    let raw = proof.metadata_bytes(deadline)?;
    ensure!(
        proof.sha == input.producer_sha256,
        "Linux CUDA provenance SHA differs from assembly input"
    );
    let build = schema::provenance_build(&raw)?;
    let asset = Asset {
        name: input.producer_name.clone(),
        role: AssetRole::Notice,
        bytes: raw.len() as u64,
        sha256: proof.sha,
    };
    let mut held = vec![proof];
    let mut budget = 8_u64 << 30;
    let files = inventory(&input, false, &mut held, &mut budget, deadline)?;
    let system = inventory(&input, true, &mut held, &mut budget, deadline)?;
    let bytes = schema::assemble_scientific(build, asset, files, input.load_order, system)?;
    let descriptor = Descriptor::parse(&bytes)?;
    for notice in descriptor.verify_provenance(&raw)? {
        in_time(deadline)?;
        let mut file = Held::open(
            &input.directory.join(&notice.name),
            false,
            notice.bytes.min(16 << 20),
        )?;
        let contents = file.metadata_bytes(deadline)?;
        ensure!(
            contents.len() as u64 == notice.bytes && file.sha == notice.sha256,
            "held Linux CUDA legal notice differs"
        );
        held.push(file);
    }
    for file in &held {
        file.recheck(deadline)?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> RuntimeAssemblyInput {
        let modules: Vec<_> = [
            "libonnxruntime_providers_shared.so",
            "libonnxruntime_providers_cuda.so",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        RuntimeAssemblyInput {
            directory: "/absent-airk-linux-assembly-control".into(),
            producer_name: "airk-linux-cuda-build-provenance.json".into(),
            producer_sha256: Sha256Digest::from_bytes([3; 32]),
            checkpointed_proof: None,
            incremental_proof: None,
            custody_copies: vec![],
            historical_tool_copies: vec![],
            load_order: modules.clone(),
            module_names: modules,
            system_imports: BTreeSet::from(["libcuda.so.1".into()]),
        }
    }
    #[test]
    fn checks_native_caller_rosters_before_opening_files() -> Result<()> {
        validate_input(&request())?;
        let mut traversal = request();
        traversal.module_names[0] = "../foreign.so".into();
        traversal.load_order = traversal.module_names.clone();
        assert!(validate_input(&traversal).is_err());
        let mut duplicate = request();
        duplicate.load_order[1] = duplicate.load_order[0].clone();
        assert!(validate_input(&duplicate).is_err());
        let mut host = request();
        host.system_imports.insert("libunapproved.so".into());
        assert!(validate_input(&host).is_err());
        Ok(())
    }
}
