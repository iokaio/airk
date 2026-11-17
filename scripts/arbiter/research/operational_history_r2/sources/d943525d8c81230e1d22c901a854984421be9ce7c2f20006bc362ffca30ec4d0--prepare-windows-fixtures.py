# SPDX-License-Identifier: Apache-2.0
# Isolated historical fixture driver: original release bytes, never install/sign.
import datetime,hashlib,json,os,pathlib,queue,stat,subprocess,threading,time,zipfile,platform,uuid,tarfile
ASSETS=pathlib.Path(__file__).resolve().parent
owner=json.loads((ASSETS.parent/'user-private-fixture-base.json').read_text())
BASE=pathlib.Path(owner['path'])/('historical-windows-'+uuid.uuid4().hex)
BASE.mkdir()
PROOF=json.loads((ASSETS/'verified-artifacts.json').read_text())
REPORT={'schema':1,'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'base':str(BASE),'host':platform.node(),'architecture':platform.machine(),'installed':False,'resigned':False,'candidates':[]}
def digest(p):
 with p.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
def write_json(p,v):p.write_text(json.dumps(v,indent=2)+'\n')
def inventory(root):
 out=[]
 for p in sorted(root.rglob('*')):
  assert not p.is_symlink(),p
  if p.is_file():out.append({'path':str(p.relative_to(root)),'bytes':p.stat().st_size,'sha256':digest(p),'mode':oct(stat.S_IMODE(p.stat().st_mode))})
 return out
def run(rec,name,argv,cwd,timeout=40):
 log=cwd/'observations';log.mkdir(exist_ok=True,mode=0o700)
 started=time.monotonic()
 with (log/(name+'.stdout')).open('wb') as out,(log/(name+'.stderr')).open('wb') as err:
  child=subprocess.Popen(argv,cwd=cwd,stdin=subprocess.DEVNULL,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
  timed_out=False
  try:code=child.wait(timeout=timeout)
  except subprocess.TimeoutExpired:
   timed_out=True;child.terminate()
   try:code=child.wait(timeout=5)
   except subprocess.TimeoutExpired:child.kill();code=child.wait(timeout=5)
 rec['commands'].append({'name':name,'argv':argv,'pid':child.pid,'exit_code':code,'timed_out':timed_out,'elapsed_seconds':round(time.monotonic()-started,3),'stdout':str(log/(name+'.stdout')),'stderr':str(log/(name+'.stderr')),'owned_child_reaped':True})
 return code,(log/(name+'.stdout')).read_text(errors='replace')
def stdio(rec,argv,cwd):
 log=cwd/'observations';transcript=[];messages=queue.Queue()
 with (log/'mcp.stderr').open('wb') as err:
  child=subprocess.Popen(argv,cwd=cwd,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
  def reader():
   for line in child.stdout:
    if len(line)>1024*1024:messages.put(RuntimeError('MCP line exceeds 1MiB'));return
    try:messages.put(json.loads(line))
    except Exception as ex:messages.put(ex);return
  thread=threading.Thread(target=reader,daemon=True);thread.start()
  try:
   def exchange(message):
    transcript.append({'sent':message});child.stdin.write((json.dumps(message)+'\n').encode());child.stdin.flush()
    if 'id' not in message:return
    deadline=time.monotonic()+30
    while True:
     reply=messages.get(timeout=max(0.001,deadline-time.monotonic()))
     if isinstance(reply,Exception):raise reply
     transcript.append({'received':reply})
     if reply.get('id')==message['id']:
      assert 'error' not in reply,reply
      return reply
   initialize=exchange({'jsonrpc':'2.0','id':1,'method':'initialize','params':{'protocolVersion':'2025-06-18','capabilities':{},'clientInfo':{'name':'airk-historical-fixture','version':'1'}}})
   exchange({'jsonrpc':'2.0','method':'notifications/initialized'})
   response=exchange({'jsonrpc':'2.0','id':2,'method':'tools/call','params':{'name':'airk_status','arguments':{}}})
   assert not response['result'].get('isError'),response
   rec['mcp_status']=response
   rec['negotiated_protocol']=initialize['result']['protocolVersion']
  finally:
   child.stdin.close()
   try:code=child.wait(timeout=10)
   except subprocess.TimeoutExpired:
    child.terminate()
    try:code=child.wait(timeout=5)
    except subprocess.TimeoutExpired:child.kill();code=child.wait(timeout=5)
   thread.join(timeout=2)
   rec['mcp_process']={'pid':child.pid,'exit_code':code,'owned_child_reaped':True,'reader_joined':not thread.is_alive()}
   write_json(log/'mcp-transcript.json',transcript)
for proof in PROOF['candidates']:
 tag=proof['tag'];version=proof['package_version'];case=BASE/tag;case.mkdir(mode=0o700)
 rec={'tag':tag,'version':version,'commit':proof['commit'],'layout':proof['layout'],'commands':[]}
 REPORT['candidates'].append(rec)
 try:
  asset=next(a for a in proof['assets'] if a['name'].endswith('.vsix'));vsix=ASSETS/tag/asset['name'];assert digest(vsix)==asset['sha256']
  extension=case/'extension';extension.mkdir(mode=0o700)
  with zipfile.ZipFile(vsix) as z:
   for item in z.infolist():
    path=pathlib.PurePosixPath(item.filename)
    assert not path.is_absolute() and '..' not in path.parts
    if not item.filename.startswith('extension/native/'):continue
    mode=(item.external_attr>>16)&0xffff
    assert not stat.S_ISLNK(mode)
    destination=case/path
    if item.is_dir():destination.mkdir(parents=True,exist_ok=True,mode=0o700);continue
    destination.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    destination.write_bytes(z.read(item));os.chmod(destination,stat.S_IMODE(mode) or 0o600)
   manifest=json.loads((extension/'native/manifest.json').read_text())
   assert manifest['commit']==proof['commit'] and manifest['target']=='x86_64-pc-windows-msvc'
   for rel,expected in manifest['payload'].items():assert digest(extension/'native'/rel)==expected
   rec['native_inventory']=inventory(extension/'native')
   # Reproduce historical prepareHelper exactly: version/platform/digest cache;
   # original implementation writes only verified executable bytes with 0755.
   helper=case/'User/globalStorage/iokaio.airk/helpers'/f"{version}-win32-x64-{manifest['payload']['airk.exe'][:16]}"/'airk.exe'
   helper.parent.mkdir(parents=True,mode=0o700);helper.write_bytes((extension/'native/airk.exe').read_bytes());os.chmod(helper,0o755)
   assert digest(helper)==proof['native']['sha256']
   rec['helper']={'path':str(helper),'sha256':digest(helper),'mode':oct(stat.S_IMODE(helper.stat().st_mode)),'grammar_source':f'{tag}:packages/vscode/src/native.ts:prepareHelper','byte_identical_to_release':True}
  signature_command="Get-AuthenticodeSignature -LiteralPath '"+str(helper).replace("'","''")+"' | Select-Object Status,@{n='Subject';e={$_.SignerCertificate.Subject}},@{n='Thumbprint';e={$_.SignerCertificate.Thumbprint}} | ConvertTo-Json -Compress"
  code,signature=run(rec,'authenticode-verify',['pwsh.exe','-NoProfile','-NonInteractive','-Command',signature_command],case)
  assert code==0
  rec['authenticode']=json.loads(signature)
  assert rec['authenticode']['Status']==0 and 'Ioka LLC' in rec['authenticode']['Subject'],rec['authenticode']
  code,text=run(rec,'version',[str(helper),'--version'],case);assert code==0 and text.strip()==f'airk {version}',text
  code,help_text=run(rec,'help',[str(helper),'--help'],case);assert code==0
  assert '--root' in help_text and '--data-dir' in help_text and '--collection' in help_text
  code,_=run(rec,'index-help',[str(helper),'index','--help'],case);assert code==0
  code,_=run(rec,'mcp-help',[str(helper),'mcp','--help'],case);assert code==0
  source=case/'workspace';source.mkdir(mode=0o700)
  (source/'README.md').write_text('# Fictional harbor\nThe cobalt harbor accepts the fictional copper token.\n')
  (source/'demo.py').write_text('def fictional_token_valid(token):\n    return token == "copper-token"\n')
  private=case/'private-data';private.mkdir(mode=0o700);collection='historical-fixture'
  prefix=[str(helper),'--root',str(source),'--data-dir',str(private),'--collection',collection]
  rec['source_before']=inventory(source)
  code,_=run(rec,'index',prefix+['index'],case,180);assert code==0,'actual old index failed'
  code,status=run(rec,'status',prefix+['status'],case);assert code==0
  rec['status']=json.loads(status);rec['source']=str(source);rec['data_root']=str(private);rec['collection']=collection
  assert (private/proof['layout']/collection).is_dir(),'expected original collection layout missing'
  # Exact historical configFor('codex') rendering, stored ONLY in this fixture.
  config=case/'User/config.toml';config_text=f'[mcp_servers.airk-{collection}]\ncommand = '+json.dumps(str(helper),separators=(',',':'))+'\nargs = '+json.dumps(prefix[1:]+['mcp'],separators=(',',':'))+'\n'
  config.write_text(config_text)
  rec['original_config']={'path':str(config),'sha256':digest(config),'bytes':config.stat().st_size,'renderer_source':f'{tag}:packages/vscode/src/model.ts:configFor','actual_stdio_argv':prefix+['mcp'],'installed_to_client':False}
  stdio(rec,prefix+['mcp'],case)
  rec['source_after']=inventory(source);assert rec['source_before']==rec['source_after']
  rec['saved_data_inventory']=inventory(private)
  rec['native_and_stdio_publication_equal']=json.loads(rec['mcp_status']['result']['content'][0]['text'])['publication']==rec['status']['publication']
  assert rec['native_and_stdio_publication_equal']
  archive=case/'saved-index-original.tar'
  with tarfile.open(archive,'x') as tar:
   for name in ['private-data','workspace','User/config.toml']:tar.add(case/name,arcname=name,recursive=True)
  rec['saved_index_archive']={'path':str(archive),'bytes':archive.stat().st_size,'sha256':digest(archive)}
  rec['outcome']='created'
 except Exception as ex:
  rec['outcome']='failed';rec['error']=type(ex).__name__+': '+str(ex)
 finally:
  write_json(case/'receipt.json',rec);write_json(BASE/'campaign.json',REPORT);write_json(ASSETS/'campaign.json',REPORT)
  print(json.dumps({'tag':tag,'outcome':rec['outcome'],'error':rec.get('error')}),flush=True)
REPORT['finished_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
write_json(BASE/'campaign.json',REPORT)
write_json(ASSETS/'campaign.json',REPORT)
