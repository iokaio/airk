# SPDX-License-Identifier: Apache-2.0
"""Candidate card custody/preflight only. No model imports or execution dispatch."""
import argparse,json
from pathlib import Path
import admission as a

def main():
    p=argparse.ArgumentParser();p.add_argument('--card',type=Path,required=True);p.add_argument('--stage',choices=['inspect','preflight'],default='inspect');args=p.parse_args()
    E=Path(__file__).resolve().parents[1]
    import importlib.util,sys
    spec=importlib.util.spec_from_file_location('custody',E/'candidate-overlay-preparation-r1/candidate_overlay.py');h=importlib.util.module_from_spec(spec);sys.modules[spec.name]=h;spec.loader.exec_module(h);h.identity()
    card=json.loads(args.card.read_bytes())
    for key in ['roles','cohort','native_F0_examples','native_binding']:
        pin=card[key];raw=Path(pin['path']).read_bytes()
        if len(raw)!=pin['bytes']or a.digest(raw)!=pin['sha256']:raise ValueError('frozen '+key+' bytes changed')
    print(json.dumps({'scope':card['scope'],'card_sha256':a.digest(args.card.read_bytes()),'custody_verified':True,'model_calls':False,'gradient_steps':0,'promotion':False}))
    if args.stage=='preflight':a.require_execution_bindings(card)
if __name__=='__main__':main()
