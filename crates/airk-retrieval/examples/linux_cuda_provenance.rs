// SPDX-License-Identifier: Apache-2.0
//! Emit public Linux CUDA provenance from the seven original build/notice receipts.
#[cfg(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    feature = "test-support",
    feature = "gpu-cuda"
))]
fn main() -> anyhow::Result<()> {
    use anyhow::{Context, ensure};
    use std::io::{Read, Write};

    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() == 1 && arguments[0] == "--payload-names" {
        for name in airk_retrieval::arbiter::runtime::cuda::linux_cuda_payload_names() {
            writeln!(std::io::stdout().lock(), "{name}")?;
        }
        return Ok(());
    }
    if arguments.len() == 3 && arguments[0] == "--empty-product" {
        let file = std::fs::File::open(&arguments[1])?;
        ensure!(
            file.metadata()?.is_file(),
            "expected the original scientific manifest"
        );
        let mut bytes = Vec::new();
        file.take((128 << 10) + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 128 << 10,
            "scientific manifest exceeds bound"
        );
        let expected = arguments[2]
            .to_str()
            .context("expected scientific SHA-256")?
            .parse()?;
        let output =
            airk_retrieval::arbiter::runtime::cuda::encode_empty_product(&bytes, expected)?;
        std::io::stdout().lock().write_all(&output)?;
        return Ok(());
    }
    ensure!(
        arguments.len() == 7,
        "expected aggregate, provider, configure, bootstrap, ORT archive, NVIDIA archive and original notice receipts"
    );
    let mut rows = Vec::with_capacity(7);
    for path in arguments {
        let file = std::fs::File::open(&path).with_context(|| format!("open {path:?}"))?;
        ensure!(file.metadata()?.is_file(), "receipt must be a regular file");
        let mut bytes = Vec::new();
        file.take((8 << 20) + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 8 << 20, "receipt exceeds bound");
        rows.push(bytes);
    }
    let inputs: [Vec<u8>; 7] = rows
        .try_into()
        .map_err(|_| anyhow::anyhow!("receipt count"))?;
    let output = airk_retrieval::arbiter::runtime::cuda::encode_build_provenance(
        inputs.each_ref().map(Vec::as_slice),
    )?;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}

#[cfg(not(all(
    target_os = "linux",
    target_arch = "x86_64",
    target_env = "gnu",
    feature = "test-support",
    feature = "gpu-cuda"
)))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("Linux x86-64 GNU and the gpu-cuda and test-support features are required")
}
