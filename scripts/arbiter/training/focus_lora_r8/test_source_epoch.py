# SPDX-License-Identifier: Apache-2.0
"""Independent literal closed-epoch controls. No filesystem model/runtime reads."""
import copy,unittest
from pathlib import Path
import source_epoch as s

LITERAL_PATHS=['Cargo.lock','crates/airk-core/src/arbiter/mod.rs','crates/airk-core/src/arbiter/context_action.rs','crates/airk-core/src/arbiter/context_action/calibration.rs','crates/airk-core/src/arbiter/focus.rs','crates/airk-retrieval/src/arbiter/compiler/focus.rs','crates/airk-retrieval/src/arbiter/runtime/qualification.rs','crates/airk-retrieval/src/arbiter/runtime/qualification/regeneration.rs']


def fixture():
    root=Path.cwd();H='1'*64;F='2'*64
    base={str(root/name):H for name in LITERAL_PATHS}
    base.update({f'unchanged-consumer-{i}':H for i in range(13266)})
    original={'reference_module':{'sha256':H},'versions':{'torch':'literal'},'input_seals':dict(base)}
    held={'repository_root':str(root),'input_seals':base,'reference_module':original['reference_module'],'versions':original['versions'],'native_census':{'path':'census','sha256':'237d7cd102961c80a6004f7fb0b1f1caa2631208829b36c83dc7b812a3be7d33'},'native_owned_receipt':{'path':'receipt','sha256':'8d2d8428674f2a0b76674c3606575e82bcb9afea4e228c31ed9971a2362caf55'},'execution_sources':{'original':{'path':'original','sha256':H}}}
    epoch={'schema':'airk.private.F1-operational-source-epoch/1','held_failed_plan':{'sha256':'6f1cf1c880958c2f8c465495852e6f1bf82253533593081cd1b99ccff511f8f0'},'native_build_receipt':{'sha256':'75aa28c484e8121957f0ee590b0f8376233bf14bedc88dfef88bf1ec0ea5df36'},'native_census':dict(held['native_census']),'native_image':{'sha256':'60b8d3e568054a5fa23007c616b2f6e582d1d5a2718a89484733a39941158665'},'native_owned_receipt':dict(held['native_owned_receipt']),'original_reference_plan':dict(held['execution_sources']['original']),'changes':[{'relative_path':name,'old_sha256':H,'new_sha256':F}for name in LITERAL_PATHS],'unchanged13266_sha256':s.closure_sha({f'unchanged-consumer-{i}':H for i in range(13266)})}
    build={'schema':'airk-readiness-command/1','status':'completed','passed':True,'inputs_unchanged':True,'exit_code':0,'changed_groups':[],'groups':{'native':{'sha256':'dbc89b3cca981d1c87c2340c8697ac65d7f6a844ece6f24bedad336857d3eace','files':{name:F for name in LITERAL_PATHS}}}}
    emission={'native_image':{'sha256':'60b8d3e568054a5fa23007c616b2f6e582d1d5a2718a89484733a39941158665'}}
    return held,original,epoch,build,emission


class EpochControls(unittest.TestCase):
    def test_exact8_advance_all13274_retained_and13266_identical(self):
        args=fixture();before=copy.deepcopy(args);result=s.advance(*args)
        self.assertEqual(len(result),13274);self.assertEqual(set(result),set(args[0]['input_seals']))
        self.assertEqual(sum(result[p]!=args[0]['input_seals'][p]for p in result),8)
        self.assertEqual(args,before)
        for i in range(13266):self.assertEqual(result[f'unchanged-consumer-{i}'],'1'*64)
    def test_missing_extra_duplicate_or_foreign_path_refuses(self):
        for edit in [lambda e:e['changes'].pop(),lambda e:e['changes'].append(dict(e['changes'][0])),lambda e:e['changes'][0].update(relative_path='model.safetensors'),lambda e:e['changes'].__setitem__(0,dict(e['changes'][1]))]:
            args=fixture();edit(args[2])
            with self.assertRaises(ValueError):s.advance(*args)
    def test_noncontext_runtime_checkpoint_math_and_data_drift_refuses(self):
        for index in [0,1,100,13265]:
            args=fixture();args[0]['input_seals'][f'unchanged-consumer-{index}']='3'*64
            with self.assertRaisesRegex(ValueError,'immutable13266'):s.advance(*args)
    def test_arbitrary_live_refresh_and_wrong_old_value_refuse(self):
        for field in ['old_sha256','new_sha256']:
            args=fixture();args[2]['changes'][0][field]='3'*64
            with self.assertRaisesRegex(ValueError,'reviewed old/new'):s.advance(*args)
    def test_build_source_epoch_and_failed_or_stale_build_refuse(self):
        for edit in [lambda b:b.update(passed=False),lambda b:b.update(inputs_unchanged=False),lambda b:b.update(exit_code=1),lambda b:b.update(changed_groups=['native']),lambda b:b['groups']['native'].update(sha256='3'*64)]:
            args=fixture();edit(args[3])
            with self.assertRaises(ValueError):s.advance(*args)
    def test_parent_reference_census_image_and_math_remain_bound(self):
        for edit in [lambda a:a[2]['native_build_receipt'].update(sha256='3'*64),lambda a:a[2]['original_reference_plan'].update(sha256='3'*64),lambda a:a[4]['native_image'].update(sha256='3'*64),lambda a:a[0].update(reference_module={'sha256':'3'*64})]:
            args=fixture();edit(args)
            with self.assertRaises(ValueError):s.advance(*args)

if __name__=='__main__':unittest.main(verbosity=2)
