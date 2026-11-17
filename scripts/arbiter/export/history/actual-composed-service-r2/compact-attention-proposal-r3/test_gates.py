# SPDX-License-Identifier: Apache-2.0
"""No ONNX/Torch/ORT imports or graph creation. Exercise guard control flow."""
import struct,tempfile,unittest
from pathlib import Path
from types import SimpleNamespace
from gate_protocol import cache_contract,dispatch_with_cache,infer_complete,original_external_context,fresh_child_cwd,external_save_plan

class Gates(unittest.TestCase):
    def setUp(self):
        self.ns='/airk_compact_gqa/'
        self.inputs=['q','k','v','','',self.ns+'seqlen32',self.ns+'total32','','','','/scorer/Where_output_0']
        self.outputs=['context','presentk','presentv']
        self.attrs=dict(num_heads=16,kv_num_heads=8,causal=1,local_window_size=-1,do_rotary=0,softcap=0.,smooth_softmax=0,scale=struct.unpack('<f',bytes.fromhex('f304b53d'))[0])
    def known(self,t):return {'q':(1,[1,t,2048]),'k':(1,[1,t,1024]),'v':(1,[1,t,1024]),self.inputs[10]:(1,[1,1,t,t]),self.inputs[5]:(6,[1]),self.inputs[6]:(6,[1])}
    def contract(self,t=17):return cache_contract(self.inputs,self.outputs,self.attrs,self.known(t),t,self.ns)
    def test_no_past_fills_before_global_completeness(self):
        state={};order=[]
        def stock():state.update(context=(1,[1,17,2048]),presentk=(0,[]),presentv=(0,[]));order.append('stock')
        def publish(n,d,s):state[n]=(d,s);order.append(n)
        def global_check():
            self.assertEqual(state,{'context':(1,[1,17,2048]),'presentk':(1,[1,8,17,128]),'presentv':(1,[1,8,17,128])})
        stock()
        with self.assertRaises(AssertionError):global_check() # Exact prior failure.
        dispatch_with_cache(self.contract,stock,publish);global_check()
        self.assertEqual(order[-3:],['stock','presentk','presentv'])
    def test_refusal_prevents_stock_dispatch(self):
        self.attrs['scale']=1.;called=[]
        with self.assertRaises(ValueError):dispatch_with_cache(self.contract,lambda:called.append(1),lambda *a:None)
        self.assertEqual(called,[])
    def test_all_cases_have_exact_complete_types(self):
        for t in [1,17,128,8192]:self.assertEqual(self.contract(t),{'presentk':(1,[1,8,t,128]),'presentv':(1,[1,8,t,128])})
        for mutation in [(0,[1,17,1024]),(1,[1,'T',1024]),(1,[1,8,17,128])]:
            known=self.known(17);known['k']=mutation
            with self.assertRaises(ValueError):cache_contract(self.inputs,self.outputs,self.attrs,known,17,self.ns)
    def test_mask_past_and_window_changes_refused(self):
        for index,value in [(3,'past'),(5,'mask_sum'),(10,'dropped_mask')]:
            changed=list(self.inputs);changed[index]=value
            with self.assertRaises(ValueError):cache_contract(changed,self.outputs,self.attrs,self.known(17),17,self.ns)
        self.attrs['local_window_size']=512
        with self.assertRaises(ValueError):self.contract()
    def test_global_incomplete_never_accepted(self):
        engine=SimpleNamespace(run_=True,out_mp_='inference-copy')
        def step():engine.run_=False;return False
        engine._infer_impl=step;engine._update_output_from_vi=lambda:None
        with self.assertRaises(ValueError):infer_complete(engine)
        engine.run_=True
        def done():engine.run_=False;return True
        engine._infer_impl=done;self.assertEqual(infer_complete(engine),'inference-copy')
    def test_checker_real_external_context_restores_on_failure(self):
        previous=Path.cwd()
        with tempfile.TemporaryDirectory(dir=previous) as tmp:
            root=Path(tmp);assets=root/'assets';out=root/'out';assets.mkdir();out.mkdir()
            source=assets/'source.original';source.write_bytes(b'original metadata fixture')
            (assets/'actual.bin').write_bytes(b'actual fixture bytes')
            try:
                import os;os.chdir(out)
                with self.assertRaisesRegex(RuntimeError,'checker refusal'):
                    with original_external_context(source,out):
                        self.assertEqual(Path('actual.bin').read_bytes(),b'actual fixture bytes')
                        raise RuntimeError('checker refusal')
                self.assertEqual(Path.cwd(),out)
            finally:os.chdir(previous)
    def test_child_uses_fresh_output_not_frozen_source(self):
        with tempfile.TemporaryDirectory(dir=Path.cwd()) as tmp:
            p=Path(tmp);base=p/'proposal';out=p/'output';base.mkdir();out.mkdir()
            self.assertEqual(fresh_child_cwd(out,base),out.resolve())
            with self.assertRaises(ValueError):fresh_child_cwd(base,base)
    def test_externalized_save_choices_and_freshness(self):
        with tempfile.TemporaryDirectory(dir=Path.cwd()) as tmp:
            graph=Path(tmp)/'optimized.onnx';external,entries=external_save_plan(graph)
            self.assertEqual(external.name,'optimized.weights.bin')
            self.assertEqual(entries,{'session.optimized_model_external_initializers_file_name':'optimized.weights.bin','session.optimized_model_external_initializers_min_size_in_bytes':'1024'})
            observed={}
            for key,value in entries.items():observed[key]=value # Fake configurable builder receives actual planned options.
            self.assertNotEqual(observed.get('session.optimized_model_external_initializers_file_name',''),'')
            external.write_bytes(b'occupied')
            with self.assertRaises(ValueError):external_save_plan(graph)
if __name__=='__main__':unittest.main()