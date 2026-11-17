# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
b=Path(__file__).resolve().parent
live=b/'p10-worktree/crates/airk-cli/tests/catalog_upgrade.rs'
s=live.read_text()
old='fn fixture() -> Result<tempfile::TempDir> {'
assert s.count(old)==1
s=s.replace(old,'''// TempDir owns cleanup; only a newly created child is a native control root.
// Do not repair permissions on existing caller/shared/user directories.
struct PrivateFixture {
    _owner: tempfile::TempDir,
    control: PathBuf,
}
impl PrivateFixture {
    fn path(&self) -> &Path {
        &self.control
    }
}

fn fixture() -> Result<PrivateFixture> {''')
old='''fn private_fixture() -> Result<tempfile::TempDir> {
    Ok(match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-catalog-upgrade-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    })
}'''
new='''fn private_fixture() -> Result<PrivateFixture> {
    let owner = match std::env::var_os("AIRK_PRIVATE_CENSUS_BASE") {
        Some(base) => tempfile::Builder::new()
            .prefix("airk-catalog-upgrade-")
            .tempdir_in(base)?,
        None => tempfile::tempdir()?,
    };
    let control = owner.path().join("native-control");
    ensure!(!control.exists(), "native fixture control child already exists");
    airk_cli::local_ipc::prepare_root(&control)?;
    Ok(PrivateFixture { _owner: owner, control })
}'''
assert s.count(old)==1
s=s.replace(old,new)
old='''    for path in [&control, &source, &data] {
        fs::create_dir(path)?;
    }'''
new='''    airk_cli::local_ipc::prepare_root(&control)?;
    for path in [&source, &data] {
        fs::create_dir(path)?;
    }'''
assert s.count(old)==1
s=s.replace(old,new)
old='''    for crossed in [false, true] {
        let root = fixture()?;
        let owner = CatalogUpgrade::acquire(root.path())?;'''
new='''    for crossed in [false, true] {
        let root = fixture()?;
        // Validate the same existing native owner boundary as serve before
        // testing upgrade/payload refusal; an ACL setup error must not mask it.
        airk_cli::local_ipc::prepare_root(root.path())
            .context("catalog fixture must satisfy native owner-only control admission")?;
        let owner = CatalogUpgrade::acquire(root.path())?;'''
assert s.count(old)==1
s=s.replace(old,new)
(b/'catalog-upgrade-owned-fixture.proposed.rs').write_text(s,newline='\n')
print(json.dumps({'base_sha256':hashlib.sha256(live.read_bytes()).hexdigest()}))
