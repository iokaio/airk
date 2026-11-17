# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
root=Path(__file__).resolve().parents[2]
for name in ('README.md','packages/vscode/README.md','docs/README.md','docs/releases/README.md','docs/releases/release-1.8.23.md'):
 p=root/name
 s=p.read_text(encoding='utf-8')
 pairs=[
 ('candidate preparation — build in progress','Windows debug manual-test candidate'),
 ('source synchronized, candidate build in progress.','Windows debug manual-test candidate built and package-verified.'),
 ('No 1.8.23 native package or VSIX has been built, installed or qualified.','The Windows debug runtime, native ZIP and VSIX have been built and package-verified. Installed-editor acceptance and full release qualification remain pending.'),
 ('No 1.8.23 native package or VSIX has been\nbuilt or qualified.','The Windows debug runtime, native ZIP and VSIX have been built and package-verified.'),
 ('No 1.8.23 native package or VSIX has been built or\nqualified.','The Windows debug runtime, native ZIP and VSIX have been built and package-verified.'),
 ('catalog, build and artifact checks are in progress.','scoped native and extension checks and package verification passed. Full release qualification remains pending.'),
 ('generated catalog, compilation, packaging and artifact checks are in progress.','Generated CLI reference, compilation, packaging and artifact checks passed for this Windows debug candidate.'),
 ('records reproductions and source validation. Package verification,\ninstalled-editor acceptance','records reproductions and source validation. Installed-editor acceptance'),
 ('candidate outcomes. Package verification, installed-editor acceptance','candidate outcomes. Installed-editor acceptance'),
 ('Candidate build\nand installed qualification','Installed qualification'),
 ('candidate preparation and pending package and installed\nacceptance scopes','the Windows debug candidate and pending installed acceptance scopes'),
 ('its candidate build is in progress.','its Windows debug candidate is built and package-verified.'),
 ('the candidate build is in progress and no native package or VSIX is complete yet.','the Windows debug native package and VSIX are built and package-verified.'),
 ('the candidate build is in progress.','the Windows debug candidate is built and package-verified.'),
 ('candidate compilation and packaging are in progress, while its native package and CI qualification remain pending.','Windows debug compilation and packaging passed; CI and release qualification remain pending.'),
 ('the build is in progress. Native package, VSIX, installed-editor and platform qualification are pending.','the Windows debug native package and VSIX are built and package-verified. Installed-editor and platform qualification remain pending.'),
 ('Source checks are not qualification of this pending artifact.','Scoped checks and package verification do not establish installed-editor or full release qualification.'),
 ('Remaining gates include the synchronized version check, matching native/VSIX\nbuild identities, package verification, installed-editor empty-install and','Version synchronization and matching native/VSIX package identities passed. Remaining gates include installed-editor empty-install and'),
 ]
 for old,new in pairs:s=s.replace(old,new)
 p.write_text(s,encoding='utf-8',newline='\n')
ledger=root/'docs/simplify/one-process-ledger.md'
s=ledger.read_text(encoding='utf-8').replace('**Correction implemented; scoped validation in progress.**','**Windows debug manual-test installer built; installed-editor acceptance pending.**',1)
note='''
### Bounded Windows installer handoff (10 November 2026)

The user set a 30-minute delivery limit. The delivered candidate uses the debug
runtime, not an optimized release runtime. No production installation was changed.
The native ZIP and VSIX package checks passed with unchanged consumed inputs:
`artifacts/one-process/installer-bounded-build-r1.json`. Cargo used the persistent
`artifacts/v190/p10-target`, incremental compilation enabled, with timing receipts;
the final debug runtime compilation took 24.62 seconds. The build sequence also
passed the one-test ordinary-reopen regression and one-test generated CLI reference.

Validation before packaging: scoped native Clippy passed; the native batch selected
54 library tests (53 passed; the remaining ordinary-reopen defect was corrected and
its one-test rerun passed), and both actual CLI lifecycle tests passed. The full
mocked extension batch selected 1,425 tests: 1,404 passed, zero failed, 21 gated skips.
Receipts: `installer-bounded-native-validation-r1.json`,
`installer-bounded-native-tests-r1.json`, `installer-bounded-extension-tests-r1.json`.
The final native UI selection has its separate receipt
`installer-bounded-native-ui-r1.json`; report its actual outcome separately.

Full current-source workspace tests, optimized builds, installed-editor acceptance,
macOS/Linux, signing and release publication were not completed for this handoff.
The pre-existing structural boundary failure and earlier failed receipts remain
recorded below; they have not been waived or relabeled as passing. This is an
unpublished Windows manual-test candidate. AGENTS.md and CLAUDE.md now contain the
same bounded investigation, scope and stopping rules and remain byte-identical.

'''
anchor='Original observed evidence (collected before tests or production changes):'
s=s.replace(anchor,note+anchor,1)
ledger.write_text(s,encoding='utf-8',newline='\n')
