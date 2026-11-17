# SPDX-License-Identifier: Apache-2.0
# Create-new additive binding of unchanged independent reference data only.
# This command is PREPARED, not executed. No model, native or Torch imports.
import argparse,copy,hashlib,json,pathlib

def load(path):return json.loads(pathlib.Path(path).read_bytes())
def sha(raw):return hashlib.sha256(raw).hexdigest()
def main():
 p=argparse.ArgumentParser();p.add_argument('--audit',type=pathlib.Path,required=True);p.add_argument('--audit-sha256',required=True);p.add_argument('--output',type=pathlib.Path,required=True);args=p.parse_args()
 assert sha(args.audit.read_bytes())==args.audit_sha256,'reviewed audit differs';audit=load(args.audit)
 assert audit['qualification'] is False and audit['missing_rows']==[] and audit['reference_rows']==18
 for pin in audit['pinned_audit_inputs']:
  path=pathlib.Path(pin['path']);assert path.stat().st_size==pin['bytes'] and sha(path.read_bytes())==pin['sha256'],'audit source changed: '+str(path)
 # Source/output account and approved directory access are owned by the existing
 # same-human execution path. Never change an ACL or reuse an existing output.
 output=args.output.resolve();approved=pathlib.Path(__file__).resolve().parent
 assert output.parent==approved and not output.exists() and not output.is_symlink(),'fresh direct private output required'
 documents=[]
 for model in audit['models']:
  assert model['binding_allowed'] and model['byte_identical_template'] and model['input_digest_equal_no_normalization']
  source=load(model['current_input']['path']);assert source['numeric_fixture'] is None
  fixture=pathlib.Path(model['retained_numeric_fixture']['path']);assembly=load(fixture.with_name('assembly.json'))
  binding=copy.deepcopy(assembly['fixture']) # Preserve the original native pin shape verbatim.
  assert binding['path']==str(fixture) and binding['sha256']==model['retained_numeric_fixture']['sha256']
  result=copy.deepcopy(source);result['numeric_fixture']=binding
  check=copy.deepcopy(result);check['numeric_fixture']=None;assert check==source,'unrelated native input mutation'
  documents.append((model['model'],result))
 output.mkdir(exist_ok=False);rows=[]
 for name,value in documents:
  path=output/(name+'.resource-run-input.private.json');raw=(json.dumps(value,indent=2)+'\n').encode('utf-8')
  with path.open('xb') as handle:handle.write(raw)
  rows.append({'model':name,'input':{'path':str(path),'bytes':len(raw),'sha256':sha(raw)},'only_native_field_changed':'numeric_fixture','reference_producer_or_fixture_modified':False})
 receipt={'audit':{'path':str(args.audit.resolve()),'sha256':args.audit_sha256},'bindings':rows,'model_native_Torch_GPU_executions':0,'qualification':False,'old_resource_task_CAL_numeric_verdicts_transferred':False}
 with (output/'binding.json').open('x',encoding='utf-8') as handle:json.dump(receipt,handle,indent=2);handle.write('\n')
 print(json.dumps(receipt,indent=2))
if __name__=='__main__':main()
