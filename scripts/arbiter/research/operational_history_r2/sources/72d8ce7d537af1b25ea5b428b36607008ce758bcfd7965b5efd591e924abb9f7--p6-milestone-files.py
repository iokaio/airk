# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,subprocess
ROOT=Path(__file__).resolve().parents[2]
status=subprocess.check_output(['git','-c','safe.directory=D:/code/Github/airk','status','--porcelain=v1','--untracked-files=all'],cwd=ROOT,text=True)
changed={line[3:].strip('"') for line in status.splitlines()}
explicit='''docs/split/inventory/extension.md
packages/vscode/scripts/build-explorer.cjs
packages/vscode/src/adminUiContract.ts
packages/vscode/src/adminUiHost.ts
packages/vscode/src/airkctl/adminChunks.ts
packages/vscode/src/airkctl/session.ts
packages/vscode/src/explorerContracts.ts
packages/vscode/src/explorerController.ts
packages/vscode/src/explorerEvidence.ts
packages/vscode/src/explorerManagement.ts
packages/vscode/src/uiPilotContract.ts
packages/vscode/src/uiPilotHost.ts
packages/vscode/src/workbenchContracts.ts
packages/vscode/src/workbenchRender.ts
packages/vscode/src/workbenchView.ts
packages/vscode/src/workbenchWebview/main.tsx
packages/vscode/src/test/adminUiController.test.ts
packages/vscode/src/test/explorerBoundaries.test.ts
packages/vscode/src/test/explorerHelp.test.ts
packages/vscode/src/test/workbench.test.ts
packages/vscode/src/test/workbenchClientLifetime.test.ts
scripts/dev/contract_types.py
scripts/tests/test_contract_types.py'''.splitlines()
frontend={p for p in changed if p.startswith(('packages/admin/','packages/ui/','packages/vscode/src/explorerWebview/'))}|(set(explicit)&changed)
generated={p for p in changed if p.startswith(('packages/contracts-gen/','packages/vscode/src/generated/','crates/airk-cli/admin-assets/'))}
shared='''packages/vscode/src/airkctl/features/explorerHost.ts
packages/vscode/src/airkctl/features/liveStatus.ts
packages/vscode/src/test/explorerLiveStatus.test.ts
packages/vscode/src/serviceExtension.ts
packages/vscode/src/extensionUpdateLoader.ts
packages/vscode/src/test/extensionUpdateLoader.test.ts
scripts/package-extension.py
scripts/tests/test_package_extension.py'''.splitlines()
native_dependencies={p for p in changed if p.startswith(('fixtures/split/ui_pilot/','packages/vscode/src/explorerHelp/'))}|({'crates/airk-mcp/src/help/content.json','crates/airk-mcp/src/explorer_templates.json','fixtures/split/arbiter/public_answer_cases.json'}&changed)
result={'status':'Exact refreshed review list only; no staging/commit. Picker followup89affected tests/two typechecks and fresh native-capture16semantic tests/three typechecks/contract identity passed. Producer escaped-string correction3byte/negative proofs and2actual-generated-host DOM cases passed; rebuilt JS has no physical trailing whitespace. Native Help/manifest string bytes unchanged, Admin exact rebuild and package consumers passed. Earlier436+10+89receipts retain original identities.',
        'frontend_owned_exact':sorted(frontend),'generated_contracts_and_current_assets_exact':sorted(generated),
        'shared_requires_primary_owner_coordination_exact':sorted(set(shared)&changed),
        'native_help_and_parity_dependency_exact':sorted(native_dependencies),
        'version_coupling_exact':['Cargo.toml','Cargo.lock','packages/vscode/package.json','packages/vscode/package-lock.json'],
        'version_recommendation':'Commit all four in the root native integration milestone with native dependency and P7 source owners; frontend milestone follows that coherent version/contract/content baseline. Do not split version-only hunks from dependency-bearing manifests/locks.',
        'excluded':'No artifacts/temp/private state, compiled packages/vscode/out, unrelated native Rust/CI/site/installer files or package-extension shared changes are authorized for child staging by this list.',
        'asset_warning':'Paths below reflect the final reviewed producer outputs after the picker guard rebuild. Include explicit old-asset deletions and new hashed assets atomically; compiled VSCode out/ stays excluded.'}
target=ROOT/'artifacts/advanced-split/p6-milestone-files.json';target.write_text(json.dumps(result,indent=2)+'\n')
print({key:len(value) for key,value in result.items() if isinstance(value,list)})
