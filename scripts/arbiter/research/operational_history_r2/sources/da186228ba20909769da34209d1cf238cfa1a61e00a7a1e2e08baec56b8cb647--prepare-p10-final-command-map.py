# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json

root=Path('D:/code/Github/airk')
art=root/'artifacts/v190'
work=art/'p10-worktree'
runtime=art/'p10-runtime-final-r1'
target=art/'p10-target'
py='python'
wrapper=art/'p10-local-command.py'
drivers=['scripts/benchmark-mcp-investigation.py','scripts/check-service-clients.py',
 'scripts/check-publication-faults.py','scripts/check-retrieval-quality.py',
 'scripts/fixtures/retrieval-v190.json','docs/research/baseline-v190.json',
 'scripts/ci-local.ps1','scripts/build-service-runtime.py']
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
driver_proof=[{'path':p,'root_sha256':sha(root/p),'worktree_sha256':sha(work/p),
 'identical':(root/p).read_bytes()==(work/p).read_bytes()} for p in drivers]
assert all(p['identical'] for p in driver_proof)
steps=[]
def add(name,args,env=None,**extra):
    steps.append({'name':name,'cwd':str(work),'environment':env or {},
     'log':str(art/f'p10-final-{name}-r1.log'),
     'command':[py,str(wrapper),str(art/f'p10-final-{name}-r1.log'),*map(str,args)],**extra})
add('fmt',['cargo','fmt','--all','--','--check'],authority='source prerequisite; frozen native identity not yet selected')
add('lightweight-clippy',['cargo','clippy','-p','airk-cli','--no-default-features','--all-targets','--locked','--timings','--','-D','warnings'])
add('workspace-clippy',['cargo','clippy','--workspace','--all-targets','--all-features','--locked','--timings','--','-D','warnings'])
add('lightweight-check',['cargo','check','-p','airk-cli','--no-default-features','--all-targets','--locked','--timings'],authority='required distinct lightweight-main gate retained despite earlier Clippy')
add('release-build',[py,str(work/'scripts/build-service-runtime.py'),'--profile','release','--destination',str(runtime)],
 {'AIRK_DEV_CARGO_INCREMENTAL':'0','CARGO_INCREMENTAL':'0'},
 authority='Build exactly once after final source receipt equality. No debug pair may substitute. Existing target reused; no Windows clean.')
add('split-process-resources',['cargo','test','-p','airk-cli','--test','combined_reads','--all-features','--locked','--timings',
 'split_actual_service_','--','--test-threads=1','--nocapture'],
 {'AIRK_CATALOG_CANDIDATE_BINARY':str(runtime/'airk.exe'),'AIRK_P10_RESOURCE_OUTPUT_DIR':str(art/'p10-resource-final-r1')},
 expected_passing_tests=2,precondition='Create a fresh empty exact artifact directory. Seal release runtime first. Environment applies only to this command.',
 acceptance='Both real process tests plus strict complete >=4 successful same-child-running samples and >=4 baseline/settled samples; cancel metadata-only contention N/A. Preserve failed/refused/indeterminate samples, confirmation timings, full native replies, all source hashes and cleanup/ack evidence.')
add('workspace-tests',['cargo','test','--workspace','--all-features','--locked','--timings'],
 owner='separate sole Cargo owner, after release process/resource stage, no competing native test/build/measurement',
 authority='Collect all test binaries and doctest outcomes. Stop queue if any current run failed; collect complete output first.')
# Keep the unchanged P9 cohort and control algorithms, with explicit P10 outputs.
prior=json.loads((art/'p9-final-runtime-command-map-r5.json').read_text(encoding='utf-8'))
model=art/'p9-cargo-clean-local-ba534967db55/inputs/local-vector-model/minilm-l6-v2-u8-v1'
add('clients',[py,str(work/'scripts/check-service-clients.py'),'--runtime-dir',str(runtime),'--files','16','--model','--report',str(art/'p10-final-clients-r1.json')])
add('publication-faults',[py,str(work/'scripts/check-publication-faults.py'),'--binary',str(runtime/'airk.exe'),'--attempts','6','--output',str(art/'p10-final-publication-faults-r1.json')])
for mode in ('off','report'):
    add(mode,[py,str(art/'p9-frozen-replay.py'),'--runtime-dir',str(runtime),'--previous-runtime',str(art/'p6f-resume-predecessor-shared-copy'),
     '--verification',mode,'--output',str(art/f'p10-final-{mode}-mcp-r1')],
     exclusive_cpu_campaign=True,cohort='unchanged samples=30, concurrency=[1], predecessor upgrade, corpus, controls and model; no concurrent Clippy/build/tests/resource campaign')
