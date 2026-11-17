# SPDX-License-Identifier: Apache-2.0
"""Root-only exact first after-state custody, no build/receipt rewrite."""
import argparse,copy,hashlib,importlib.util,json,shutil
from pathlib import Path
HERE=Path(__file__).resolve().parent
def digest(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(p,n):
 s=importlib.util.spec_from_file_location(n,p);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
def main():
 ap=argparse.ArgumentParser();ap.add_argument('--first-receipt',type=Path,required=True);ap.add_argument('--first-receipt-sha256',required=True);ap.add_argument('--execute-capture',action='store_true');a=ap.parse_args()
 if not a.execute_capture or digest(a.first_receipt)!=a.first_receipt_sha256:raise ValueError('exact actual first receipt/capture intent required')
 inputs=json.loads((HERE/'input-contract.json').read_bytes());r12=Path(inputs['r12_proposal']);request=json.loads((r12/'continuation-request.json').read_bytes());r8=load(request['recipe']['path'],'r8_after_custody');lock=json.loads(Path(request['recipe_lock']['path']).read_bytes());o=r8.cpu_owner(lock);helper=load(HERE/'run_checkpoint_sections.py','section_after_custody');first=o.read_json(a.first_receipt);helper.verify_section(first,0,o,'6bee6b8b95a01fea28adf04117260d12503012ea8120ae225afa232b6589df97');out=r8.PARENT/'cu8-checkpoint-first-after-custody-r1';guard=copy.deepcopy(lock);guard['build_path']['fresh_child']=out.name;policy=r8.identity_and_parent(o,out,guard);out.mkdir();rows=[]
 for kind in ['ninja_log','ninja_deps']:
  original=first[kind+'_after'];target=out/(kind+'-after');observed=helper.copy_state(o.plain(original['path'],regular=True),target,original);rows.append({'kind':kind,'status':'COPIED_EXACT_RECORDED_AFTER','original_pin':original,'copy':observed})
 receipt={'schema':'airk.private.cuda-first-state-custody/1','first_receipt':{'path':str(a.first_receipt),'bytes':a.first_receipt.stat().st_size,'sha256':a.first_receipt_sha256},'phase_receipt_unchanged':True,'output_policy':policy,'rows':rows}
 r8.write(out/'receipt.json',receipt);print(json.dumps({'output':str(out),'status':'EXACT_FIRST_AFTER_COPIED_NO_BUILD'}))
if __name__=='__main__':main()
