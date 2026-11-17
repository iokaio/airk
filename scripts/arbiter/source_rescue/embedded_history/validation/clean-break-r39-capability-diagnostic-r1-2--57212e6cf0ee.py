# SPDX-License-Identifier: Apache-2.0
import json,subprocess,sys
commands=[['python','scripts/dev-check.py','-p','airk-mcp','--features','airk-kernel/local-inference','--skip-fmt'],[r'C:\Users\tdotn\.cargo\bin\cargo.exe','test','-p','airk-mcp','--lib','--no-default-features','--features','airk-kernel/local-inference','--locked','--timings','actual_combined_capabilities_and_parse_fit_their_public_body_budgets','--','--nocapture'],[r'C:\Program Files\nodejs\node.exe','--test','--test-name-pattern=actual native refusal frame is retained before','scripts/tests/native-editor-workbench.test.cjs']]
codes=[]
for i,command in enumerate(commands):
 if i==1 and codes[0]:
  print('Rust diagnostic skipped because scoped Clippy failed',flush=True);codes.append(None);continue
 print(json.dumps({'command':command}),flush=True)
 codes.append(subprocess.run(command).returncode)
print(json.dumps({'diagnostic_exit_codes':codes,'expected':'Clippy passes; each diagnostic selects exactly one red test'}),flush=True)
sys.exit(1 if any(code for code in codes if code is not None) else 0)