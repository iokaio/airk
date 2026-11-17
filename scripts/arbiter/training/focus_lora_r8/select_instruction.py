# SPDX-License-Identifier: Apache-2.0
"""Pure actual-owned SCOREâ†’fixedDEV choice; never executes a model."""
import argparse,json
from pathlib import Path
from focus_worker import digest
import worker_logic as logic
from workflow import load_owned
def require_private_root(value):
    if not value:
        raise ValueError('explicit private root required')
    root = next(p for p in Path(__file__).resolve().parents if (p/'Cargo.toml').is_file() and (p/'AGENTS.md').is_file())
    private = Path(value).resolve()
    if not private.is_relative_to(root/'artifacts'):
        raise ValueError('private root must remain beneath repository artifacts; tracked or external roots refused')
    return private


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--owned-score-receipt',type=Path,required=True);p.add_argument('--owned-score-receipt-sha256',required=True);p.add_argument('--native-census',type=Path,required=True);p.add_argument('--native-census-sha256',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--execution-root',type=Path,required=True);args=p.parse_args();O=Path(__file__).resolve().parent;E=require_private_root(args.execution_root)
    if not args.output.resolve().is_relative_to(E):raise ValueError('selection output must remain in private execution root')
    import importlib.util,sys
    spec=importlib.util.spec_from_file_location('custody',O/'support/candidate_overlay.py');h=importlib.util.module_from_spec(spec);sys.modules[spec.name]=h;spec.loader.exec_module(h);h.identity()
    if digest(args.native_census)!=args.native_census_sha256 or args.output.exists():raise ValueError('exact actual fresh census/output required')
    score_pin={'path':str(args.owned_score_receipt.resolve()),'sha256':args.owned_score_receipt_sha256};actual=load_owned(score_pin,'score')
    if actual['plan']['native_census']['sha256']!=args.native_census_sha256:raise ValueError('actual SCORE used another native census')
    emission=json.loads(args.native_census.read_bytes());rows=logic.native_rows(emission,json.loads((E/'f2-existing-pairs-closure-r1/all-original1955-roster.json').read_bytes()),json.loads((E/'f2-existing-replay-candidates-r1/qasper-fixed45-draws.json').read_bytes()));report=actual['worker'];choice=logic.choose_instruction(report,rows)
    result={'schema':'airk.private.F1-fixedDEV-choice/2','native_census_sha256':args.native_census_sha256,'score_report':{'path':actual['execution']['worker_result_path'],'sha256':actual['execution']['worker_result_sha256']},'owned_score_receipt':score_pin,'continuity_key':actual['continuity_key'],'choice':choice,'qualification':False,'promotion':False};args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8');print(json.dumps({'path':str(args.output),'sha256':digest(args.output)}))
