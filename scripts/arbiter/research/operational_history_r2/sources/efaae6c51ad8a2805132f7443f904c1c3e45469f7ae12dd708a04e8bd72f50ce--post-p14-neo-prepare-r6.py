# SPDX-License-Identifier: Apache-2.0
"""Prepare a fresh, reviewed post-P14 portable batch only. Never SSH or Cargo."""
import argparse,ast,hashlib,importlib.util,json,os,re,subprocess,tarfile
from pathlib import Path
BASE=Path(__file__).resolve().parent
CATALOG=BASE/'post-p14-first-pass-qualification-r6/command-catalog.json'
SEED_PLAN=BASE/'p14-native-quota-r1-plan.json'
RUNNER=BASE/'p11-native-20261106-r1-runner.py'
OWNER=BASE/'post-p14-neo-owner-r3.py'
LAUNCHER=BASE/'post-p14-neo-launch-r3.py'
CONNECTION=BASE/'neo-connection.local.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def read(p):return json.loads(Path(p).read_text(encoding='utf-8-sig'))
def save(p,value):
    with Path(p).open('x',encoding='utf-8') as out:json.dump(value,out,indent=2);out.write('\n')
def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--label',required=True)
    p.add_argument('--integration-manifest',type=Path,required=True)
    p.add_argument('--integration-manifest-sha256',required=True)
    p.add_argument('--p14-acceptance',type=Path,required=True)
    p.add_argument('--p14-acceptance-sha256',required=True)
    p.add_argument('--catalog-sha256',required=True)
    args=p.parse_args()
    assert re.fullmatch(r'post-p14-native-[a-z0-9-]+',args.label)
    for expected in [args.integration_manifest_sha256,args.p14_acceptance_sha256,args.catalog_sha256]:assert re.fullmatch('[0-9a-f]{64}',expected)
    assert sha(args.integration_manifest)==args.integration_manifest_sha256
    integrated=read(args.integration_manifest)
    assert integrated['status']=='reviewed_integrated_source' and integrated['reviewed'] is True
    work=Path(integrated['source_root']).resolve(strict=True)
    assert work==Path(integrated['source_root']) and (work/'Cargo.toml').is_file()
    expected=integrated['native_source']
    assert set(expected)=={'sha256','files'} and re.fullmatch('[0-9a-f]{64}',expected['sha256']) and expected['files']>0
    native_inputs=integrated['native_inputs']
    assert len(native_inputs)==expected['files']
    assert sha(args.p14_acceptance)==args.p14_acceptance_sha256
    accepted=read(args.p14_acceptance)
    assert accepted['phase']=='P14' and accepted['status']=='accepted', 'Future batch refuses before P14 acceptance'
    assert accepted['native_source']=={'sha256':'237f896933ad59a4075f9395fd53fcb92e376946faf1fc01036c139fc0533c0f','files':960}, 'Require the actual lifecycle-admission P14 source checkpoint'
    assert sha(CATALOG)==args.catalog_sha256
    catalog=read(CATALOG)
    assert catalog['counts']['mac_portable_new_rust']==30 and catalog['counts']['mac_selected_total']==33
    assert catalog['counts']['mac_existing_related']==2 and catalog['counts']['mac_p14_related']==1
    assert sha(OWNER)=='bc7a85cadca6dc4c4ed2deaedb5656c504220175539160a266fbb4bbc2c7cf08'
    assert sha(LAUNCHER)=='997b16a962c50af443fea2d7c15615bc33fe3da3509c109e05a731c422ff5a15'
    connection=read(CONNECTION)
    assert connection['host']=='192.168.68.61', 'Use the existing reserved Neo SSH configuration'
    assert connection['user'] and connection['identity_path'] and connection['ssh_options']
    assert len(catalog['new_rust_tests'])==31 and sum(not row['windows_only'] for row in catalog['new_rust_tests'])==30
    steps=[{'name':'fmt','command':['cargo','fmt','--all','--','--check']}]
    for i,row in enumerate(catalog['mac_steps'],1):
        command=row['command'];assert command[:2]==['cargo','test']
        assert all(token in command for token in ['--locked','--offline','--timings','--all-features','--test-threads=1'])
        assert row['expected']>0
        steps.append({'name':'portable-'+str(i)+'-'+row['package'], 'command':command,'expected_passing_tests':row['expected']})
    assert len(steps)==13 and sum(row.get('expected_passing_tests',0) for row in steps)==33
    seed=read(SEED_PLAN)
    assert seed['runner_arguments'][0]=='--fresh' and '--cache' not in seed['runner_arguments']
    plan={'phase':'PostP14Portable','working_version':'1.8.8','draft':False,'pending':[],
        'native_source':expected,'steps':steps,'runner_arguments':seed['runner_arguments'],
        'not_run':['Windows-only DELETE-sharing-denial case and predecessor process harness excluded, not passed.',
            'No optimized producer/signing/install repeat, full Mac workspace, Intel/Linux/editor/performance qualification.',
            '30 new portable cases plus2 existing related and1 P14 related case only; expected nonzero counts required for every selection.']}
    suffixes=['.tar.gz','-snapshot.json','-plan.json','-runner.py','-head.commit','-head.tree','-prepared.json','-active.json']
    assert all(not(BASE/(args.label+s)).exists() for s in suffixes),'Preserve previous campaign evidence'
    # Explicit full native input map is supplied by the root after final integration.
    def verify_native():
        identity=hashlib.sha256()
        for name,value in sorted(native_inputs.items()):
            assert not Path(name).is_absolute() and '..' not in Path(name).parts
            path=work/name;assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(work)
            assert sha(path)==value,name
            raw=name.encode();identity.update(len(raw).to_bytes(8,'big'));identity.update(raw);identity.update(bytes.fromhex(value))
        assert {'sha256':identity.hexdigest(),'files':len(native_inputs)}==expected
    verify_native()
    for row in [*catalog['new_rust_tests'],*catalog['p14_related_rust_tests']]:
        assert row['file'] in native_inputs,'Test input absent from declared native freeze'
        text=(work/row['file']).read_text(encoding='utf-8')
        assert len(re.findall(r'\bfn\s+'+re.escape(row['name'])+r'\s*\(',text))==1, row['name']
    env=dict(os.environ,GIT_CONFIG_COUNT='1',GIT_CONFIG_KEY_0='safe.directory',GIT_CONFIG_VALUE_0=work.as_posix())
    git=['git','-c','safe.directory='+work.as_posix()]
    names=subprocess.check_output([*git,'ls-files','-z','--cached','--others','--exclude-standard'],cwd=work,env=env).split(b'\0')
    excluded={'.git','.codex','.agents','.aws','.mcp.json','.ssh','temp','target','artifacts','node_modules','__pycache__'}
    names=sorted({os.fsdecode(name) for name in names if name and not(set(Path(os.fsdecode(name)).parts)&excluded)})
    assert set(native_inputs)<=set(names)
    archive=BASE/(args.label+'.tar.gz');public={}
    with tarfile.open(archive,'x:gz') as output:
        for name in names:
            path=work/name;assert path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(work)
            public[name]=sha(path);output.add(path,arcname=name,recursive=False)
    verify_native();assert all(sha(work/name)==digest for name,digest in public.items())
    runner=RUNNER.read_text(encoding='utf-8').replace('P11a','PostP14Portable').replace('p11a.neo-qualification','post-p14.neo-qualification').replace('p11-native-','post-p14-native-')
    anchor="    environment.pop('AIRK_TEST_BINARY', None)";assert runner.count(anchor)==1
    runner=runner.replace(anchor,anchor+"\n    environment.pop('AIRK_CATALOG_CANDIDATE_BINARY', None)\n    environment.pop('AIRK_P10_RESOURCE_OUTPUT_DIR', None)\n    environment.pop('AIRK_P11B_RESOURCE_OUTPUT_DIR', None)")
    head=subprocess.check_output([*git,'rev-parse','HEAD'],cwd=work,env=env).decode().strip()
    commit=subprocess.check_output([*git,'cat-file','commit',head],cwd=work,env=env)
    assert hashlib.sha1(b'commit '+str(len(commit)).encode()+b'\0'+commit).hexdigest()==head
    tree_id=commit.splitlines()[0].decode().removeprefix('tree ')
    tree=subprocess.check_output([*git,'cat-file','tree',tree_id],cwd=work,env=env)
    assert hashlib.sha1(b'tree '+str(len(tree)).encode()+b'\0'+tree).hexdigest()==tree_id
    # Preserve actual entry commit/tree metadata for fixture owners that read HEAD;
    # this is not a synthetic commit or a clean-source claim.
    anchor="    subprocess.run(['git', 'add', '--all'], cwd=source, check=True)";assert runner.count(anchor)==1
    runner=runner.replace(anchor,anchor+"\n    original_commit=(run/'head.commit').read_bytes()\n    original_tree=(run/'head.tree').read_bytes()\n    expected_tree=original_commit.splitlines()[0].decode().removeprefix('tree ')\n    actual_tree=subprocess.check_output(['git','hash-object','-t','tree','-w','--stdin'],input=original_tree,cwd=source).decode().strip()\n    assert actual_tree==expected_tree\n    actual_commit=subprocess.check_output(['git','hash-object','-t','commit','-w','--stdin'],input=original_commit,cwd=source).decode().strip()\n    assert actual_commit==record['entry_commit']\n    subprocess.run(['git','update-ref','HEAD',actual_commit],cwd=source,check=True)")
    ast.parse(runner)
    snapshot={'entry_commit':head,'source_state':'independently frozen integrated snapshot; entry commit is metadata, not a clean-source claim',
        'native_source':expected,'files':public,'archive_sha256':sha(archive)}
    save(BASE/(args.label+'-snapshot.json'),snapshot);save(BASE/(args.label+'-plan.json'),plan)
    with (BASE/(args.label+'-runner.py')).open('x',encoding='utf-8') as output:output.write(runner)
    for suffix,content in [('-head.commit',commit),('-head.tree',tree)]:
        with (BASE/(args.label+suffix)).open('xb') as output:output.write(content)
    artifacts=[archive,BASE/(args.label+'-snapshot.json'),BASE/(args.label+'-plan.json'),BASE/(args.label+'-runner.py'),BASE/(args.label+'-head.commit'),BASE/(args.label+'-head.tree'),OWNER,LAUNCHER,CONNECTION]
    save(BASE/(args.label+'-prepared.json'),{'phase':'PostP14Portable','prepared_only':True,'ssh_launched':False,
        'label':args.label,'native_source':expected,'public_files':len(public),'steps':len(steps),'expected_tests':sum(row.get('expected_passing_tests',0) for row in steps),
        'prerequisites':{'integration':{'path':str(args.integration_manifest.resolve()),'sha256':args.integration_manifest_sha256},
            'p14_acceptance':{'path':str(args.p14_acceptance.resolve()),'sha256':args.p14_acceptance_sha256},
            'catalog':{'path':str(CATALOG),'sha256':args.catalog_sha256}},
        'pins':{str(path.resolve()):sha(path) for path in artifacts},
        'cleanup':'Exact ownedtarget cargo clean finally, preserve timing/logs, actual exit0 and targetabsence required; failure never qualifies'})
    print(json.dumps({'prepared_only':True,'label':args.label,'source':expected,'steps':len(steps),'tests':sum(row.get('expected_passing_tests',0) for row in steps),
        'launch_later_by_root':f'python {LAUNCHER} launch {args.label}','collect_later_by_root':f'python {LAUNCHER} collect {args.label}'}))
if __name__=='__main__':main()
