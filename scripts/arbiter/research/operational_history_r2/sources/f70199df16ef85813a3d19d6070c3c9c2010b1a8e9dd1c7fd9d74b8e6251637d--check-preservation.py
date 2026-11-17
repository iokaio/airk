# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json,re
p=Path(__file__).resolve().parent
r=p.parents[2]
manifest=json.loads((p/'BASE.json').read_text())
checks={}
for n in ['crates/airk-cli/tests/integrity_restart.rs','crates/airk-kernel/src/diagnostics.rs','crates/airk-cli/src/embedding_pool_client.rs','crates/airk-cli/src/project_protocol.rs','crates/airk-cli/src/project_supervisor.rs']:
    checks[n]= (p/'source'/n).read_bytes()==(r/n).read_bytes()
assert all(checks.values()),checks
n='crates/airk-store/src/service_collection_tests.rs'
pat=re.compile(r'(?m)^(?:#\[[^\n]*\]\n)*fn (\w+)[\s\S]*?^}\n')
current={m.group(1):m.group() for m in pat.finditer((r/n).read_text(encoding='utf-8'))}
result={m.group(1):m.group() for m in pat.finditer((p/'source'/n).read_text(encoding='utf-8'))}
changed=[name for name,text in current.items() if result.get(name)!=text]
expected={'installed_data_root_rename_preserves_publication_settings_pause_and_receipts','integrity_replacement_preserves_damage_and_refuses_retired_head','integrity_abandoned_replacement_recovery_keeps_original_observation','integrity_sidecar_does_not_strand_owned_purge_or_certify_full_reset'}
assert set(changed)==expected,changed
for n,item in manifest['files'].items():
    assert hashlib.sha256((r/n).read_bytes()).hexdigest()==item['sha256'] if 'sha256'in item else not (r/n).exists(),n
(p/'PRESERVATION.json').write_text(json.dumps({'kind':'exact source comparisons, not compilation/test qualification','whole_files_exact_live':checks,'original_collection_functions':len(current),'original_functions_exact':len(current)-len(changed),'changed_existing_functions_only_inert_recovery_lifecycle_seed':changed,'all_raw_base_preimages_still_match':True},indent=2)+'\n',encoding='utf-8',newline='\n')
print('preservation comparisons passed')
