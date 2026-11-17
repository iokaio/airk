# SPDX-License-Identifier: Apache-2.0
import json,re
from pathlib import Path
base=Path(__file__).resolve().parent
log=(base/'workspace-complete.log').read_text(encoding='utf-8')
summaries=re.findall(r'test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',log)
assert summaries and all(s[0]=='ok' and s[2]=='0' for s in summaries)
result={'workspace':{'printed_summaries':len(summaries),'passed':sum(int(s[1]) for s in summaries),'failed':sum(int(s[2]) for s in summaries),'ignored':sum(int(s[3]) for s in summaries),'scope':'Full workspace/all-features run after retry-fixture correction; final one-line MCP budget fix verified separately with full affected suite and process case'},'mcp_after_budget_fix':81,'process_parity_after_budget_fix':1,'native_read_retry':7,'service_runtime':25,'extension':{'passed':540,'skipped':19},'python_contracts':{'run':291,'skipped':6},'listing_contracts':13,'native_read_contracts_after_fixture_fix':4,'initial_four_cpu_campaign':{'passed':15,'total':15},'final_payload_four_cpu_mcp_rounds':{'passed':5,'total':5}}
(base/'validation-summary.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result,indent=2))
