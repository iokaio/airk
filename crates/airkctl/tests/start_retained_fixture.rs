// SPDX-License-Identifier: Apache-2.0
//! Start the real native candidate pair inside a checked Windows campaign fixture.
#![cfg(windows)]

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs::File, io, path::Path};

fn verified_image(path: &Path, expected_variable: &str) -> Result<()> {
    let expected = std::env::var(expected_variable)?;
    ensure!(expected.len() == 64, "invalid expected image digest");
    let mut digest = Sha256::new();
    io::copy(&mut File::open(path)?, &mut digest)?;
    ensure!(
        format!("{:x}", digest.finalize()) == expected,
        "native image differs"
    );
    Ok(())
}

#[test]
#[ignore = "requires a stopped owner-verified Windows fixture and staged matched images"]
fn start_retained_fixture_pair() -> Result<()> {
    let private = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts")
        .canonicalize()?;
    let control = std::path::PathBuf::from(
        std::env::var_os("AIRK_TEST_FIXTURE_CONTROL").context("missing control root")?,
    )
    .canonicalize()?;
    let here = std::env::current_exe()?.canonicalize()?;
    let pair = here.parent().context("staged image lacks parent")?;
    ensure!(
        control.starts_with(&private)
            && pair.starts_with(&private)
            && control != private
            && control != pair
            && !pair.starts_with(&control)
            && !control.starts_with(pair),
        "private staging or control root differs"
    );
    let airk = pair.join("airk.exe");
    verified_image(&airk, "AIRK_TEST_FIXTURE_MAIN_SHA256")?;
    verified_image(&pair.join("airkctl.exe"), "AIRK_TEST_FIXTURE_CLI_SHA256")?;
    let start = airkctl::start::start(&airk, &control, env!("CARGO_PKG_VERSION"))
        .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))?;
    ensure!(start.started, "fixture already running");
    println!(
        "AIRK_FIXTURE_HEALTH {}",
        serde_json::to_string(&start.health)?
    );
    Ok(())
}
