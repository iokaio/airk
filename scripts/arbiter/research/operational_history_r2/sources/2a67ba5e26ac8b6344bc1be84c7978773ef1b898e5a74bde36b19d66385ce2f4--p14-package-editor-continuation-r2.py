# SPDX-License-Identifier: Apache-2.0
"""Continue only private editor stages using existing exact package/compile passes.
No native build, npm compile, package rebuild, normal editor/update action or publication.
"""
import hashlib,importlib.util,json,os,subprocess,sys
from pathlib import Path
BASE=Path(__file__).resolve().parent
WORK=BASE/'p10-worktree'
RUNTIME=BASE/'p14-runtime-final-r2'
EXPECTED={'sha256':'5d8912b117b410306ac9a3b44af6d96d70bb018006c2d49a1efa365597fe3e11','files':960}
RECORD=BASE/'p14-package-editor-continuation-r2.json'
PREPARED=BASE/'p14-package-editor-continuation-r2-prepared.json'
STOCK=BASE.parent/'tools/vscode-1.145.0-a717-r1'
LEGACY=BASE/'p14-legacy-upgrade-editor-owner-r2.py'
LEGACY_OWNER=WORK/'artifacts/p14-legacy-upgrade-editor-owner-r2'
def read(p):return json.loads(Path(p).read_text(encoding='utf-8'))
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load(name,p):
    spec=importlib.util.spec_from_file_location(name,p);value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
