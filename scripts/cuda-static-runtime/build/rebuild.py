# SPDX-License-Identifier: Apache-2.0
"""Validate explicit new-run inputs read-only. This command cannot launch a build."""
import argparse,datetime,hashlib,json,re
from pathlib import Path
STAGES=['r8','r12','flash','llm','final','compose','aggregate']
FIELDS={'schema','repository_root','campaign_cutoff_utc','stages','inputs','output_directories','qualification'}
def validate_manifest(value,repository,now):
 if set(value)!=FIELDS or value['schema']!='airk.cuda-parent-rebuild-input/1' or value['qualification'] is not False:raise ValueError('exact nonqualifying rebuild input manifest required')
 root=Path(value['repository_root'])
 if not root.is_absolute() or root!=Path(repository):raise ValueError('exact repository root required')
 cutoff=datetime.datetime.fromisoformat(value['campaign_cutoff_utc'])
 if cutoff.utcoffset()!=datetime.timedelta(0) or now.utcoffset()!=datetime.timedelta(0) or cutoff<=now:raise ValueError('explicit future UTC new-run cutoff required; historical deadlines are not extended')
 stages=value['stages']
 if not isinstance(stages,list) or [s.get('name') for s in stages]!=STAGES:raise ValueError('exact ordered R8/R12/checkpoint/composition/aggregate stages required')
 def pin(row):
  if set(row)!={'path','bytes','sha256'} or not Path(row['path']).is_absolute() or type(row['bytes']) is not int or row['bytes']<=0 or not isinstance(row['sha256'],str) or not re.fullmatch('[0-9a-f]{64}',row['sha256']):raise ValueError('exact absolute file pin required')
 for stage in stages:
  if set(stage)!={'name','source','input_contract','argv'} or not isinstance(stage['argv'],list) or not stage['argv'] or any(not isinstance(x,str) or not x or '\x00' in x for x in stage['argv']):raise ValueError('exact explicit stage command/source/input required')
  pin(stage['source']);pin(stage['input_contract'])
  source=Path(stage['source']['path'])
  if '..' in source.parts or not source.is_relative_to(root/'scripts'):raise ValueError('rebuild implementation must be maintained source, never ignored artifacts')
 if not isinstance(value['inputs'],list) or not 1<=len(value['inputs'])<=16380:raise ValueError('bounded explicit input closure required')
 for row in value['inputs']:pin(row)
 outputs=value['output_directories']
 if not isinstance(outputs,list) or not outputs or len(outputs)>32 or len(set(outputs))!=len(outputs):raise ValueError('bounded distinct proposed outputs required')
 for output in outputs:
  if not Path(output).is_absolute() or '..' in Path(output).parts:raise ValueError('canonical absolute output proposal required')
 return {'status':'VALID_MANIFEST_STRUCTURE_ONLY','execution_authorized':False,'qualification':False,'stage_count':len(stages),'input_count':len(value['inputs']),'fresh_run_launcher_available':False}
def check_pin(row):
 path=Path(row['path'])
 if path.resolve(strict=True)!=path or path.is_symlink() or not path.is_file():raise ValueError('canonical regular input required')
 with path.open('rb') as stream:digest=hashlib.file_digest(stream,'sha256').hexdigest()
 if path.stat().st_size!=row['bytes'] or digest!=row['sha256']:raise ValueError('rebuild input bytes differ: '+str(path))
def main():
 parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--input-manifest',type=Path,required=True);parser.add_argument('--manifest-sha256',required=True);args=parser.parse_args()
 with args.input_manifest.open('rb') as stream:raw=stream.read((4<<20)+1)
 if len(raw)>4<<20 or hashlib.sha256(raw).hexdigest()!=args.manifest_sha256:raise ValueError('bounded reviewed rebuild manifest pin required')
 value=json.loads(raw);result=validate_manifest(value,Path(__file__).resolve().parents[3],datetime.datetime.now(datetime.timezone.utc))
 for stage in value['stages']:
  check_pin(stage['source']);check_pin(stage['input_contract'])
 for row in value['inputs']:check_pin(row)
 if any(Path(p).exists() for p in value['output_directories']):raise ValueError('fresh proposed output directories required')
 print(json.dumps({**result,'all_declared_input_pins_match':True,'manifest_sha256':args.manifest_sha256,'outputs_created':False}))
if __name__=='__main__':main()
