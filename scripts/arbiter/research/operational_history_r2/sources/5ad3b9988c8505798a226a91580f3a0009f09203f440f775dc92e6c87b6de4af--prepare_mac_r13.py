# SPDX-License-Identifier: Apache-2.0
import ast,hashlib,json,re
from pathlib import Path
root=Path.cwd();old=root/'artifacts/advanced-split/mac-cli-await-r12';p=root/'artifacts/advanced-split/mac-cli-await-r13';p.mkdir(exist_ok=False)
v=json.loads((old/'plan.json').read_bytes());seal=v['current_source_set_sha256'];baton='mac-cli-await-'+seal[:16]
for n in ['source.json','run.sh','observe.sh','fetch.sh','invoke.py','transport_worker.py']:
 data=(old/n).read_bytes()
 if n=='source.json':(p/n).write_bytes(data);continue
 s=data.decode().replace('mac-cli-await-r12','mac-cli-await-r13')
 # source-r12 is APPLIED and MUST remain r12; hyphenated source names were not replaced above.
 if n=='run.sh':
  matches=re.findall(r'mac-cli-await-[a-f0-9]{16}',s);assert len(matches)==1,matches
  s=s.replace(matches[0],baton).replace('# R12 TEST-ONLY;','# R13 RUN-ONLY;')
 if n=='invoke.py':
  s=s.replace("choices=('source','observe','run','fetch')","choices=('observe','run','fetch')")
  marker="manifest_raw=(PREPARED_DIRECTORY/'source.json').read_bytes();"
  check="run_text=(PREPARED_DIRECTORY/'run.sh').read_text();guards=__import__('re').findall(r'\\[ \\\"\\$\\{AIRK_CARGO_OWNER_BATON:-\\}\\\" = \\\"(mac-cli-await-[a-f0-9]{16})\\\" \\]',run_text);assert guards==['mac-cli-await-'+args.expected_seal[:16]],('Emitted Cargo baton differs from run guard',guards)\n"
  # Simpler exact guard string assertion, not a regex whose escaping can drift.
  check="expected_baton='mac-cli-await-'+args.expected_seal[:16]\nassert ('[ \\\"${AIRK_CARGO_OWNER_BATON:-}\\\" = \\\"'+expected_baton+'\\\" ]') in (PREPARED_DIRECTORY/'run.sh').read_text(),'Emitted Cargo baton differs from run guard'\n".replace('\\"','"')
  assert marker in s;s=s.replace(marker,check+marker)
 (p/n).write_text(s,encoding='utf-8',newline='\n')
# Verify actual independently parsed guard matches actual invocation expression.
run=(p/'run.sh').read_text();guard=re.search(r'\[ "\$\{AIRK_CARGO_OWNER_BATON:-\}" = "([^"]+)" \]',run).group(1);assert guard==baton
invoke=(p/'invoke.py').read_text();ast.parse(invoke)
# Execute the exact new preflight guard in isolation, no transport imports or side effects.
start=invoke.index("expected_baton=");end=invoke.index('manifest_raw=',start)
class Args:expected_seal=seal
exec(invoke[start:end],{'args':Args(),'PREPARED_DIRECTORY':p})
# A mismatched baton must fail cheaply BEFORE any SSH/owned transport.
class Bad:expected_seal='0'*64
try:exec(invoke[start:end],{'args':Bad(),'PREPARED_DIRECTORY':p})
except AssertionError:pass
else:raise AssertionError('Baton mismatch accepted')
checks=[]
for n in ['run.sh','observe.sh','fetch.sh']:
 fragments=re.findall(r"<<'PY'\n(.*?)\nPY(?:\n|$)",(p/n).read_text(),re.S)
 for fragment in fragments:ast.parse(fragment)
 checks.append({'script':n,'python_fragments':len(fragments)})
ast.parse((p/'transport_worker.py').read_text())
assert 'mac-cli-await-clippy ' not in run and run.count('cargo nextest run --profile ci')==1
# Command and all independent result assertions remain identical to R12.
a=(old/'run.sh').read_text();assert a[a.index('"$platform_python" scripts/dev/receipt_runner.py'):]==run[run.index('"$platform_python" scripts/dev/receipt_runner.py'):]
v.update(status='Frozen RUN-ONLY R13 against already APPLIED c940/source-r12; original R12 pre-native exit29 retained',base_source_set_sha256=seal,base_remote_manifest='artifacts/advanced-split/receipts/mac-cli-await-source-r12.json',base_manifest_sha256=v['current_manifest_sha256'],transfer_files={},native_files_changed=0,source_apply_required=False)
v['prepared_scripts']={n:hashlib.sha256((p/n).read_bytes()).hexdigest() for n in ['run.sh','observe.sh','fetch.sh','invoke.py','transport_worker.py']}
v['local_checks'].update(baton_guard_matches_actual_emit=True,wrong_baton_refuses_before_transport=True,python_syntax=checks,remote_execution=False)
(p/'plan.json').write_text(json.dumps(v,indent=2)+'\n',encoding='utf-8',newline='\n')
(p/'README.txt').write_text(f'''ROOT-ONLY R13 RUN-ONLY. Already APPLIED source-r12 c940 native/harness seal {seal}; manifest {v['current_manifest_sha256']}. No source apply; no Clippy rerun. Original R12 exit29 and all transport inputs/results remain unchanged.
Actual emitted Cargo baton {baton} equals run.sh guard; private invoke preflight asserts this before transport. Positive match and negative mismatch isolated checks passed, Python syntax passed. Same47Nextest/34literal assertions and physical/compiler/retention/clean safeguards; fresh remote receipts/mac-cli-await-r13.
 python artifacts/advanced-split/mac-cli-await-r13/invoke.py observe --expected-seal {seal}
 python artifacts/advanced-split/mac-cli-await-r13/invoke.py run --expected-seal {seal}
 python artifacts/advanced-split/mac-cli-await-r13/invoke.py fetch --expected-seal {seal}
No SSH/Cargo/modelbytes by preparer.
''',encoding='utf-8',newline='\n')
print(json.dumps({'path':str(p),'baton':baton,'plan_sha256':hashlib.sha256((p/'plan.json').read_bytes()).hexdigest(),'invoke_sha256':v['prepared_scripts']['invoke.py'],'syntax':checks}))
