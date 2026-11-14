// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn split_schema3_catalog_engine_identity_verifies_the_paired_cli() -> Result<()> {
    let owner = tempfile::tempdir()?;
    let root = owner.path().canonicalize()?;
    crate::runtime_install::tests::split_pair_fixture(&root)?;
    let main = root.join(main_name());
    let expected = runtime_pair::engine_identity_at_version(&main, env!("CARGO_PKG_VERSION"))?;
    assert_eq!(engine_identity(&main)?, expected);
    let cli = if cfg!(windows) {
        "airkctl.exe"
    } else if cfg!(target_os = "macos") {
        "AIRK.app/Contents/MacOS/airkctl"
    } else {
        "airkctl"
    };
    fs::write(root.join(cli), b"modified split CLI")?;
    assert!(
        engine_identity(&main).is_err(),
        "a retained main digest alone cannot authorize a changed CLI"
    );
    Ok(())
}
