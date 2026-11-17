# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root = Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
out = Path('D:/code/Github/airk/artifacts/v190')
name = 'crates/airk-cli/src/project_supervisor_guarded_qualification_tests.rs'
old = (root / name).read_text(encoding='utf-8')
new = old.replace('    gate: NativeCatalogAdmission,\n}\n\nimpl CatalogControl for NativeControl {', '    gate: NativeCatalogAdmission,\n    source: Option<crate::source_supervisor::SourceWake>,\n}\n\nimpl CatalogControl for NativeControl {\n    fn fence_source_policy(&self, project: ProjectBinding, operation: Incarnation) -> Result<()> {\n        self.source.as_ref().context("native fixture source owner absent")?\n            .fence_policy(project, operation)\n    }\n    fn release_source_policy(&self, project: ProjectBinding, operation: Incarnation) -> Result<()> {\n        self.source.as_ref().context("native fixture source owner absent")?\n            .release_policy(project, operation)\n    }')
assert new != old
new = new.replace('let control = NativeControl { owner, gate };', 'let control = NativeControl { owner, gate, source: None };')
assert new.count('source: None') == 1
new += '\n#[path = "project_supervisor_split_tests.rs"]\nmod split_tests;\n'
test = (out / 'p10-happy-cutover-test.proposed.rs').read_text(encoding='utf-8')
patch = ''.join(difflib.unified_diff(old.splitlines(keepends=True), new.splitlines(keepends=True), fromfile='a/' + name, tofile='b/' + name))
target = 'crates/airk-cli/src/project_supervisor_split_tests.rs'
patch += ''.join(difflib.unified_diff([], test.splitlines(keepends=True), fromfile='/dev/null', tofile='b/' + target))
cutover_name = 'crates/airk-cli/src/catalog_source_change_control.rs'
cutover_old = (root / cutover_name).read_text(encoding='utf-8')
marker = 'pub(crate) fn split_cutover(\n'
prefix, body = cutover_old.split(marker, 1)
signature_end = ') -> Result<(CatalogRequest, Sha256Digest)> {\n'
signature, rest = body.split(signature_end, 1)
wrapper = marker + signature + signature_end + '    split_cutover_with_engine(registry, control, snapshot, crate::runtime_pair::engine_identity()?)\n}\n\n'
wrapper += '// Private unit fixture executes the actual accepted work in this test process.\n// It cannot attest installed image identity or live provider/process drains.\n#[cfg(test)]\npub(crate) fn split_cutover_for_native_fixture(\n    registry: &mut ServiceRegistry,\n    control: &Path,\n    snapshot: &airk_core::catalog_split::SplitSnapshot,\n    engine: Sha256Digest,\n) -> Result<(CatalogRequest, Sha256Digest)> {\n    split_cutover_with_engine(registry, control, snapshot, engine)\n}\n\n'
wrapper += 'fn split_cutover_with_engine(\n' + signature + '    engine: Sha256Digest,\n' + signature_end
cutover_body, tail = rest.split('\nfn require_missing_with_plain_parent', 1)
assert cutover_body.count('engine_identity: crate::runtime_pair::engine_identity()?') == 1
cutover_body = cutover_body.replace('engine_identity: crate::runtime_pair::engine_identity()?', 'engine_identity: engine')
cutover_new = prefix + wrapper + cutover_body + '\nfn require_missing_with_plain_parent' + tail
patch += ''.join(difflib.unified_diff(cutover_old.splitlines(keepends=True), cutover_new.splitlines(keepends=True), fromfile='a/' + cutover_name, tofile='b/' + cutover_name))
(out / 'p10-happy-cutover.proposed.patch').write_text(patch, encoding='utf-8', newline='\n')
print('Wrote ignored proposed patch; no compiler input changed.')