add('latency-gates',[py,str(art/'p9-evaluate-gates.py'),'--candidate-off',str(art/'p10-final-off-mcp-r1/replay/report.json'),
 '--candidate-report',str(art/'p10-final-report-mcp-r1/replay/report.json'),'--output',str(art/'p10-final-gates-r1.json')],
 expected='all unchanged 84 canonical comparison gates, no threshold/sample/control edits')
add('relevance',[py,str(work/'scripts/check-retrieval-quality.py'),'--binary',str(runtime/'airk.exe'),
 '--output',str(art/'p10-final-relevance-r1.json'),'--trials','3','--detail','agent_lean',
 '--corpus',str(work/'scripts/fixtures/retrieval-v190.json'),'--model-dir',str(model)],
 expected='all nine unchanged quality cases, three trials; retain exact corpus/source hashes and full observations',exclusive_cpu_campaign=True)
add('quality-comparison',[py,str(art/'p9-compare-quality.py'),'--candidate',str(art/'p10-final-relevance-r1.json'),
 '--output',str(art/'p10-final-quality-comparison-r1.json')])
add('ci-local',['pwsh','-NoProfile','-File',str(work/'scripts/ci-local.ps1'),'-SkipBuild','-RuntimeDir',str(runtime),'-Cores','4','-Repeat','1'],
 expected='all three original suites once; retain nested complete suite/client reports. No second release build.')
helpers=['p10-local-command.py','p9-frozen-replay.py','p9-evaluate-gates.py','p9-compare-quality.py','p9-integrated-checks.py']
result={'schema':'airk.p10.final-qualification-command-map/1','status':'prepared_not_run',
 'cwd':str(work),'runtime':str(runtime),'persistent_target':str(target),'native_source':None,
 'source_freeze':'Select only after metrics corrections pass and final fmt/lightweight Clippy/workspace-allfeatures Clippy/lightweight check receipts agree exactly. Verify build receipt source_before/source_after, runtime airk-build.json source, and all later wrapper receipts match that selected identity. Old afa debug pair and earlier source snapshots are not qualification.',
 'source_receipt_gate':'Before/after every command p10-local-command records source_identity. The sole owner compares each source_before/source_after to frozen source, verifies actual/effective exits and nonzero expected test counts, and stops on first failure after collecting the entire current run. No edits until current run completes.',
 'runtime_seal_gate':{'helper':str(art/'p9-integrated-checks.py'),
  'call':'Import existing helper; seal_runtime(Path(absolute_runtime), frozen_source_expected). Save complete returned files/build/runtime/install inventory as p10-runtime-final-r1-seal.json; assert exact equality before AND after every subsequent stage. Existing retirement hold guard remains authoritative.',
  'release_only':True,'all_inventory':True,'initial_source_expected':'from final successful prerequisites/build, never inferred from runtime filename'},
 'commands':steps,'driver_parity':driver_proof,
 'unchanged_helper_sha256':{name:sha(art/name) for name in helpers},
 'root_driver_use':'Root-owned frozen replay/evaluator/quality helpers retain their existing ROOT and fixed P5/control/P8/predecessor references. Driver bytes they import were verified byte-identical with isolated tree above; recheck recorded hashes before launch. Direct candidate script paths point into worktree. All command and output paths absolute.',
 'environment':'Wrapper retains existing verified offline model/runtime assets, explicit owned ignore config and private fixture roots. Per-command AIRK_DEV_CARGO_INCREMENTAL overrides its default; release MUST set both incremental variables to0. Dev/test Windows commands retain1. No routine cargo clean Windows. Neo campaign is separate later and must unconditionally cargo clean afterward.',
 'not_launched':['no Cargo/native/build/test/process/latency/Neo command launched by preparing this map','no source snapshot taken','no runtime sealed yet','no full acceptance or P11-P14 completion inferred'],
 'resource_summary_commands':[[py,str(art/'p10-resource-campaign.py'),'--summarize',str(art/f'p10-resource-final-r1/{case}/observations.jsonl'),
 '--output',str(art/f'p10-resource-final-r1/{case}/summary.json')] for case in ('complete','cancel')],
 'source_map_reference':str(art/'p9-final-runtime-command-map-r5.json'),
 'cost_controls':['One fresh release pair only, reused every runtime stage.','All latency/quality runs serial and CPU-exclusive.','One Cargo owner; collect all results before source corrections.','Full workspace testing is required and is a separate owned stage, not implied by focused tests.','Do not invoke p9-integrated main runtime campaign: it rebuilds and repeats more broadly.']}
out=art/'p10-final-qualification-command-map-r1.json'
assert not out.exists()
out.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'output':str(out),'commands':len(steps),'driver_matches':len(driver_proof),'sha256':sha(out)}))
