# SPDX-License-Identifier: Apache-2.0
"""Close a complete workspace selection while preserving exact prior run identities."""
import hashlib,json,pathlib,subprocess,sys,xml.etree.ElementTree as ET
BASE=pathlib.Path(__file__).resolve().parent
ROOT=BASE.parents[3]
def read(name): return json.loads((BASE/name).read_bytes())
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def passed(name):
    root=ET.parse(BASE/name).getroot()
    rows=set()
    for suite in root.findall('testsuite'):
        for case in suite.findall('testcase'):
            if not any(case.find(tag) is not None for tag in ('failure','error','skipped')):
                rows.add((suite.attrib['name'],case.attrib['name']))
    return rows
def write(name,value):
    with (BASE/name).open('x',encoding='utf-8') as stream:
        json.dump(value,stream,indent=2);stream.write('\n')
def prepare():
    old=read('workspace-final-tests-r2.json')
    current=read('workspace-completion-inventory-r1.json')
    assert old['selected_tests']==4748 and old['exit_code']==100 and not old['passed']
    for value in (old,current):
        assert value['inputs_unchanged'] and value['runner_unchanged'] and value['runtime_unchanged'] and value['command_inputs_unchanged'] and not value['changed_groups']
    assert old['groups']['fixtures']==current['groups']['fixtures']
    oldfiles=old['groups']['native']['files'];newfiles=current['groups']['native']['files']
    changed={key:{'old':oldfiles.get(key),'new':newfiles.get(key)} for key in oldfiles.keys()|newfiles.keys() if oldfiles.get(key)!=newfiles.get(key)}
    assert set(changed)=={'crates/airkctl/tests/error_outcomes.rs'},changed
    assert changed['crates/airkctl/tests/error_outcomes.rs']['new']=='5c5884794b3bd6bfd83f1a081e21c799f6ff9f74fc18b3dfb4385fe6a4f04b9b'
    inventory=next(json.loads(line) for line in (BASE/'workspace-completion-inventory-r1.log').read_text(encoding='utf-8').splitlines() if line.startswith('{"rust-build-meta"'))
    alltests={(key,name) for key,suite in inventory['rust-suites'].items() for name,case in suite['testcases'].items() if not case['ignored']}
    prior=passed('workspace-final-tests-r2-junit.xml')
    assert len(alltests)==4748 and len(prior)==3397 and prior<=alltests
    # Rerun all tests in any incomplete binary, including the complete corrected binary.
    binaries=sorted({key for key,name in alltests-prior})
    selected={(key,name) for key,name in alltests if key in binaries}
    assert alltests<=prior|selected
    expression=' | '.join('binary_id(='+name+')' for name in binaries)
    assert len(expression)<30000
    plan={'schema':'airk.private.workspace-selection-completion/1','source_change':changed,
        'scope':'Reused original unchanged passing test identities; only error_outcomes test command/comment changed. No production source or existing assertions changed.',
        'prior_receipt':{'path':'workspace-final-tests-r2.json','sha256':sha(BASE/'workspace-final-tests-r2.json')},
        'inventory_receipt':{'path':'workspace-completion-inventory-r1.json','sha256':sha(BASE/'workspace-completion-inventory-r1.json')},
        'native_group':current['groups']['native']['sha256'],'fixtures_group':current['groups']['fixtures']['sha256'],
        'all_tests':sorted(alltests),'prior_passed':sorted(prior),'next_selected':sorted(selected),
        'binaries':binaries,'filterset':expression,'all_count':len(alltests),'prior_pass_count':len(prior),
        'next_count':len(selected),'unique_unexecuted_or_failed_count':len(alltests-prior),
        'complete':False,'failed_original_run_preserved':True}
    write('workspace-completion-plan-r1.json',plan)
    print(json.dumps({key:plan[key] for key in ('all_count','prior_pass_count','next_count','unique_unexecuted_or_failed_count','filterset')}))
def run():
    plan=read('workspace-completion-plan-r1.json')
    command=[sys.executable,'-B',str(ROOT/'scripts/dev/receipt_runner.py'),'--groups','native,fixtures','--tier','T2','--out',str(BASE),'--target-dir',str(ROOT/'target'),'--fixture-base','C:/Users/tdotn/AppData/Local/airk-final-exec-20261116T052942Z/temp','workspace-final-completion-r1','cargo','nextest','run','--profile','ci','--workspace','--all-features','--locked','--timings','--ignore-default-filter','--max-fail','1:wait','-E',plan['filterset']]
    raise SystemExit(subprocess.run(command,cwd=ROOT).returncode)
def verify():
    plan=read('workspace-completion-plan-r1.json');receipt=read('workspace-final-completion-r1.json')
    assert receipt['passed'] and receipt['selected_tests']==plan['next_count'] and receipt['inputs_unchanged']
    assert receipt['groups']['native']['sha256']==plan['native_group'] and receipt['groups']['fixtures']['sha256']==plan['fixtures_group']
    old={tuple(x) for x in plan['prior_passed']};new=passed('workspace-final-completion-r1-junit.xml');alltests={tuple(x) for x in plan['all_tests']}
    assert new=={tuple(x) for x in plan['next_selected']}
    assert old|new==alltests
    result={**plan,'complete':True,'completed_receipt':{'path':'workspace-final-completion-r1.json','sha256':sha(BASE/'workspace-final-completion-r1.json')},'unique_passed_count':len(old|new),'nonpassing_original_receipts_remain_nonpassing':True}
    write('workspace-completion-result-r1.json',result)
    print(json.dumps({'complete':True,'unique_passed_count':len(old|new),'next_passed':len(new)}))
{'prepare':prepare,'run':run,'verify':verify}[sys.argv[1]]()