# SPDX-License-Identifier: Apache-2.0
import hashlib,json,re,xml.etree.ElementTree as ET
from pathlib import Path
base=Path(__file__).resolve().parent
source={'sha256':'979568465e27b37bc82538ca91e6f5ef843f20886270679ba7da1f42e40e53fc','files':948}
receipt_path=base/'p11a-final-workspace-tests-r2.receipt.json'
receipt=json.loads(receipt_path.read_text())
assert receipt['actual_exit_code']==receipt['exit_code']==100
assert receipt['source_before']==receipt['source_after']==source and receipt['source_unchanged']
log=base/'p11a-final-workspace-tests-r2.log'
assert hashlib.sha256(log.read_bytes()).hexdigest()==receipt['log_sha256']
assert re.search(r'2572 tests run: 2571 passed, 1 failed, 53 skipped',log.read_text())
junit=base/'p10-worktree/target/nextest/ci/junit.xml'
cases=ET.parse(junit).getroot().findall('.//testcase')
failed=[c for c in cases if c.find('failure') is not None or c.find('error') is not None]
assert len(cases)==2572 and len(failed)==1
expected='cli_reference_tests::cli_reference_documents_every_public_command'
assert failed[0].attrib['name']==expected
repair=base/'p11a-cli-reference-verify-r1.receipt.json'
checked=json.loads(repair.read_text())
assert checked['exit_code']==checked['actual_exit_code']==0 and checked['passing_tests']==1
assert checked['source_before']==checked['source_after']==source and checked['source_unchanged']
assert expected in checked['command'] and '--exact' in checked['command']
assert hashlib.sha256((base/'p11a-cli-reference-verify-r1.log').read_bytes()).hexdigest()==checked['log_sha256']
output=base/'p11a-workspace-doc-resolution-r1.json';assert not output.exists()
output.write_text(json.dumps({'status':'sole_documentation_failure_resolved_by_focused_normal_check',
 'source':source,'original_suite':{'cases':2572,'passed':2571,'failed':1,'skipped':53,'seconds':1243.263,
 'failed_name':expected,'receipt':str(receipt_path),'log_sha256':receipt['log_sha256'],'junit_sha256':hashlib.sha256(junit.read_bytes()).hexdigest()},
 'repair':{'receipt':str(repair),'log_sha256':checked['log_sha256'],'passing_tests':1,
 'cli_reference_sha256':hashlib.sha256((base/'p10-worktree/docs/cli-reference.md').read_bytes()).hexdigest()},
 'scope':'All selected workspace cases have passing evidence after documentation-only generation and exact one-case normal verification. Original failed run retained; full workspace not rerun. Four documented long campaigns and49ignored cases remain skipped here; P14 restores required campaigns.'},indent=2)+'\n')
print('Workspace census collected:2571pass/1docfailure/53skip; sole failure corrected and ordinary exact verification passed')
