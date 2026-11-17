# SPDX-License-Identifier: Apache-2.0
"""Pure post-score DEV choice; no model or prompt construction."""
import argparse,json
from pathlib import Path
from focus_worker import digest
import worker_logic as logic
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--score-report',type=Path,required=True);p.add_argument('--score-report-sha256',required=True);p.add_argument('--native-census',type=Path,required=True);p.add_argument('--native-census-sha256',required=True);p.add_argument('--output',type=Path,required=True);args=p.parse_args();O=Path(__file__).resolve().parent;E=O.parent
    import importlib.util,sys
    spec=importlib.util.spec_from_file_location('custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(spec);sys.modules[spec.name]=h;spec.loader.exec_module(h);h.identity()
    if digest(args.score_report)!=args.score_report_sha256 or digest(args.native_census)!=args.native_census_sha256 or args.output.exists():raise ValueError('exact original fresh score/census/output required')
    emission=json.loads(args.native_census.read_bytes());rows=logic.native_rows(emission,json.loads((E/'f2-existing-pairs-closure-r1/all-original1955-roster.json').read_bytes()),json.loads((E/'f2-existing-replay-candidates-r1/qasper-fixed45-draws.json').read_bytes()));report=json.loads(args.score_report.read_bytes())
    choice=logic.choose_instruction(report,rows)
    if report['native_census_sha256']!=args.native_census_sha256:raise ValueError('score did not consume this native census')
    result={'schema':'airk.private.F1-fixedDEV-choice/1','native_census_sha256':args.native_census_sha256,'score_report':{'path':str(args.score_report.resolve()),'sha256':args.score_report_sha256},'choice':choice,'qualification':False,'promotion':False};args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8');print(json.dumps({'path':str(args.output),'sha256':digest(args.output)}))