def main():
    assert not RECORD.exists() and not LEGACY_OWNER.exists(),'Fresh continuation and legacy fixture required'
    prepared=read(PREPARED);assert sha(Path(__file__))==prepared['owner_sha256']
    assert all(sha(Path(p))==expected for p,expected in prepared['pins'].items()),'Reviewed input changed'
    previous=read(BASE/'p14-package-editor-owner-r2.json')
    assert previous['status']=='failed' and previous['native_source']==EXPECTED
    assert [row['name'] for row in previous['steps']]==['native','extension-compile','vsix','five-private-scenarios']
    assert previous['steps'][3]['exit_code']==1
    adopted=[]
    for row in previous['steps'][:3]:
        receipt=Path(row['receipt']);value=read(receipt)
        assert row['exit_code']==value['exit_code']==value['actual_exit_code']==0 and value['validation_error'] is None
        assert sha(receipt)==row['receipt_sha256']
        assert value['source_before']==value['source_after']==EXPECTED and value['source_unchanged'] is True
        log=receipt.with_name(receipt.name.replace('.receipt.json','.log'));assert sha(log)==value['log_sha256']
        adopted.append(row)
    prior_owner=load('p14_prior_package_owner',BASE/'p14-package-editor-owner-r3.py')
    ui=read(BASE/'p14-package-editor-owner-r3-inputs.json')
    assert len(ui)==256 and prior_owner.extension_inputs()==ui==previous['extension_input_hashes']
    os.environ.update(PYTHONUTF8='1',CARGO_NET_OFFLINE='true',
        AIRK_PRIVATE_CENSUS_BASE=str(BASE/'p10-private-fixtures-619c272e8a644988b26bbe4334fc8c02'),
        GIT_CONFIG_GLOBAL=str(BASE/'p10-owned-ignore-controls/global.gitconfig'))
    builder=load('p14_continuation_builder',WORK/'scripts/build-service-runtime.py')
    sealer=load('p14_continuation_sealer',BASE/'p9-integrated-checks.py')
    seal=read(BASE/'p14-runtime-final-r2-seal.json')
    release=read(BASE/'p14-release-runtime-r2.receipt.json');producer_sha=sha(BASE/'p14-release-runtime-r2.receipt.json')
    assert release['exit_code']==release['actual_exit_code']==0 and release['source_unchanged'] is True
    assert release['source_before']==release['source_after']==EXPECTED
    assert sha(BASE/'p14-release-runtime-r2.log')==release['log_sha256']
    assert producer_sha==previous['producer_receipt_sha256']
    stock=read(STOCK/'receipt.json');stock_seal=read(STOCK/'stock-file-seal.json')
    metadata=read(STOCK/'official-metadata.json')
    assert stock['archive_sha256']=='79cf4cc5b17357cc64abdd22a2d7d4884d05f07b3f0defbf5fc0d67fe03d9dc4'
    assert sha(STOCK/'official.zip')==stock['archive_sha256']
    assert stock['version']=='1.145.0' and stock['normal_editor_updater_untouched'] is True
    assert stock['commit']==metadata['version'] and stock['archive_sha256']==metadata['sha256hash']
    assert stock['files_verified_against_official_archive']==len(stock_seal)==3280
    assert sha(STOCK/'stock-file-seal.json')==stock['stock_file_seal_sha256']
    stock_runtime=STOCK/'runtime';editor=Path(stock['editor']);cli=Path(stock['cli'])
    assert editor.resolve()==(stock_runtime/'Code.exe').resolve()
    app=stock_runtime/stock['commit'][:10]/'resources/app'
    assert cli.resolve()==(app/'out/cli.js').resolve()
    assert sha(editor)==stock['editor_sha256']
    vsix=previous['vsix'];assert vsix['sha256']=='49170b51765048e438dfd9516a35e4884d7667efe5ad2652d02303caf22f4c90'
    native_install=previous['native_install_sha256']
    output=WORK/'artifacts/p14-package-editor-r2'
    native_manifest=read(output/'native/publish/manifest.json')
    native_zip=output/'native/publish'/native_manifest['archive']
    assert native_manifest['source']==EXPECTED and native_manifest['signed_distribution'] is False
    assert native_manifest['install_manifest_sha256']==native_install and sha(native_zip)==native_manifest['archive_sha256']
    extension_manifest=read(output/'extension/publish/manifest.json')
    assert sha(output/'extension/publish/manifest.json')==vsix['manifest_sha256']
    assert extension_manifest['native_manifest_sha256']==sha(output/'native/publish/manifest.json')
    assert extension_manifest['native_archive_sha256']==native_manifest['archive_sha256']
    drivers=dict(previous['driver_hashes'])
    adoption=read(BASE/'p14-editor-tools-live-adoption-r1.json')
    assert adoption['native_source_unchanged']==EXPECTED and adoption['existing_vsix_unchanged']==vsix['sha256']
    changed={row['path']:row for row in adoption['files'] if row['path'] in drivers}
    assert set(changed)=={'scripts/editor-ui.cjs','scripts/check-native-editor.cjs'}
    for name,row in changed.items():
        assert drivers[name]==row['base_sha256']
        assert sha(WORK/name)==row['proposed_sha256']
        drivers[name]=row['proposed_sha256']
    cleanup=read(BASE/'p14-editor-failed-fixture-cleanup-r2.json')
    assert cleanup['qualified'] is True and cleanup['lifecycle_stop_claimed'] is False and cleanup['ui_stop_receipt_created'] is False
    prior_attempt=read(BASE/'p14-package-editor-continuation-r1.json');assert prior_attempt['status']=='failed' 
    record={'status':'running','native_source':EXPECTED,'prior_failed_owner':str(BASE/'p14-package-editor-owner-r2.json'),
        'prior_failed_owner_sha256':sha(BASE/'p14-package-editor-owner-r2.json'),'adopted_passed_stages':adopted,
        'adoption_scope':'Only three passed package/compile stages adopted; original failed editor owner remains failed.',
        'producer_receipt_sha256':producer_sha,'runtime_seal_sha256':sha(BASE/'p14-runtime-final-r2-seal.json'),
        'vsix':vsix,'native_install_sha256':native_install,'stock_receipt':str(STOCK/'receipt.json'),
        'stock_receipt_sha256':sha(STOCK/'receipt.json'),'extension_input_hashes':ui,'driver_hashes':drivers,'steps':[],
        'limits':'Unmodified official same-commit ZIP editor and private profiles only. No normal editor/updater action, artifact rebuild, publication or final post-recovery installer claim.'}
    record['prior_failed_continuation']={'path':str(BASE/'p14-package-editor-continuation-r1.json'),'sha256':sha(BASE/'p14-package-editor-continuation-r1.json'),'status':'failed'}
    record['prior_native_cleanup']={'path':str(BASE/'p14-editor-failed-fixture-cleanup-r2.json'),'sha256':sha(BASE/'p14-editor-failed-fixture-cleanup-r2.json'),'scope':'native quiescence only; no lifecycle/UI Stop receipt claimed'}
    record['reviewed_driver_adoption']={'path':str(BASE/'p14-editor-tools-live-adoption-r1.json'),'sha256':sha(BASE/'p14-editor-tools-live-adoption-r1.json'),'changed_drivers':changed}
    def save():RECORD.write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
    def invariant():
        assert all(sha(Path(p))==expected for p,expected in prepared['pins'].items())
        assert builder.source_identity()==EXPECTED
        assert prior_owner.extension_inputs()==ui
        assert all(sha(WORK/p)==expected for p,expected in drivers.items())
        assert sha(BASE/'p14-release-runtime-r2.receipt.json')==producer_sha
        assert sha(BASE/'p14-release-runtime-r2.log')==release['log_sha256']
        assert sealer.seal_runtime(RUNTIME,EXPECTED)==seal
        assert sha(Path(vsix['path']))==vsix['sha256']
        assert sha(native_zip)==native_manifest['archive_sha256']
        assert sha(RUNTIME/'airk-install.json')==native_install
        assert sha(STOCK/'receipt.json')==record['stock_receipt_sha256']
        assert sha(STOCK/'stock-file-seal.json')==stock['stock_file_seal_sha256']
        assert not(stock_runtime/'data').exists(),'Stock ZIP must not become a portable normal user profile'
        assert read(app/'package.json')['version']==stock['version']
        product=read(app/'product.json');assert product['commit']==stock['commit'] and 'target' not in product
        actual={p.relative_to(stock_runtime).as_posix() for p in stock_runtime.rglob('*') if p.is_file()}
        assert actual==set(stock_seal),'Stock editor file inventory changed'
        for name,expected in stock_seal.items():
            path=stock_runtime/name
            assert not path.is_symlink() and path.resolve().is_relative_to(stock_runtime.resolve()) and sha(path)==expected,name
    def run(name,command,extra=None):
        invariant();env=os.environ.copy()
        for key in ['AIRK_EDITOR_LEGACY_FIXTURE','AIRK_EDITOR_SCENARIOS','AIRK_TEST_BINARY','AIRK_CATALOG_CANDIDATE_BINARY','AIRK_P10_RESOURCE_OUTPUT_DIR','AIRK_P11B_RESOURCE_OUTPUT_DIR']:env.pop(key,None)
        env.update(extra or {})
        label='p14-package-continuation-'+name+'-r2.log';assert not(BASE/label).exists()
        result=subprocess.run([sys.executable,'-X','utf8',str(BASE/'p11-local-command.py'),label,*command],cwd=WORK,env=env)
        receipt=BASE/label.replace('.log','.receipt.json');value=read(receipt) if receipt.exists() else None
        record['steps'].append({'name':name,'exit_code':result.returncode,'receipt':str(receipt),'receipt_sha256':sha(receipt) if receipt.exists() else None});save()
        assert result.returncode==0 and value is not None,name
        assert value['command']==command and value['exit_code']==value['actual_exit_code']==0
        assert value['source_before']==value['source_after']==EXPECTED and value['source_unchanged'] is True
        assert sha(BASE/label)==value['log_sha256'];invariant()
    def installed(name,scenarios,extra=None):
        root=WORK/'artifacts/editor-qualification';before=set(root.iterdir()) if root.exists() else set()
        run(name,['node',str(WORK/'scripts/check-editor-windows.cjs'),str(editor),str(cli),vsix['path']],
            {'AIRK_EDITOR_SCENARIOS':','.join(scenarios),**(extra or {})})
        created=set(root.iterdir())-before;assert len(created)==1
        summary_path=created.pop()/'summary.json';summary=read(summary_path)
        assert summary['status']=='passed' and summary['qualifiesFullScenarioSelection'] is True
        assert summary['selectedScenarios']==scenarios and summary['staged'] is False
        assert Path(summary['source']).resolve()==Path(vsix['path']).resolve()
        assert summary['nativeInstallDigest']==native_install
        assert {row['scenario'] for row in summary['scenarios']}==set(scenarios)
        assert len(summary['scenarios'])==sum(2 if value in ['same','different'] else 1 for value in scenarios)
        assert all(row['installed'] is True and row['version']=='1.8.8' for row in summary['scenarios'])
        assert summary['legacyUpgradeCoverage'] is ('legacy' in scenarios)
        record.setdefault('installed',[]).append({'scenarios':scenarios,'summary':str(summary_path),'sha256':sha(summary_path),'vsix_sha256':vsix['sha256']});save()
    invariant();save()
    try:
        installed('five-private-scenarios',['same','different','multi','usage','untrusted'])
        run('legacy-fixture',['python',str(LEGACY)])
        fixture=read(LEGACY_OWNER/'fixture.json')
        assert fixture['candidate']['install_manifest_sha256']==native_install
        assert fixture['source_hashes_before']==fixture['source_hashes_after']
        record['legacy_fixture']={'path':str(LEGACY_OWNER/'fixture.json'),'sha256':sha(LEGACY_OWNER/'fixture.json'),'clone_sha256':sha(LEGACY),'vsix_sha256':vsix['sha256']};save()
        installed('legacy-private-scenario',['legacy'],{'AIRK_EDITOR_LEGACY_FIXTURE':str(LEGACY_OWNER/'fixture.json')})
        invariant();assert sha(STOCK/'official.zip')==stock['archive_sha256']
        record['status']='passed';save()
    except BaseException as error:
        record['status']='failed';record['failure']=repr(error);save();raise
if __name__=='__main__':main()
