// SPDX-License-Identifier: Apache-2.0
//! Bind the fresh Windows compiler and static aggregate to one verified public emission.
use super::*;

pub(super) struct Fresh {
    pub argv: Value,
    pub source_changes: Vec<Sha256Digest>,
}

pub(super) fn public_options(argv: &Value, compiler: &str) -> Result<Vec<BuildOption>> {
    const PRIVATE_COMPILERS: [&str; 3] = [
        "CMAKE_C_COMPILER",
        "CMAKE_CXX_COMPILER",
        "CMAKE_CUDA_HOST_COMPILER",
    ];
    let mut verified = BTreeSet::new();
    let mut remaining = Vec::new();
    for arg in rows(argv)? {
        if let Some((name, value)) = arg
            .as_str()
            .and_then(|s| s.strip_prefix("-D"))
            .and_then(|s| s.split_once('='))
            && PRIVATE_COMPILERS.contains(&name)
        {
            ensure!(
                value
                    .replace('\\', "/")
                    .eq_ignore_ascii_case(&compiler.replace('\\', "/"))
                    && verified.insert(name),
                "fresh compiler path duplicated or differs from the authenticated tool"
            );
            continue;
        }
        remaining.push(arg.clone());
    }
    ensure!(
        verified.len() == PRIVATE_COMPILERS.len(),
        "missing fresh compiler path"
    );
    options(&Value::Array(remaining))
}

pub(super) fn observe(
    source: &dyn Source,
    input: &Input,
    build: &Value,
    compiled: &Value,
    environment: &Value,
    archive: &Value,
) -> Result<Option<Fresh>> {
    if build["schema"] != "airk.private.cuda-fresh-static-build-proof/1" {
        return Ok(None);
    }
    #[cfg(not(windows))]
    {
        let _ = (source, input, compiled, environment, archive);
        anyhow::bail!("fresh Windows public provenance requires the Windows file owner");
    }
    #[cfg(windows)]
    {
        let validated = fresh::validate_fresh_producer(source, &input.producer_receipt, build)?;
        let evidence = &environment["aggregate"];
        ensure!(
            input.aggregate_proof.sha256.to_string()
                == "05c074577ed80c5f2ac733a5775f71732ff631c890a78932083fddde9a7b4d5c"
                && environment["schema"] == "airk.cuda-static-environment/6"
                && environment["activation_pins_available"] == false
                && environment["build"] == *build
                && validated.static_core_build == input.producer_receipt.sha256
                && evidence["schema"] == "airk.cuda-static-aggregate-proof/1"
                && evidence["target"] == "x86_64-pc-windows-msvc"
                && evidence["qualification"] == false
                && sha(&evidence["producer_proof_sha256"])? == canonical_sha(build)?
                && sha(&evidence["receipt_sha256"])? == input.aggregate_receipt.sha256
                && archive["status"] == "AGGREGATED_UNQUALIFIED"
                && archive["qualification"] == false
                && successful(&archive["command"]["owned"])
                && archive["plan"]["qualification"] == false
                && archive["plan"]["compile_receipt"] == build["build_receipt"]
                && archive["plan"]["producer_proof_sha256"] == evidence["producer_proof_sha256"]
                && canonical_sha(&archive["plan"])? == sha(&evidence["plan_sha256"])?
                && archive["output"] == evidence["output"]
                && archive["coff"] == evidence["coff"]
                && archive["output"]["sha256"]
                    == serde_json::to_value(input.aggregate_archive.sha256)?,
            "fresh Windows static aggregate or producer differs"
        );
        let open = |row: &Value| -> Result<Value> {
            let pin = ResourceFilePin {
                path: row["path"].as_str().context("source path absent")?.into(),
                sha256: sha(&row["sha256"])?,
            };
            json(source, &pin)
        };
        let configured = open(&build["configure_receipt"])?;
        let recipe = open(&build["configure_recipe"])?;
        let pin_in = |rows: &[Value], pin: &ResourceFilePin| {
            rows.iter()
                .filter(|row| {
                    row["path"].as_str() == Some(pin.path.as_str())
                        && row["sha256"].as_str() == Some(pin.sha256.to_string().as_str())
                })
                .count()
                == 1
        };
        let native_inputs = rows(&recipe["native_inputs"])?;
        ensure!(
            canonical_sha(&recipe["native_inputs"])? == sha(&build["native_input_closure_sha256"])?
                && pin_in(native_inputs, &input.cuda_version)
                && pin_in(native_inputs, &input.cuda_inventory)
                && pin_in(native_inputs, &input.cudnn_inventory)
                && build["toolchain"] == compiled["toolchain"]
                && sha(&build["cpu_lock"]["sha256"])? == input.cpu_lock.sha256
                && sha(&build["source_lock"]["sha256"])? == input.source_lock.sha256
                && build["patch_inputs"] == configured["patch_inputs"],
            "fresh source/toolkit inputs differ"
        );
        let changes = rows(&build["patch_inputs"])?
            .iter()
            .map(|patch| sha(&patch["original"]["sha256"]))
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(Fresh {
            argv: configured["configure"]["argv"].clone(),
            source_changes: changes,
        }))
    }
}
